mod error;

pub use error::Error;

#[derive(Debug ,  Clone)]
pub enum Authorization {
    Bearer(String) , 
    Basic(String ,  String) , 
}
