use {
    echo_core::session::StateReason , 
    serde::{Deserialize ,  Serialize} , 
    tokio::sync::mpsc , 
};

#[derive(Debug ,  Clone)]
pub(crate) enum SessionMessage {
    Init(StateReason) , 
    Pause(StateReason) , 
    Resume(StateReason) , 
    Shutdown(StateReason) , 
}

#[derive(Debug ,  Clone ,  Copy ,  Eq ,  PartialEq ,  Serialize ,  Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SessionState {
    Init , 
    Ready , 
    Publishing , 
    Paused , 
    Terminated , 
}

pub(crate) struct SessionEvent {
    pub id: u64 , 
    pub name: String , 
    pub state: SessionState , 
}

pub(crate) fn message_channel() -> (MessageRequester ,  MessageAccepter) {
    mpsc::unbounded_channel()
}

pub(crate) fn event_channel() -> (EventResponder ,  EventAccepter) {
    mpsc::unbounded_channel()
}

pub(crate) type MessageRequester = mpsc::UnboundedSender<SessionMessage>;
pub(crate) type MessageAccepter = mpsc::UnboundedReceiver<SessionMessage>;
pub(crate) type EventResponder = mpsc::UnboundedSender<SessionEvent>;
pub(crate) type EventAccepter = mpsc::UnboundedReceiver<SessionEvent>;
