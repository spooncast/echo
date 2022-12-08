use {
    crate::{
        message::{EventResponder ,  MessageAccepter ,  SessionEvent ,  SessionMessage ,  SessionState} , 
        receiver::{close_event_channel ,  close_request_channel ,  CloseRequester ,  SrtReceiver} , 
        Error , 
    } , 
    echo_core::session::{
        AppName ,  InputQuality ,  ManageMessage ,  ManagerHandle ,  MediaMessage ,  SessionHandle , 
        SessionId ,  StateReason , 
    } , 
    echo_types::Protocol , 
    srt_tokio::{tokio::create_bidrectional_srt ,  SrtSocketBuilder} , 
    std::time::{Duration ,  Instant} , 
    tokio::{
        stream::StreamExt , 
        sync::{mpsc ,  oneshot} , 
        time::timeout , 
    } , 
};

pub(crate) struct EchoSession {
    id: SessionId , 
    name: AppName , 
    port: u16 , 
    connection_timeout: Duration , 
    read_timeout: Duration , 
    latency: Duration , 
    session_manager: ManagerHandle , 
    session_handle: Option<SessionHandle> , 

    accepter: MessageAccepter , 
    responder: EventResponder , 

    last_message: Option<SessionMessage> , 
    state: SessionState , 
}

impl EchoSession {
    pub fn new(
        id: SessionId , 
        name: &str , 
        port: u16 , 
        connection_timeout: Duration , 
        read_timeout: Duration , 
        latency: Duration , 
        accepter: MessageAccepter , 
        responder: EventResponder , 
        session_manager: ManagerHandle , 
        reason: StateReason , 
    ) -> Self {
        Self {
            id , 
            name: name.to_string() , 
            port , 
            connection_timeout , 
            read_timeout , 
            latency , 
            session_manager , 
            session_handle: None , 

            accepter , 
            responder , 

            last_message: Some(SessionMessage::Init(reason)) , 
            state: SessionState::Init , 
        }
    }

    pub async fn run(mut self) {
        if let Err(err) = self.run_inner().await {
            log::error!("{} {} {}" ,  self.name ,  self.port ,  err);
        }

        self.responder
            .send(SessionEvent {
                id: self.id , 
                name: self.name.to_string() , 
                state: SessionState::Terminated , 
            })
            .map_err(|_| Error::OtherStr("failed to send session event")) // XXX
            .unwrap();

        if let Some(ref mut session) = self.session_handle {
            let name = self.name.to_string();
            session
                .send(MediaMessage::EndOfSample)
                .map_err(|_| Error::SessionSendFailed)
                .unwrap();
            let msg = self.last_message.take();
            if let Some(SessionMessage::Shutdown(reason)) = msg {
                self.send_manage_message(ManageMessage::ReleaseSession(name ,  self.id ,  reason));
            } else {
                self.send_manage_message(ManageMessage::ReleaseSession(
                    name , 
                    self.id , 
                    StateReason::new(30000 ,  "teardown by server") , 
                ));
            }
        }
    }

    fn send_manage_message(&self ,  msg: ManageMessage) {
        self.session_manager
            .send(msg)
            .map_err(|_| Error::ManageMessageSendFailed)
            .unwrap();
    }

    fn send_session_event(&self) {
        self.responder
            .send(SessionEvent {
                id: self.id , 
                name: self.name.to_string() , 
                state: self.state , 
            })
            .map_err(|_| Error::OtherStr("failed to send session event"))
            .unwrap();
    }

