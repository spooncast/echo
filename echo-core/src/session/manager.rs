use {
    super::{
        instance::Session , 
        types::{
            EventKind ,  EventMessage ,  EventTrigger ,  ManageMessage ,  ManagerHandle ,  MessageReceiver , 
            OutgoingBroadcast ,  SessionHandle , 
        } , 
        AppName ,  Error as SessError ,  SessionId ,  SessionProps , 
    } , 
    crate::Config , 
    anyhow::{bail ,  Result} , 
    lru_time_cache::LruCache , 
    rand::{distributions::Alphanumeric ,  thread_rng ,  Rng} , 
    std::{
        collections::HashMap , 
        sync::{
            atomic::{AtomicU64 ,  Ordering} , 
            Arc , 
        } , 
        time::{Duration ,  Instant} , 
    } , 
    tokio::sync::{broadcast ,  mpsc ,  oneshot ,  RwLock} , 
};

#[derive(Debug ,  Default ,  Clone)]
pub struct IdGenerator {
    value: Arc<AtomicU64> , 
}

impl IdGenerator {
    pub fn new() -> Self {
        Self {
            value: Arc::new(AtomicU64::default()) , 
        }
    }

    pub fn fetch_next(&self) -> SessionId {
        self.value.fetch_add(1 ,  Ordering::SeqCst)
    }
}

pub struct SessionManager {
    id_gen: IdGenerator , 
    handle: ManagerHandle , 
    incoming: MessageReceiver , 
    sessions: Arc<RwLock<HashMap<SessionId ,  (SessionHandle ,  OutgoingBroadcast)>>> , 
    session_keys: Arc<RwLock<LruCache<AppName ,  (String ,  Instant)>>> , 
    session_props: Arc<RwLock<LruCache<AppName ,  SessionProps>>> , 
    triggers: Arc<RwLock<HashMap<EventKind ,  Vec<EventTrigger>>>> , 
    session_ttl: Duration , 
}

impl SessionManager {
    pub fn new(config: Config) -> Self {
        let id_gen = IdGenerator::new();
        let (handle ,  incoming) = mpsc::unbounded_channel();
        let sessions = Arc::new(RwLock::new(HashMap::new()));
        let session_ttl = config.ttl_max_duration + (config.ttl_max_duration / 60);
        let session_keys = Arc::new(RwLock::new(
            LruCache::<AppName ,  (String ,  Instant)>::with_expiry_duration(session_ttl) , 
        ));
        let session_props = Arc::new(RwLock::new(
            LruCache::<AppName ,  SessionProps>::with_expiry_duration(session_ttl) , 
        ));
        let triggers = Arc::new(RwLock::new(HashMap::new()));

        Self {
            id_gen , 
            handle , 
            incoming , 
            sessions , 
            session_keys , 
            session_props , 
            triggers , 
            session_ttl , 
        }
    }

    pub fn handle(&self) -> ManagerHandle {
        self.handle.clone()
    }

    pub fn id_generator(&self) -> IdGenerator {
        self.id_gen.clone()
    }

