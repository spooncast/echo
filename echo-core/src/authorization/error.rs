use thiserror::Error;

#[derive(Error ,  Debug ,  Clone)]
pub enum Error {
    #[error("expired token")]
    ExpiredToken , 

    #[error("unauthorized")]
    Unauthorized , 
}