    // TODO force shutdown
    async fn run_inner(&mut self) -> Result<() ,  Error> {
        let binding = SrtSocketBuilder::new_listen()
            .local_port(self.port)
            .latency(self.latency)
            .build_multiplexed()
            .await?;

        tokio::pin!(binding);

        log::info!("{} {} srt multiplex listen ..." ,  self.name ,  self.port);

        let (close_sender ,  mut close_receiver) = close_event_channel();
        let mut input_quality = InputQuality::default();
        let mut receiver_count = 0;
        let mut receiver_handler: Option<CloseRequester> = None;
        let mut ready_time = None;
        let mut sid = 0;

        loop {
            match self.accepter.try_recv() {
                Ok(msg) => {
                    self.last_message = Some(msg);
                    if let Some(SessionMessage::Shutdown(ref _r)) = self.last_message {
                        break;
                    }
                }
                Err(err) => match err {
                    mpsc::error::TryRecvError::Closed => {
                        break;
                    }
                    _ => {} // mpsc::error::TryRecvError::Empty
                } , 
            }

            match close_receiver.try_recv() {
                Ok(close_result) => {
                    log::info!(
                        "{} {} receive close signal from receiver. {:?}" , 
                        self.name , 
                        self.port , 
                        close_result , 
                    );
                    receiver_count -= 1;
                    if let Some(quality) = close_result {
                        input_quality = input_quality + quality;
                        self.send_manage_message(ManageMessage::InputQualityReport(
                            self.name.to_string() , 
                            self.id , 
                            input_quality , 
                        ));
                    }
                }
                Err(err) => match err {
                    mpsc::error::TryRecvError::Closed => {
                        return Err(Error::OtherStr("close_receiver closed"));
                    }
                    _ => {} // mpsc::error::TryRecvError::Empty
                } , 
            }
            if receiver_count == 0 && ready_time.is_none() {
                log::info!("{} {} srt waiting" ,  self.name ,  self.port);
                ready_time = Some(Instant::now());

                if self.session_handle.is_none() {
                    if self.state != SessionState::Ready {
                        log::info!("{} {} srt ready" ,  self.name ,  self.port);
                        self.state = SessionState::Ready;
                        self.send_session_event();
                    }
                } else {
                    // core session exists
                    if self.state != SessionState::Paused {
                        log::info!("{} {} srt paused" ,  self.name ,  self.port);
                        self.state = SessionState::Paused;
                        self.send_session_event();
                        let msg = self.last_message.take();
                        if let Some(SessionMessage::Pause(reason)) = msg {
                            self.send_manage_message(ManageMessage::PauseSession(
                                self.name.to_string() , 
                                self.id , 
                                reason , 
                            ));
                        } else {
                            self.send_manage_message(ManageMessage::PauseSession(
                                self.name.to_string() , 
                                self.id , 
                                StateReason::unknown() , 
                            ));
                        }
                    }
                }
            }
            // binding point
            match timeout(Duration::from_millis(500) ,  binding.next()).await {
                Ok(res) => {
                    match res {
                        Some(Ok((conn ,  pack_chan))) => {
                            // session handle - create a core session if it does not present
                            if self.session_handle.is_none() {
                                //
                                self.create_core_session().await?;
                            } else {
                                let msg = self.last_message.take();
                                if let Some(SessionMessage::Resume(reason)) = msg {
                                    self.send_manage_message(ManageMessage::ResumeSession(
                                        self.name.to_string() , 
                                        self.id , 
                                        reason , 
                                    ));
                                } else {
                                    self.send_manage_message(ManageMessage::ResumeSession(
                                        self.name.to_string() , 
                                        self.id , 
                                        StateReason::unknown() , 
                                    ));
                                }
                            }

                            if let Some(handler) = receiver_handler.take() {
                                log::info!(
                                    "{} {} send close signal to receiver" , 
                                    self.name , 
                                    self.port
                                );
                                let _ = handler.send(());
                            }

                            if self.state != SessionState::Publishing {
                                log::info!("{} {} srt publishing" ,  self.name ,  self.port);
                                self.state = SessionState::Publishing;
                                self.send_session_event();
                            }

                            let srt_socket = create_bidrectional_srt(pack_chan ,  conn);
                            let (close_requester ,  close_accepter) = close_request_channel();
                            receiver_handler = Some(close_requester);

                            if let Some(ref session) = self.session_handle {
                                let receiver = SrtReceiver::new(
                                    sid , 
                                    &self.name , 
                                    self.port , 
                                    srt_socket , 
                                    self.read_timeout , 
                                    session.clone() , 
                                    close_accepter , 
                                    close_sender.clone() , 
                                );
                                sid += 1;
                                //echo receiver signal watting
                                tokio::spawn(receiver.run());

                                receiver_count += 1;
                                if ready_time.is_some() {
                                    ready_time = None;
                                }
                            }
                        }
                        Some(Err(err)) => {
                            log::error!("{} multiplex listen error: {:?}" ,  self.name ,  err);
                            return Err(Error::SessionCreationFailed);
                        }
                        None => {
                            log::error!("{} multiplex listen close" ,  self.name);
                            return Err(Error::SessionCreationFailed); // XXX
                        }
                    }
                }
                Err(_) => {
                    if let Some(inst) = ready_time {
                        if Instant::now().duration_since(inst) > self.connection_timeout {
                            log::warn!(
                                "{} {} srt connection timeout {:?} {:?}" , 
                                self.name , 
                                self.port , 
                                Instant::now().duration_since(inst) , 
                                self.connection_timeout
                            );
                            break;
                        }
                    }
                }
            } // end match
        } // end loop

        for _ in 0..20 {
            // wait... srt_protocol::protocol::connection - Exp event hit ,  exp count=??
            if receiver_count == 0 {
                break;
            }

            match timeout(Duration::from_millis(500) ,  close_receiver.recv()).await {
                Ok(res) => match res {
                    Some(close_result) => {
                        log::info!(
                            "{} {} receive close signal from receiver. {:?}" , 
                            self.name , 
                            self.port , 
                            close_result , 
                        );
                        receiver_count -= 1;
                        if let Some(quality) = close_result {
                            input_quality = input_quality + quality;
                            self.send_manage_message(ManageMessage::InputQualityReport(
                                self.name.to_string() , 
                                self.id , 
                                input_quality , 
                            ));
                        }
                    }
                    _ => {}
                } , 
                _ => {}
            }

            if receiver_count > 0 {
                if let Some(handler) = receiver_handler.take() {
                    log::info!("{} {} send close signal to receiver" ,  self.name ,  self.port);
                    if let Err(err) = handler.send(()) {
                        return Err(Error::OtherString(format!(
                            "receiver_handler error {:?}" , 
                            err
                        )));
                    }
                }
            }
        }

        for _ in 0..4 {
            // XXX
            let _ = timeout(Duration::from_millis(500) ,  binding.next()).await;
        }

        Ok(())
    }

    // Create Core Session
    async fn create_core_session(&mut self) -> Result<() ,  Error> {
        let name = self.name.to_string();
        let (accepter ,  responder) = oneshot::channel();
        let msg = self.last_message.take();
        // session manger
        if let Some(SessionMessage::Init(reason)) = msg {
            self.send_manage_message(ManageMessage::CreateSession(
                name , 
                self.id , 
                Protocol::SRT , 
                None , 
                reason , 
                accepter , 
            ));
        } else {
            self.send_manage_message(ManageMessage::CreateSession(
                name , 
                self.id , 
                Protocol::SRT , 
                None , 
                StateReason::unknown() , 
                accepter , 
            ));
        }
        // responder waitting
        let session_result = responder.await.map_err(|_| Error::SessionCreationFailed)?;
        match session_result {
            Ok((session_handle ,  _)) => {
                self.session_handle = Some(session_handle);
            }
            Err(err) => {
                log::error!("{} session creation error: {}" ,  self.name ,  err);
                return Err(Error::SessionCreationFailed);
            }
        }

        Ok(())
    }
}