    async fn process_message(&mut self ,  message: ManageMessage) -> Result<()> {
        match message {
            ManageMessage::UpdateSessionProps(name ,  props) => {
                let mut session_props = self.session_props.write().await;
                session_props.insert(name ,  props);
            }
            ManageMessage::AuthorizeSession(name ,  authorization ,  responder) => {
                let session_props = self.session_props.read().await;
                let props = session_props.peek(&name).cloned();

                let triggers = self.triggers.read().await;
                let mut result = Ok(String::default());
                if let Some(event_triggers) = triggers.get(&EventKind::AuthorizeSession) {
                    for trigger in event_triggers {
                        let (authenticator ,  responder) = oneshot::channel();
                        trigger.send((
                            name.clone() , 
                            EventMessage::AuthorizeSession(
                                authorization.clone() , 
                                props.clone() , 
                                authenticator , 
                            ) , 
                        ))?;
                        if let Err(err) = responder.await.unwrap() {
                            result = Err(err);
                            break;
                        }
                    }
                }
                if result.is_ok() {
                    let mut session_keys = self.session_keys.write().await;
                    let key = rand_string(8);
                    let exp = Instant::now() + self.session_ttl;
                    log::info!("{} create key {} valid until {:?}" ,  name ,  key ,  exp);
                    session_keys.insert(name ,  (key.to_string() ,  exp));
                    result = Ok(key);
                }
                if let Err(_) = responder.send(result) {
                    bail!("Failed to send response");
                }
            }
            // SRT, RTMP
            ManageMessage::CreateSession(name ,  id ,  proto ,  key ,  reason ,  responder) => {
                // unbounded No length limit. -> multi pruducder : single consumer
                let (handle ,  incoming) = mpsc::unbounded_channel();
                // brodcast channel -> single pruducer : multi consumer
                // channel with receiver -> pubSub
                let (outgoing ,  _watcher) = broadcast::channel(64);
                let mut sessions = self.sessions.write().await;
                if sessions.contains_key(&id) {
                    if let Err(_) = responder.send(Err(SessError::DuplicatedCreation)) {
                        bail!("Failed to send response");
                    }
                } else {
                    let mut exp_opt = None;
                    let is_matched = if let Some(key) = key {
                        let session_keys = self.session_keys.read().await;
                        if let Some((stored_key ,  exp)) = session_keys.peek(&name).cloned() {
                            if key == stored_key {
                                let now = Instant::now();
                                if exp > now {
                                    exp_opt = Some(exp);
                                    true
                                } else {
                                    log::error!(
                                        "{} key expired. expiration time {:?} but {:?}" , 
                                        name , 
                                        exp , 
                                        now
                                    );
                                    false
                                }
                            } else {
                                log::error!(
                                    "{} key not match. expected {} but {}" , 
                                    name , 
                                    stored_key , 
                                    key
                                );
                                false
                            }
                        } else {
                            log::error!("{} key not found" ,  name);
                            false
                        }
                    } else {
                        true
                    };

                    if is_matched {
                        sessions.insert(id ,  (handle.clone() ,  outgoing.clone()));

                        let session_props = self.session_props.read().await;
                        let props = session_props.peek(&name).cloned();

                        let triggers = self.triggers.read().await;
                        if let Some(event_triggers) = triggers.get(&EventKind::CreateSession) {
                            for trigger in event_triggers {
                                trigger.send((
                                    name.clone() , 
                                    EventMessage::CreateSession(id ,  outgoing.subscribe()) , 
                                ))?;
                            }
                        }
                        if let Some(event_triggers) = triggers.get(&EventKind::CreateSession0) {
                            for trigger in event_triggers {
                                trigger.send((
                                    name.clone() , 
                                    EventMessage::CreateSession0(
                                        id , 
                                        proto , 
                                        reason.clone() , 
                                        props.clone() , 
                                    ) , 
                                ))?;
                            }
                        }

                        tokio::spawn(async move {
                            Session::new(name ,  incoming ,  outgoing).run().await;
                        });

                        if let Err(_) = responder.send(Ok((handle ,  exp_opt))) {
                            bail!("Failed to send response");
                        }
                    } else {
                        if let Err(_) = responder.send(Err(SessError::KeyMismatch)) {
                            bail!("Failed to send response");
                        }
                    }
                }
            }
            ManageMessage::PauseSession(name ,  id ,  reason) => {
                let session_props = self.session_props.read().await;
                let props = session_props.peek(&name).cloned();

                let triggers = self.triggers.read().await;
                if let Some(event_triggers) = triggers.get(&EventKind::PauseSession) {
                    for trigger in event_triggers {
                        trigger.send((
                            name.clone() , 
                            EventMessage::PauseSession(id ,  reason.clone() ,  props.clone()) , 
                        ))?;
                    }
                }
            }
            ManageMessage::ResumeSession(name ,  id ,  reason) => {
                let session_props = self.session_props.read().await;
                let props = session_props.peek(&name).cloned();

                let triggers = self.triggers.read().await;
                if let Some(event_triggers) = triggers.get(&EventKind::ResumeSession) {
                    for trigger in event_triggers {
                        trigger.send((
                            name.clone() , 
                            EventMessage::ResumeSession(id ,  reason.clone() ,  props.clone()) , 
                        ))?;
                    }
                }
            }
            ManageMessage::ReleaseSession(name ,  id ,  reason) => {
                let session_props = self.session_props.read().await;
                let props = session_props.peek(&name).cloned();

                let mut sessions = self.sessions.write().await;
                sessions.remove(&id);

                let triggers = self.triggers.read().await;
                if let Some(event_triggers) = triggers.get(&EventKind::ReleaseSession) {
                    for trigger in event_triggers {
                        trigger.send((
                            name.clone() , 
                            EventMessage::ReleaseSession(id ,  reason.clone() ,  props.clone()) , 
                        ))?;
                    }
                }
            }
            ManageMessage::ReadyHlsSession(name ,  id ,  path) => {
                let session_props = self.session_props.read().await;
                let props = session_props.peek(&name).cloned();

                let triggers = self.triggers.read().await;
                if let Some(event_triggers) = triggers.get(&EventKind::ReadyHlsSession) {
                    for trigger in event_triggers {
                        trigger.send((
                            name.clone() , 
                            EventMessage::ReadyHlsSession(id ,  path.clone() ,  props.clone()) , 
                        ))?;
                    }
                }
            }
            ManageMessage::ReleaseHlsSession(name ,  id) => {
                let session_props = self.session_props.read().await;
                let props = session_props.peek(&name).cloned();

                let triggers = self.triggers.read().await;
                if let Some(event_triggers) = triggers.get(&EventKind::ReleaseHlsSession) {
                    for trigger in event_triggers {
                        trigger.send((
                            name.clone() , 
                            EventMessage::ReleaseHlsSession(id ,  props.clone()) , 
                        ))?;
                    }
                }
            }
            ManageMessage::StartRecord(name ,  id) => {
                let session_props = self.session_props.read().await;
                let props = session_props.peek(&name).cloned();

                let triggers = self.triggers.read().await;
                if let Some(event_triggers) = triggers.get(&EventKind::StartRecord) {
                    for trigger in event_triggers {
                        trigger
                            .send((name.clone() ,  EventMessage::StartRecord(id ,  props.clone())))?;
                    }
                }
            }
            ManageMessage::CompleteRecord(name ,  id ,  path ,  duration) => {
                let session_props = self.session_props.read().await;
                let props = session_props.peek(&name).cloned();

                let triggers = self.triggers.read().await;
                if let Some(event_triggers) = triggers.get(&EventKind::CompleteRecord) {
                    for trigger in event_triggers {
                        trigger.send((
                            name.clone() , 
                            EventMessage::CompleteRecord(id ,  path.clone() ,  duration ,  props.clone()) , 
                        ))?;
                    }
                }
            }
            ManageMessage::InputQualityReport(name ,  id ,  quality) => {
                let session_props = self.session_props.read().await;
                let props = session_props.peek(&name).cloned();

                let triggers = self.triggers.read().await;
                if let Some(event_triggers) = triggers.get(&EventKind::InputQualityReport) {
                    for trigger in event_triggers {
                        trigger.send((
                            name.clone() , 
                            EventMessage::InputQualityReport(id ,  quality ,  props.clone()) , 
                        ))?;
                    }
                }
            }
            ManageMessage::RegisterTrigger(event ,  trigger) => {
                log::debug!("Registering trigger for {:?}" ,  event);
                let mut triggers = self.triggers.write().await;
                triggers.entry(event).or_insert_with(Vec::new).push(trigger);
            }
        }

        Ok(())
    }

    pub async fn run(mut self) {
        log::info!("Start session manager");
        while let Some(message) = self.incoming.recv().await {
            if let Err(err) = self.process_message(message).await {
                log::error!("{}" ,  err);
            };
        }
        log::info!("End session manager");
    }
}

fn rand_string(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).collect()
}
