use {
    bytes::Bytes , 
    echo_codec::{
        aac::{self ,  AacCoder} , 
        flv ,  FormatReader ,  FormatWriter , 
    } , 
    echo_types::{MediaSample ,  Timestamp} , 
    rml_rtmp::{
        handshake::{Handshake ,  HandshakeProcessResult ,  PeerType} , 
        sessions::{ServerSession ,  ServerSessionConfig ,  ServerSessionEvent ,  ServerSessionResult} , 
    } , 
    std::convert::TryFrom , 
    thiserror::Error , 
};

const ADTS_FRAME_SAMPLES: u32 = 1024;

#[derive(Error ,  Debug)]
pub enum Error {
    #[error("RTMP handshake failed")]
    HandshakeFailed , 

    #[error("RTMP session initialization failed")]
    SessionInitializationFailed , 

    #[error("Tried to use RTMP session while not initialized")]
    SessionNotInitialized , 

    #[error("Received invalid input")]
    InvalidInput , 

    #[error("RTMP request was not accepted")]
    RequestRejected , 

    #[error("No stream ID")]
    NoStreamId , 

    #[error("Application name cannot be empty")]
    EmptyAppName , 

    #[error("flv error {0}")]
    FlvError(#[from] echo_codec::flv::FlvError) , 

    #[error("aac error {0}")]
    AacError(#[from] echo_codec::aac::AacError) , 
}

pub enum Event {
    ReturnData(Bytes) , 
    SendSample(MediaSample) , 
    AcquireSession {
        app_name: String , 
        stream_key: String , 
    } , 
    ReleaseSession , 
    LeaveSession , 
}

enum State {
    HandshakePending , 
    Ready , 
    Publishing , 
    Finished , 
}

pub struct RtmpHandle {
    state: State , 
    return_queue: Vec<Event> , 
    handshake: Handshake , 
    session: Option<ServerSession> , 
    aac_coder: AacCoder , 
    sample_rate: u32 , 
    channels: u8 , 
    frame_count: u32 , 
}

impl RtmpHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_bytes(&mut self ,  input: &[u8]) -> Result<Vec<Event> ,  Error> {
        match &mut self.state {
            State::HandshakePending => {
                self.perform_handshake(input)?;
            }
            _ => {
                self.handle_input(input)?;
            }
        }

        Ok(self.return_queue.drain(..).collect())
    }

    fn handle_input(&mut self ,  input: &[u8]) -> Result<() ,  Error> {
        let results = self
            .session()?
            .handle_input(input)
            .map_err(|_| Error::InvalidInput)?;
        self.handle_results(results)?;
        Ok(())
    }

    fn perform_handshake(&mut self ,  input: &[u8]) -> Result<() ,  Error> {
        let result = self
            .handshake
            .process_bytes(input)
            .map_err(|_| Error::HandshakeFailed)?;

        match result {
            HandshakeProcessResult::InProgress { response_bytes } => {
                self.emit(Event::ReturnData(response_bytes.into()));
            }
            HandshakeProcessResult::Completed {
                response_bytes , 
                remaining_bytes , 
            } => {
                log::info!("RTMP handshake successful");
                if !response_bytes.is_empty() {
                    self.emit(Event::ReturnData(response_bytes.into()));
                }

                self.initialize_session()?;

                if !remaining_bytes.is_empty() {
                    self.handle_input(&remaining_bytes)?;
                }

                self.state = State::Ready;
            }
        }

        Ok(())
    }

    fn initialize_session(&mut self) -> Result<() ,  Error> {
        let config = ServerSessionConfig::new();
        let (session ,  results) =
            ServerSession::new(config).map_err(|_| Error::SessionInitializationFailed)?;
        self.session = Some(session);
        self.handle_results(results)
    }

    fn accept_request(&mut self ,  id: u32) -> Result<() ,  Error> {
        let results = {
            let session = self.session()?;
            session
                .accept_request(id)
                .map_err(|_| Error::RequestRejected)?
        };
        self.handle_results(results)
    }

    fn handle_results(&mut self ,  results: Vec<ServerSessionResult>) -> Result<() ,  Error> {
        for result in results {
            match result {
                ServerSessionResult::OutboundResponse(packet) => {
                    self.emit(Event::ReturnData(packet.bytes.into()));
                }
                ServerSessionResult::RaisedEvent(event) => {
                    self.handle_event(event)?;
                }
                ServerSessionResult::UnhandleableMessageReceived(_) => () , 
            }
        }

        Ok(())
    }

    fn handle_event(&mut self ,  event: ServerSessionEvent) -> Result<() ,  Error> {
        use ServerSessionEvent::*;

        match event {
            ConnectionRequested {
                request_id , 
                app_name , 
                ..
            } => {
                if app_name.is_empty() {
                    return Err(Error::EmptyAppName);
                }

                self.accept_request(request_id)?;
            }
            PublishStreamRequested {
                request_id , 
                app_name , 
                stream_key , 
                ..
            } => {
                self.emit(Event::AcquireSession {
                    app_name , 
                    stream_key , 
                });
                self.accept_request(request_id)?;
                self.state = State::Publishing;
            }
            PublishStreamFinished { .. } => {
                self.emit(Event::LeaveSession);
                self.emit(Event::ReleaseSession);
                self.state = State::Finished;
            }
            AudioDataReceived {
                data ,  /* timestamp ,  */
                ..
            } => {
                let flv = flv::tag::AudioData::try_from(data.as_ref())?;

                if flv.is_sequence_header() {
                    match self.aac_coder.set_asc(flv.body.as_ref()) {
                        Ok(asc) => {
                            if let Some(freq) = asc.sampling_frequency {
                                self.sample_rate = freq;
                            }
                            self.channels = asc.channel_configuration.into();
                        }
                        Err(err) => {
                            log::error!("audio configuration error: {}" ,  err);
                        }
                    }
                    return Ok(());
                }

                let aac = match self.aac_coder.read_format(aac::Raw ,  &flv.body)? {
                    Some(raw_aac) => self
                        .aac_coder
                        .write_format(aac::AudioDataTransportStream ,  raw_aac)? , 
                    None => return Ok(()) , 
                };

                // XXX sid
                let sample = MediaSample::new_aac_audio(
                    0 , 
                    self.sample_rate , 
                    self.channels , 
                    Timestamp::new(
                        ADTS_FRAME_SAMPLES as u64 * self.frame_count as u64 , 
                        self.sample_rate as u64 , 
                    ) , 
                    aac , 
                );
                self.emit(Event::SendSample(sample));
                self.frame_count += 1;
            }
            VideoDataReceived { .. } => {
                // ignore video data
            }
            StreamMetadataChanged { .. } => {
                // ignore meta data
            }
            _ => {}
        }

        Ok(())
    }

    fn emit(&mut self ,  event: Event) {
        self.return_queue.push(event);
    }

    fn session(&mut self) -> Result<&mut ServerSession ,  Error> {
        self.session.as_mut().ok_or(Error::SessionNotInitialized)
    }
}

impl Default for RtmpHandle {
    fn default() -> Self {
        Self {
            state: State::HandshakePending , 
            return_queue: Vec::with_capacity(8) , 
            handshake: Handshake::new(PeerType::Server) , 
            session: None , 
            aac_coder: AacCoder::new() , 
            sample_rate: 48_000 , 
            channels: 2 , 
            frame_count: 0 , 
        }
    }
}
