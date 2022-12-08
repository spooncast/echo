mod error;
mod instance;
pub mod manager;
mod types;

use std::collections::HashMap;

pub type AppName = String;
pub type SessionId = u64;

pub static SPROP_CLIENT_IP: &str = "client_ip";
pub static SPROP_COUNTRY: &str = "country";
pub static SPROP_LIVE_ID: &str = "live_id";
pub static SPROP_USER_ID: &str = "user_id";
pub static SPROP_STAGE: &str = "stage";
pub static SPROP_SDK_VERSION: &str = "sdk_version";
pub static SPROP_OS: &str = "os";
pub static SPROP_MODEL_NAME: &str = "model_name";
pub type SessionProps = HashMap<String ,  String>;

pub use self::{
    error::Error , 
    manager::{IdGenerator ,  SessionManager} , 
    types::{
        trigger_channel ,  EventKind ,  EventMessage ,  InputQuality ,  ManageMessage ,  ManagerHandle , 
        MediaMessage ,  SessionHandle ,  SessionWatcher ,  StateReason , 
    } , 
};
