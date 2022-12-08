use {
    crate::{adts_demuxer::AdtsDemuxer ,  demuxer::Demuxer ,  Error} , 
    echo_core::session::{AppName ,  InputQuality ,  MediaMessage ,  SessionHandle} , 
    srt_tokio::SrtSocket , 
    std::{
        sync::Arc , 
        time::{Duration ,  Instant} , 
    } , 
    tokio::{
        stream::StreamExt , 
        sync::{mpsc ,  oneshot ,  RwLock} , 
        time::timeout , 
    } , 
};

pub const MAX_READ_INTERVAL: u64 = 500_000;

pub(crate) type CloseRequester = oneshot::Sender<()>;
pub(crate) type CloseAccepter = oneshot::Receiver<()>;

pub(crate) fn close_request_channel() -> (CloseRequester ,  CloseAccepter) {
    oneshot::channel()
}

pub(crate) type CloseSender = mpsc::UnboundedSender<Option<InputQuality>>;
pub(crate) type CloseReceiver = mpsc::UnboundedReceiver<Option<InputQuality>>;

pub(crate) fn close_event_channel() -> (CloseSender ,  CloseReceiver) {
    mpsc::unbounded_channel()
}

pub(crate) struct SrtReceiver {
    name: AppName , 
    port: u16 , 
    socket: SrtSocket , 
    read_timeout: Duration , 
    demuxer: Arc<RwLock<Box<dyn Demuxer + Send + Sync + 'static>>> , 
    session_handle: SessionHandle , 
    close_accepter: CloseAccepter , 
    close_sender: CloseSender , 
}

impl SrtReceiver {
    pub(crate) fn new(
        sid: u32 , 
        name: &str , 
        port: u16 , 
        socket: SrtSocket , 
        read_timeout: Duration , 
        session_handle: SessionHandle , 
        close_accepter: CloseAccepter , 
        close_sender: CloseSender , 
    ) -> Self {
        Self {
            name: name.to_string() , 
            port , 
            socket , 
            read_timeout , 
            demuxer: Arc::new(RwLock::new(Box::new(AdtsDemuxer::new(sid ,  name)))) , 
            session_handle , 
            close_accepter , 
            close_sender , 
        }
    }

    pub(crate) async fn run(mut self) {
        log::info!(
            "{} {} new sender connected from {}" , 
            self.name , 
            self.port , 
            self.socket.settings().remote
        );

        let mut has_started = false;
        let mut got_close = false;
        let mut is_closed = false;
        let mut starving_time = None;

        while !is_closed {
            if !got_close {
                match self.close_accepter.try_recv() {
                    Ok(_) => {
                        got_close = true;
                    }
                    Err(err) => match err {
                        oneshot::error::TryRecvError::Closed => {
                            got_close = true;
                        }
                        _ => {} // oneshot::error::TryRecvError::Empty
                    } , 
                }
            }

            match timeout(
                Duration::from_micros(MAX_READ_INTERVAL) , 
                self.handle_socket(got_close) , 
            )
            .await
            {
                Ok(res) => {
                    match res {
                        Ok(_) => {
                            if !has_started {
                                has_started = true;
                            }
                        }
                        Err(err) => {
                            match err {
                                Error::SrtDisconnected => {
                                    log::info!("{} {} srt disconnected" ,  self.name ,  self.port);
                                }
                                _ => {
                                    log::error!(
                                        "{} {} srt packet handle error: {:?}" , 
                                        self.name , 
                                        self.port , 
                                        err
                                    );
                                }
                            }
                            is_closed = true;
                        }
                    }

                    if starving_time.is_some() {
                        starving_time = None;
                    }
                }
                Err(_) => {
                    if let Some(inst) = starving_time {
                        if Instant::now().duration_since(inst) > self.read_timeout {
                            log::warn!("{} {} srt read timeout" ,  self.name ,  self.port);
                            is_closed = true;
                        }
                    } else {
                        starving_time = Some(Instant::now());
                    }
                    if has_started && !got_close {
                        let _ = self.handle_bytes(&[]).await;
                    }
                }
            }
        }

        let quality = if has_started {
            let demuxer = self.demuxer.read().await;
            Some(demuxer.quality())
        } else {
            None
        };
        if let Err(err) = self.close_sender.send(quality) {
            log::error!(
                "{} {} close event sending error {}" , 
                self.name , 
                self.port , 
                err
            );
        }
    }

    async fn handle_socket(&mut self ,  drop_packet: bool) -> Result<() ,  Error> {
        if let Some((_instant ,  bytes)) = self.socket.try_next().await? {
            log::debug!("{} {} src packets received" ,  self.name ,  self.port);
            if !drop_packet {
                self.handle_bytes(&bytes).await?;
            }
        } else {
            return Err(Error::SrtDisconnected);
        }
        Ok(())
    }

    async fn handle_bytes(&self ,  input: &[u8]) -> Result<() ,  Error> {
        let mut demuxer = self.demuxer.write().await;
        for sample in demuxer.handle_bytes(input) {
            self.session_handle
                .send(MediaMessage::Sample(sample))
                .map_err(|_| Error::SessionSendFailed)?;
        }

        Ok(())
    }
}
