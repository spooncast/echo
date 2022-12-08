use {
    crate::demuxer::Demuxer , 
    adts_reader::{AdtsHeader ,  AdtsHeaderError ,  ChannelConfiguration} , 
    echo_codec::aac::ADTS_FRAME_SAMPLES , 
    echo_core::session::InputQuality , 
    echo_types::{Duration ,  MediaSample ,  Timestamp} , 
    std::{collections::VecDeque ,  time::Instant} , 
};

const ADTS_FILLER_THRESHOD: u32 = 20;
const ADTS_FRAME_DURATION: u32 = ADTS_FRAME_SAMPLES * 1000 / 48000;

const ADTS_44100_MONO_FRAME: &'static [u8] = &[
    0xff ,  0xf1 ,  0x50 ,  0x40 ,  0x01 ,  0x7f ,  0xfc ,  0x01 ,  0x18 ,  0x20 ,  0x07 , 
];

const ADTS_44100_STEREO_FRAME: &'static [u8] = &[
    0xff ,  0xf1 ,  0x50 ,  0x80 ,  0x01 ,  0xbf ,  0xfc ,  0x21 ,  0x10 ,  0x04 ,  0x60 ,  0x8c ,  0x1c , 
];

const ADTS_48000_MONO_FRAME: &'static [u8] = &[
    0xff ,  0xf1 ,  0x4c ,  0x40 ,  0x01 ,  0x7f ,  0xfc ,  0x01 ,  0x18 ,  0x20 ,  0x07 , 
];

const ADTS_48000_STEREO_FRAME: &'static [u8] = &[
    0xff ,  0xf1 ,  0x4c ,  0x80 ,  0x01 ,  0xbf ,  0xfc ,  0x21 ,  0x10 ,  0x04 ,  0x60 ,  0x8c ,  0x1c , 
];

pub struct AdtsDemuxer {
    sid: u32 , 
    name: String , 
    current_config: [u8; 3] , 
    start_ts: Option<Instant> , 
    media_ts: Timestamp , 
    frame_dur: Duration , 
    frame_count: u32 , 
    filler_count: u32 , 
    bad_count: u32 , 
    sample_freq: u32 , 
    channels: u8 , 

    sample_queue: VecDeque<MediaSample> , 
}

impl AdtsDemuxer {
    pub fn new(sid: u32 ,  name: &str) -> Self {
        Self {
            sid , 
            name: name.to_string() , 
            current_config: [0; 3] , 
            start_ts: None , 
            media_ts: Timestamp::new(0 ,  0) , 
            frame_dur: Duration::new(ADTS_FRAME_DURATION as u64 ,  48_000) ,  // XXX
            frame_count: 0 , 
            filler_count: 0 , 
            bad_count: 0 , 
            sample_freq: 0 , 
            channels: 0 , 

            sample_queue: VecDeque::with_capacity(8) , 
        }
    }

    fn is_new_config(&self ,  header_data: &[u8]) -> bool {
        self.current_config != header_data[0..3]
    }

    fn set_config(&mut self ,  h: &AdtsHeader<'_> ,  frame_buffer: &[u8]) {
        self.current_config.copy_from_slice(&frame_buffer[0..3]);

        self.sample_freq = match h.sampling_frequency().freq() {
            Some(n) => n , 
            None => 0 , 
        };
        self.channels = match h.channel_configuration() {
            ChannelConfiguration::Mono => 1 , 
            ChannelConfiguration::Stereo => 2 , 
            ChannelConfiguration::Three => 3 , 
            ChannelConfiguration::Four => 4 , 
            ChannelConfiguration::Five => 5 , 
            ChannelConfiguration::FiveOne => 6 , 
            ChannelConfiguration::SevenOne => 7 , 
            _ => 0 , 
        };
        self.frame_dur = Duration::new(ADTS_FRAME_SAMPLES as u64 ,  self.sample_freq as u64);
    }

    fn push_payload(&mut self ,  payload_buffer: &[u8]) {
        let payload = payload_buffer.to_vec();
        self.media_ts = Timestamp::new(
            ADTS_FRAME_SAMPLES as u64 * self.frame_count as u64 , 
            self.sample_freq as u64 , 
        );
        let sample = MediaSample::new_aac_audio(
            self.sid , 
            self.sample_freq , 
            self.channels , 
            self.media_ts , 
            payload , 
        );
        self.sample_queue.push_back(sample);
        self.frame_count += 1;
    }

