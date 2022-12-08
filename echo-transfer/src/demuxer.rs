use {echo_core::session::InputQuality ,  echo_types::MediaSample};

pub trait Demuxer {
    fn init(&mut self);
    fn handle_bytes(&mut self ,  input: &[u8]) -> Vec<MediaSample>;
    fn quality(&self) -> InputQuality;
}
