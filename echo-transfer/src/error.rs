use {
    actix_web::{
        http::{header ,  StatusCode} , 
        web ,  ResponseError , 
    } , 
    echo_core::session::{AppName ,  SessionId} , 
    serde_json::json , 
    std::io , 
    thiserror::Error , 
    tokio::time , 
};

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

#[derive(Error ,  Debug)]
pub enum Error {
    #[error("unsupported version: {0}")]
    UnsupportedVersion(u8) , 

    #[error("expired token")]
    ExpiredToken , 

    #[error("unauthorized")]
    Unauthorized , 

    #[error("invalid session cookie")]
    InvalidSessionCookie , 

    #[error("session not found: {0} {1}")]
    SessionNotFound(SessionId ,  AppName) , 

    #[error("I/O Error: {0}")]
    IoError(#[from] io::Error) , 

    #[error("Failed to create new session")]
    SessionCreationFailed , 

    #[error("Failed to release session")]
    SessionReleaseFailed , 

    #[error("Failed to send to session")]
    SessionSendFailed , 

    #[error("Failed to forward session")]
    SessionForwardFailed , 

    #[error("Failed to send manage message")]
    ManageMessageSendFailed , 

    #[error("Failed to receive packet")]
    SrtReceiveError , 

    #[error("Connection timeout")]
    SrtConnectionTimeout(#[from] time::Elapsed) , 

    #[error("Disconnected")]
    SrtDisconnected , 

    #[error("SRT session not found")]
    SrtSessionNotFound , 

    #[error("internal system error: {0}")]
    OtherString(String) , 

    #[error("internal system error: {0}")]
    OtherStr(&'static str) , 
}

impl ResponseError for Error {
    // builds the actual response to send back when an error occurs
    fn error_response(&self) -> web::HttpResponse {
        let status = match self {
            Error::UnsupportedVersion(_) => 400 , 
            Error::ExpiredToken => 460 , 
            Error::Unauthorized => 401 , 
            Error::InvalidSessionCookie => 400 , 
            Error::SessionNotFound(_ ,  _) => 404 , 
            _ => 500 , 
        };
        web::HttpResponse::build(StatusCode::from_u16(status).unwrap())
            .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
            .json(json!({ "code": status ,  "message": self.to_string() }))
    }
}
