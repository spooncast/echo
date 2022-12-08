use {
    super::{AppName ,  Error as SessError ,  SessionId ,  SessionProps} , 
    crate::authorization::{Authorization ,  Error as AuthError} , 
    echo_types::{MediaSample ,  Protocol} , 
    serde::{Deserialize ,  Serialize} , 
    std::{ops::Add ,  path::PathBuf ,  time::Instant} , 
    tokio::sync::{broadcast ,  mpsc ,  oneshot} , 
};

pub type Responder<P> = oneshot::Sender<P>;

#[derive(Debug ,  Default ,  Clone ,  Copy ,  Serialize)]
pub struct InputQuality {
    pub total_count: u32 , 
    pub drop_count: u32 , 
    pub bad_count: u32 , 
    pub filler_count: u32 , 
}

impl Add<InputQuality> for InputQuality {
    type Output = InputQuality;

    fn add(self ,  other: InputQuality) -> InputQuality {
        InputQuality {
            total_count: self.total_count + other.total_count , 
            drop_count: self.drop_count + other.drop_count , 
            bad_count: self.bad_count + other.bad_count , 
            filler_count: self.filler_count + other.filler_count , 
        }
    }
}

#[derive(Debug ,  Clone ,  Serialize ,  Deserialize)]
pub struct StateReason {
    code: u16 , 
    message: String , 
}

impl StateReason {
    pub fn new(code: u16 ,  message: &str) -> Self {
        let message = message.to_string();
        Self { code ,  message }
    }

    pub fn unknown() -> Self {
        Self {
            code: 50000 , 
            message: "unknown".to_string() , 
        }
    }
}

#[derive(Debug ,  Clone ,  Copy ,  PartialEq ,  Eq ,  Hash)]
pub enum EventKind {
    AuthorizeSession , 
    CreateSession , 
    CreateSession0 , 
    ReadyHlsSession , 
    PauseSession , 
    ResumeSession , 
    ReleaseSession , 
    ReleaseHlsSession , 
    StartRecord , 
    CompleteRecord , 
    InputQualityReport , 
}

#[derive(Debug)]
pub enum EventMessage {
    AuthorizeSession(
        Authorization , 
        Option<SessionProps> , 
        Responder<Result<() ,  AuthError>> , 
    ) , 
    CreateSession(SessionId ,  SessionWatcher) , 
    CreateSession0(SessionId ,  Protocol ,  StateReason ,  Option<SessionProps>) , 
    PauseSession(SessionId ,  StateReason ,  Option<SessionProps>) , 
    ResumeSession(SessionId ,  StateReason ,  Option<SessionProps>) , 
    ReleaseSession(SessionId ,  StateReason ,  Option<SessionProps>) , 
    ReadyHlsSession(SessionId ,  String ,  Option<SessionProps>) , 
    ReleaseHlsSession(SessionId ,  Option<SessionProps>) , 
    StartRecord(SessionId ,  Option<SessionProps>) , 
    CompleteRecord(SessionId ,  PathBuf ,  u64 ,  Option<SessionProps>) , 
    InputQualityReport(SessionId ,  InputQuality ,  Option<SessionProps>) , 
}

// session manager
pub enum ManageMessage {
    UpdateSessionProps(AppName ,  SessionProps) , 
    AuthorizeSession(AppName ,  Authorization ,  Responder<Result<String ,  AuthError>>) , 
    CreateSession(
        AppName , 
        SessionId , 
        Protocol , 
        Option<String> , 
        StateReason , 
        Responder<Result<(SessionHandle ,  Option<Instant>) ,  SessError>> , 
    ) , 
    PauseSession(AppName ,  SessionId ,  StateReason) , 
    ResumeSession(AppName ,  SessionId ,  StateReason) , 
    ReleaseSession(AppName ,  SessionId ,  StateReason) , 
    ReadyHlsSession(AppName ,  SessionId ,  String) , 
    ReleaseHlsSession(AppName ,  SessionId) , 
    StartRecord(AppName ,  SessionId) , 
    CompleteRecord(AppName ,  SessionId ,  PathBuf ,  u64) , 
    InputQualityReport(AppName ,  SessionId ,  InputQuality) , 
    RegisterTrigger(EventKind ,  EventTrigger) , 
}

pub type ManagerHandle = mpsc::UnboundedSender<ManageMessage>;
pub(super) type MessageReceiver = mpsc::UnboundedReceiver<ManageMessage>;

pub type EventTrigger = mpsc::UnboundedSender<(String ,  EventMessage)>;
pub(super) type EventWatcher = mpsc::UnboundedReceiver<(String ,  EventMessage)>;

pub fn trigger_channel() -> (EventTrigger ,  EventWatcher) {
    mpsc::unbounded_channel()
}

// session instance
pub enum MediaMessage {
    Sample(MediaSample) , 
    EndOfSample , 
}

pub type SessionHandle = mpsc::UnboundedSender<MediaMessage>;
pub(super) type IncomingBroadcast = mpsc::UnboundedReceiver<MediaMessage>;
pub(super) type OutgoingBroadcast = broadcast::Sender<MediaSample>;
pub type SessionWatcher = broadcast::Receiver<MediaSample>;
