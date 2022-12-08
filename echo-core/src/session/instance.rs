use {
    super::{
        types::{IncomingBroadcast ,  MediaMessage ,  OutgoingBroadcast} , 
        AppName , 
    } , 
    anyhow::Result , 
    echo_types::{MediaSample ,  MediaType} , 
};

pub struct Session {
    name: AppName , 
    incoming: IncomingBroadcast , 
    outgoing: OutgoingBroadcast , 
    audio_seq_header: Option<MediaSample> , 
    closing: bool , 
}

impl Session {
    #[allow(clippy::new_without_default)]
    pub fn new(name: AppName ,  incoming: IncomingBroadcast ,  outgoing: OutgoingBroadcast) -> Self {
        Self {
            name , 
            incoming , 
            outgoing , 
            audio_seq_header: None , 
            closing: false , 
        }
    }
    // pacaket ->
    pub async fn run(mut self) {
        log::info!("Create session {}" ,  self.name);
        while !self.closing {
            match self.incoming.recv().await {
                Some(message) => {
                    self.handle_message(message);
                }
                None => {
                    log::warn!("Close session {}" ,  self.name);
                    self.closing = true;
                }
            }
        }
        log::info!("Destroy session {}" ,  self.name);
    }

    fn handle_message(&mut self ,  message: MediaMessage) {
        match message {
            MediaMessage::Sample(sample) => {
                self.set_cache(&sample)
                    .expect("Failed to set session cache");
                self.broadcast_sample(sample);
            }
            MediaMessage::EndOfSample => {
                self.closing = true;
            }
        }
    }

    fn broadcast_sample(&self ,  sample: MediaSample) {
        if self.outgoing.receiver_count() != 0 && self.outgoing.send(sample).is_err() {
            log::error!("Failed to broadcast sample");
        }
    }

    fn set_cache(&mut self ,  sample: &MediaSample) -> Result<()> {
        match sample.media_type {
            MediaType::Audio {
                sample_rate , 
                channels , 
            } if self.audio_seq_header.is_none() => {
                log::debug!("MediaType::Audio {} ,  {}" ,  sample_rate ,  channels);
                self.audio_seq_header = Some(sample.clone());
            }
            _ => () , 
        }

        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        log::info!("Closing session");
    }
}
