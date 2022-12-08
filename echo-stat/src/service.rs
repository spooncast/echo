use {
    crate::{
        logger::SessionCountLogger , 
        session::{
            count_input_sessions ,  count_sessions ,  get_session ,  list_sessions ,  Session , 
            SessionCount ,  Sessions , 
        } , 
        sysinfo::sys_usage , 
    } , 
    echo_core::{
        session::{self ,  EventKind ,  EventMessage ,  ManageMessage ,  ManagerHandle ,  SessionId} , 
        Config , 
    } , 
    echo_types::Protocol , 
    warp::{
        http::header::{self ,  HeaderMap ,  HeaderValue} , 
        Filter , 
    } , 
};

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

pub struct Service {
    config: Config , 
    session_manager: ManagerHandle , 
}

impl Service {
    pub fn new(session_manager: ManagerHandle ,  config: Config) -> Self {
        Self {
            config , 
            session_manager , 
        }
    }

    pub async fn run(self) {
        let session_count = SessionCount::default();
        let sessions = Sessions::default();

        if self.config.stat_web_enabled {
            let health = warp::path!("health").and_then(health);

            let sys_usage = warp::path!("stat" / "1" / "sysusage").and_then(sys_usage);

            let session_count1 = session_count.clone();
            let count_sessions = warp::path!("stat" / "1" / "sessions" / "count")
                .map(move || session_count1.clone())
                .and_then(count_sessions);

            let session_count2 = session_count.clone();
            let count_input_sessions = warp::path!("stat" / "1" / "sessions" / "count" / "input")
                .map(move || session_count2.clone())
                .and_then(count_input_sessions);

            let sessions1 = sessions.clone();
            let list_sessions = warp::path!("stat" / "1" / "sessions")
                .map(move || sessions1.clone())
                .and_then(list_sessions);

            let sessions2 = sessions.clone();
            let get_session = warp::path!("stat" / "1" / "sessions" / SessionId)
                .map(move |id| (sessions2.clone() ,  id))
                .untuple_one()
                .and_then(get_session);

            let mut headers = HeaderMap::new();
            headers.insert(
                header::SERVER , 
                HeaderValue::from_str(&format!("Echo/{}" ,  VERSION.unwrap())).unwrap() , 
            );
            let routes = warp::get()
                .and(
                    health
                        .or(sys_usage)
                        .or(count_sessions)
                        .or(count_input_sessions)
                        .or(list_sessions)
                        .or(get_session) , 
                )
                .with(warp::reply::with::headers(headers))
                .with(warp::log("echo-stat-web"));

            let addr = self.config.stat_web_addr;
            tokio::spawn(async move {
                warp::serve(routes).run(addr).await;
            });
        }

        {
            let session_count = session_count.clone();
            tokio::spawn(async move {
                SessionCountLogger::new(session_count).run().await;
            });
        }

        let (trigger ,  mut trigger_watcher) = session::trigger_channel();

        if let Err(_) = self.session_manager.send(ManageMessage::RegisterTrigger(
            EventKind::CreateSession0 , 
            trigger.clone() , 
        )) {
            log::error!("Failed to register CreateSession0 trigger");
            panic!("Failed to register CreateSession0 trigger");
        }

        if let Err(_) = self.session_manager.send(ManageMessage::RegisterTrigger(
            EventKind::ReleaseSession , 
            trigger.clone() , 
        )) {
            log::error!("Failed to register ReleaseSession trigger");
            panic!("Failed to register ReleaseSession trigger");
        }

        if let Err(_) = self.session_manager.send(ManageMessage::RegisterTrigger(
            EventKind::ReadyHlsSession , 
            trigger.clone() , 
        )) {
            log::error!("Failed to register ReadyHlsSession trigger");
            panic!("Failed to register ReadyHlsSession trigger");
        }

        if let Err(_) = self.session_manager.send(ManageMessage::RegisterTrigger(
            EventKind::ReleaseHlsSession , 
            trigger.clone() , 
        )) {
            log::error!("Failed to register ReleaseHlsSession trigger");
            panic!("Failed to register ReleaseHlsSession trigger");
        }

        if let Err(_) = self.session_manager.send(ManageMessage::RegisterTrigger(
            EventKind::StartRecord , 
            trigger.clone() , 
        )) {
            log::error!("Failed to register StartRecord trigger");
            panic!("Failed to register StartRecord trigger");
        }

        if let Err(_) = self.session_manager.send(ManageMessage::RegisterTrigger(
            EventKind::CompleteRecord , 
            trigger.clone() , 
        )) {
            log::error!("Failed to register CompleteRecord trigger");
            panic!("Failed to register CompleteRecord trigger");
        }

        if let Err(_) = self.session_manager.send(ManageMessage::RegisterTrigger(
            EventKind::InputQualityReport , 
            trigger , 
        )) {
            log::error!("Failed to register InputQualityReport trigger");
            panic!("Failed to register InputQualityReport trigger");
        }

        while let Some((name ,  event)) = trigger_watcher.recv().await {
            match event {
                EventMessage::CreateSession0(id ,  proto ,  _ ,  _) => {
                    let mut sessions = sessions.write().await;
                    if !sessions.contains_key(&id) {
                        let session = Session::new(name ,  id ,  proto);
                        sessions.insert(id ,  session);

                        let mut session_count = session_count.write().await;
                        match proto {
                            Protocol::SRT => session_count.input.srt += 1 , 
                            Protocol::RTMP => session_count.input.rtmp += 1 , 
                        }
                        session_count.input.total += 1;
                        session_count.total += 1;
                    }
                }
                EventMessage::ReleaseSession(id ,  _ ,  _) => {
                    let mut sessions = sessions.write().await;
                    let mut session_count = session_count.write().await;

                    let is_fin_sess = if let Some(ref mut session) = sessions.get_mut(&id) {
                        session.release_ingest();

                        match session.protocol {
                            Protocol::SRT => session_count.input.srt -= 1 , 
                            Protocol::RTMP => session_count.input.rtmp -= 1 , 
                        }
                        session_count.input.total -= 1;
                        session_count.total -= 1;

                        session.is_finished_and_log(id)
                    } else {
                        log::warn!("ReleaseSession: session not found {}" ,  name);
                        continue;
                    };

                    if is_fin_sess {
                        sessions.remove(&id);
                    }
                }
                EventMessage::ReadyHlsSession(id ,  path ,  _) => {
                    let mut sessions = sessions.write().await;
                    let mut session_count = session_count.write().await;

                    if let Some(ref mut session) = sessions.get_mut(&id) {
                        if session.ready_hls(path) {
                            session_count.output.hls += 1;
                            session_count.output.total += 1;
                            session_count.total += 1;
                        } else {
                            log::warn!(
                                "ReadyHlsSession: it has already been started {}({})" , 
                                name , 
                                id
                            );
                        }
                    } else {
                        log::warn!("ReadyHlsSession: session not found {}({})" ,  name ,  id);
                    }
                }
                EventMessage::ReleaseHlsSession(id ,  _) => {
                    let mut sessions = sessions.write().await;
                    let mut session_count = session_count.write().await;

                    let is_fin_sess = if let Some(ref mut session) = sessions.get_mut(&id) {
                        if session.release_hls() {
                            session_count.output.hls -= 1;
                            session_count.output.total -= 1;
                            session_count.total -= 1;
                        } else {
                            log::warn!(
                                "ReleaseHlsSession: it has already been released {}({})" , 
                                name , 
                                id
                            );
                        }

                        session.is_finished_and_log(id)
                    } else {
                        log::warn!("ReleaseHlsSession: session not found {}({})" ,  name ,  id);
                        continue;
                    };

                    if is_fin_sess {
                        sessions.remove(&id);
                    }
                }
                EventMessage::StartRecord(id ,  _) => {
                    let mut sessions = sessions.write().await;
                    let mut session_count = session_count.write().await;

                    if let Some(ref mut session) = sessions.get_mut(&id) {
                        if session.start_record() {
                            session_count.output.record += 1;
                            session_count.output.total += 1;
                            session_count.total += 1;
                        } else {
                            log::warn!("StartRecord: session not found {}({})" ,  name ,  id);
                        }
                    } else {
                        log::warn!("StartRecord: session not found {}({})" ,  name ,  id);
                    }
                }
                EventMessage::CompleteRecord(id ,  path ,  _ ,  _) => {
                    let mut sessions = sessions.write().await;
                    let mut session_count = session_count.write().await;

                    let is_fin_sess = if let Some(ref mut session) = sessions.get_mut(&id) {
                        if session.complete_record(path) {
                            session_count.output.record -= 1;
                            session_count.output.total -= 1;
                            session_count.total -= 1;
                        } else {
                            log::warn!(
                                "CompleteRecord: it has already been completed {}({})" , 
                                name , 
                                id
                            );
                        }

                        session.is_finished_and_log(id)
                    } else {
                        log::warn!("CompleteRecord: session not found {}({})" ,  name ,  id);
                        continue;
                    };

                    if is_fin_sess {
                        sessions.remove(&id);
                    }
                }
                EventMessage::InputQualityReport(id ,  quality ,  _) => {
                    let mut sessions = sessions.write().await;
                    if let Some(ref mut session) = sessions.get_mut(&id) {
                        session.quality_log(id ,  quality);
                    }
                }
                _ => {}
            }
        }
    }
}

async fn health() -> Result<impl warp::Reply ,  std::convert::Infallible> {
    Ok(warp::reply())
}
