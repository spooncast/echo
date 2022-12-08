use {
    crate::{
        error::Error , 
        rtmp::{Event ,  RtmpHandle} , 
    } , 
    futures::SinkExt , 
    echo_core::{
        session::{
            ManageMessage ,  ManagerHandle ,  MediaMessage ,  SessionHandle ,  SessionId ,  StateReason , 
        } , 
        Config , 
    } , 
    echo_types::Protocol , 
    std::time::Instant , 
    tokio::{prelude::* ,  stream::StreamExt ,  sync::oneshot ,  time::timeout} , 
    tokio_util::codec::{BytesCodec ,  Framed} , 
};

enum State {
    Initializing , 
    Publishing(SessionHandle) , 
    Disconnecting , 
}

/// Represents an incoming connection
pub struct Peer<S>
where
    S: AsyncRead + AsyncWrite + Unpin , 
{
    id: SessionId , 
    bytes_stream: Framed<S ,  BytesCodec> , 
    session_manager: ManagerHandle , 
    rtmp_handle: RtmpHandle , 
    config: Config , 
    app_name: Option<String> , 
    exp_time: Option<Instant> , 
    state: State , 
}

impl<S> Peer<S>
where
    S: AsyncRead + AsyncWrite + Unpin , 
{
    pub fn new(id: SessionId ,  stream: S ,  session_manager: ManagerHandle ,  config: Config) -> Self {
        Self {
            id , 
            bytes_stream: Framed::new(stream ,  BytesCodec::new()) , 
            session_manager , 
            rtmp_handle: RtmpHandle::new() , 
            config , 
            app_name: None , 
            exp_time: None , 
            state: State::Initializing , 
        }
    }

    pub async fn run(mut self) -> Result<() ,  Error> {
        loop {
            match &mut self.state {
                State::Initializing | State::Publishing(_) => {
                    let val = self.bytes_stream.try_next();
                    match timeout(self.config.rtmp_connection_timeout ,  val).await {
                        Ok(res) => match res {
                            Ok(Some(data)) => match self.rtmp_handle.handle_bytes(&data) {
                                Ok(events) => {
                                    for event in events {
                                        self.handle_event(event).await?;
                                    }
                                }
                                Err(err) => {
                                    log::error!(
                                        "{} {} RTMP initializing or publishing error: {:?}" , 
                                        self.app_name.as_deref().unwrap_or("-") , 
                                        self.id , 
                                        err
                                    );
                                    self.disconnect()?;
                                }
                            } , 
                            Ok(None) => {
                                self.disconnect()?;
                            }
                            Err(err) => {
                                log::error!(
                                    "{} {} RTMP initializing or publishing error: {:?}" , 
                                    self.app_name.as_deref().unwrap_or("-") , 
                                    self.id , 
                                    err
                                );
                                self.disconnect()?;
                            }
                        } , 
                        Err(_) => {
                            log::error!(
                                "{} {} RTMP initializing or publishing timeout" , 
                                self.app_name.as_deref().unwrap_or("-") , 
                                self.id
                            );
                            self.disconnect()?;
                        }
                    }
                }
                State::Disconnecting => {
                    log::info!(
                        "{} {} disconnecting..." , 
                        self.app_name.as_deref().unwrap_or("-") , 
                        self.id
                    );
                    return Ok(());
                }
            }
        }
    }

    async fn handle_event(&mut self ,  event: Event) -> Result<() ,  Error> {
        match event {
            Event::ReturnData(data) => {
                self.bytes_stream
                    .send(data)
                    .await
                    .expect("Failed to return data");
            }
            Event::SendSample(sample) => {
                if let State::Publishing(session) = &mut self.state {
                    session
                        .send(MediaMessage::Sample(sample))
                        .map_err(|_| Error::SessionSendFailed)?;
                }
                if let Some(exp) = self.exp_time {
                    let now = Instant::now();
                    if now > exp {
                        log::error!(
                            "{} {} session expired. expiration time {:?} now {:?}" , 
                            self.app_name.as_deref().unwrap_or("-") , 
                            self.id , 
                            exp , 
                            now
                        );
                        self.disconnect()?
                    }
                }
            }
            Event::AcquireSession {
                app_name , 
                stream_key , 
            } => {
                self.app_name = Some(app_name.clone());
                let (request ,  response) = oneshot::channel();
                self.session_manager
                    .send(ManageMessage::CreateSession(
                        app_name.to_string() , 
                        self.id , 
                        Protocol::RTMP , 
                        Some(stream_key.to_string()) , 
                        StateReason::unknown() , 
                        request , 
                    ))
                    .map_err(|_| Error::SessionCreationFailed)?;
                let session_result = response.await.map_err(|_| Error::SessionCreationFailed)?;
                match session_result {
                    Ok((session_sender ,  exp_time)) => {
                        self.exp_time = exp_time;
                        self.state = State::Publishing(session_sender);
                    }
                    Err(err) => {
                        log::error!("{} {} session creation error: {}" ,  app_name ,  self.id ,  err);
                        return Err(Error::SessionCreationFailed);
                    }
                }
                log::info!(
                    "{} {} create rtmp session {}" , 
                    app_name , 
                    self.id , 
                    stream_key
                );
            }
            Event::ReleaseSession | Event::LeaveSession => self.disconnect()? , 
        }

        Ok(())
    }

    fn disconnect(&mut self) -> Result<() ,  Error> {
        if let State::Publishing(session) = &mut self.state {
            let app_name = self.app_name.clone().unwrap();
            session
                .send(MediaMessage::EndOfSample)
                .map_err(|_| Error::SessionSendFailed)?;

            self.session_manager
                .send(ManageMessage::ReleaseSession(
                    app_name.to_string() , 
                    self.id , 
                    StateReason::unknown() , 
                ))
                .map_err(|_| Error::SessionReleaseFailed)?;

            log::info!("{} {} destroy rtmp session" ,  app_name ,  self.id);
        }

        self.state = State::Disconnecting;

        Ok(())
    }
}

impl<S> Drop for Peer<S>
where
    S: AsyncRead + AsyncWrite + Unpin , 
{
    fn drop(&mut self) {
        log::info!(
            "{} {} disconnected" , 
            self.app_name.as_deref().unwrap_or("-") , 
            self.id
        );
    }
}
