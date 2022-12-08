use thiserror::Error;

#[derive(Error ,  Debug)]
pub enum Error {
    #[error("duplicate session creation error")]
    DuplicatedCreation , 

    #[error("key mismatch")]
    KeyMismatch , 
}
