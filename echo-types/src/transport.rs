use {
    crate::Timestamp , 
    bytes::Bytes , 
    serde::{Deserialize ,  Serialize} , 
};

#[derive(Debug ,  Clone ,  Copy ,  Serialize ,  Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    SRT , 
    RTMP , 
}

#[derive(Debug ,  Clone ,  Copy ,  Serialize ,  Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaFormat {
    AAC , 
    MP2T , 
    FLV , 
}

#[derive(Debug ,  Clone ,  Copy ,  Serialize ,  Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SampleType {
    AAC , 
}

#[derive(Debug ,  Clone ,  Copy ,  Serialize ,  Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Audio { sample_rate: u32 ,  channels: u8 } , 
}

#[derive(Clone)]
pub struct MediaSample {
    pub sid: u32 , 
    pub media_type: MediaType , 
    pub sample_type: SampleType , 
    pub timestamp: Option<Timestamp> , 
    pub data: Bytes , 
}

impl MediaSample {
    pub fn new<B>(
        sid: u32 , 
        media_type: MediaType , 
        sample_type: SampleType , 
        timestamp: Option<Timestamp> , 
        bytes: B , 
    ) -> Self
    where
        B: Into<Bytes> , 
    {
        Self {
            sid , 
            media_type , 
            sample_type , 
            timestamp , 
            data: bytes.into() , 
        }
    }

    pub fn new_aac_audio<B>(
        sid: u32 , 
        sample_rate: u32 , 
        channels: u8 , 
        timestamp: Timestamp , 
        bytes: B , 
    ) -> Self
    where
        B: Into<Bytes> , 
    {
        Self::new(
            sid , 
            MediaType::Audio {
                sample_rate , 
                channels , 
            } , 
            SampleType::AAC , 
            Some(timestamp) , 
            bytes , 
        )
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