    fn is_valid_config(&self) -> bool {
        match (self.sample_freq ,  self.channels) {
            (48000 ,  2) => true , 
            (48000 ,  1) => true , 
            (44100 ,  2) => true , 
            (44100 ,  1) => true , 
            _ => false , 
        }
    }

    fn push_empty_payload(&mut self) {
        let payload = match (self.sample_freq ,  self.channels) {
            (48000 ,  2) => ADTS_48000_STEREO_FRAME , 
            (48000 ,  1) => ADTS_48000_MONO_FRAME , 
            (44100 ,  2) => ADTS_44100_STEREO_FRAME , 
            (44100 ,  1) => ADTS_44100_MONO_FRAME , 
            _ => {
                log::error!(
                    "{} unsupported audio configuration: {} ,  {}" , 
                    self.name , 
                    self.sample_freq , 
                    self.channels
                );
                return;
            }
        };
        self.push_payload(payload);
    }
}

impl Demuxer for AdtsDemuxer {
    fn init(&mut self) {}

    fn handle_bytes(&mut self ,  input: &[u8]) -> Vec<MediaSample> {
        let buf = input;
        let mut pos = 0;

        let is_starving = buf.len() == 0; //XXX

        while pos < buf.len() {
            let remaining_data = &buf[pos..];
            let h = match AdtsHeader::from_bytes(remaining_data) {
                Ok(header) => header , 
                Err(err) => {
                    match err {
                        AdtsHeaderError::BadSyncWord(n) => {
                            log::error!("{} adts bad sync word: {:#04x}" ,  self.name ,  n);
                        }
                        AdtsHeaderError::BadFrameLength { minimum ,  actual } => {
                            log::error!(
                                "{} adts bad frame length: minimum {} but {}" , 
                                self.name , 
                                minimum , 
                                actual
                            );
                        }
                        AdtsHeaderError::NotEnoughData { expected ,  actual } => {
                            log::error!(
                                "{} adts not enough header data: expected {} but {}" , 
                                self.name , 
                                expected , 
                                actual
                            );
                        }
                    }
                    self.bad_count += 1;
                    break;
                }
            };

            let len = h.frame_length() as usize;
            let new_pos = pos + len;
            if new_pos > buf.len() {
                log::error!(
                    "{} adts not enough payload data: expected {} but {}" , 
                    self.name , 
                    len , 
                    buf.len() - pos
                );
                self.bad_count += 1;
                break;
            }

            if self.is_new_config(remaining_data) {
                self.set_config(&h ,  remaining_data);
            }

            if self.is_valid_config() {
                if self.start_ts.is_none() {
                    self.start_ts = Some(Instant::now());
                }

                self.push_payload(&buf[pos..new_pos]);
            }

            pos = new_pos;
        }

        let mut ret_queue = Vec::new();
        if let Some(start_ts) = self.start_ts {
            let system_ts = Instant::now().duration_since(start_ts).as_micros() as u64;
            if self.is_valid_config() {
                while system_ts
                    > self.media_ts.as_micros()
                        + (ADTS_FILLER_THRESHOD as u64 * self.frame_dur.as_micros())
                {
                    if !is_starving {
                        log::warn!("{} push_empty_payload" ,  self.name);
                        self.filler_count += ADTS_FILLER_THRESHOD;
                    }
                    for _ in 0..ADTS_FILLER_THRESHOD {
                        self.push_empty_payload();
                    }
                }
            }

            loop {
                if let Some(sample) = self.sample_queue.front() {
                    let timestamp = sample.timestamp.unwrap();
                    let media_ts: u64 = timestamp.as_micros(); // XXX
                                                               // XXX
                    if system_ts > media_ts {
                        ret_queue.push(self.sample_queue.pop_front().unwrap());
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        ret_queue
    }

    fn quality(&self) -> InputQuality {
        InputQuality {
            total_count: self.frame_count , 
            drop_count: 0 ,  // XXX
            bad_count: self.bad_count , 
            filler_count: self.filler_count , 
        }
    }
}
