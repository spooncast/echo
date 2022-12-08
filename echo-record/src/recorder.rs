use {
    anyhow::{bail ,  Result} , 
    echo_codec::aac::ADTS_FRAME_SAMPLES , 
    echo_core::{
        session::{AppName ,  ManageMessage ,  ManagerHandle ,  SessionId ,  SessionWatcher} , 
        Config , 
    } , 
    echo_types::{MediaSample ,  MediaType ,  SampleType ,  Timestamp} , 
    mp4_rs::{
        AacConfig ,  AudioObjectType ,  Bytes ,  ChannelConfig ,  MediaConfig ,  Mp4AsyncWriter ,  Mp4Config , 
        Mp4Sample ,  SampleFreqIndex ,  TrackConfig ,  TrackType , 
    } , 
    std::{
        convert::TryFrom , 
        path::{Path ,  PathBuf} , 
        time::SystemTime , 
    } , 
    tokio::fs::{self ,  File} , 
};

pub struct Recorder {
    name: AppName , 
    id: SessionId , 
    record_root: PathBuf , 
    session_manager: ManagerHandle , 
    session_watcher: SessionWatcher , 
    record_path: PathBuf , 
    mp4_writer: Option<Mp4AsyncWriter<File>> , 
    timestamp: u64 , 
}

impl Recorder {
    pub fn create(
        name: AppName , 
        id: SessionId , 
        session_manager: ManagerHandle , 
        session_watcher: SessionWatcher , 
        config: &Config , 
    ) -> Result<Self> {
        let record_root = config.record_root_dir.clone();
        prepare_record_directory(&record_root)?;

        let record_path = record_root.join(format!("{}.mp4" ,  name));

        Ok(Self {
            name , 
            id , 
            record_root , 
            session_manager , 
            session_watcher , 
            record_path , 
            mp4_writer: None , 
            timestamp: 0 , 
        })
    }

    pub async fn run(mut self) -> Result<()> {
        let tmp_file = tempfile::Builder::new()
            .prefix(&self.name)
            .suffix(".tmp")
            .tempfile_in(&self.record_root)?;

        log::info!(
            "{} {} start recording to {:?}" , 
            self.name , 
            self.id , 
            &tmp_file.path()
        );

        let begin_time = SystemTime::now();
        let mut has_recv = false;
        let mut sid = 0;
        while let Ok(sample) = self.session_watcher.recv().await {
            if sample.sid < sid {
                continue;
            } else if sample.sid > sid {
                sid = sample.sid;
            }
            if !has_recv {
                let mut mp4_writer = Mp4AsyncWriter::async_write_start(
                    File::create(&tmp_file).await? , 
                    &Mp4Config {
                        major_brand: "isom".into() , 
                        minor_version: 512 , 
                        compatible_brands: vec![
                            "isom".into() , 
                            "iso2".into() , 
                            "avc1".into() , 
                            "mp41".into() , 
                        ] , 
                        timescale: 1000 , 
                    } , 
                )
                .await?;

                match sample.media_type {
                    MediaType::Audio {
                        sample_rate , 
                        channels , 
                    } => {
                        let freq_index = match sample_rate {
                            48000 => SampleFreqIndex::Freq48000 , 
                            44100 => SampleFreqIndex::Freq44100 , 
                            _ => bail!("not supported sampling rate {}" ,  sample_rate) , 
                        };
                        let track_conf = TrackConfig {
                            track_type: TrackType::Audio , 
                            timescale: sample_rate , 
                            language: String::from("und") , 
                            media_conf: MediaConfig::AacConfig(AacConfig {
                                bitrate: 102_000 ,                            // XXX
                                profile: AudioObjectType::AacLowComplexity ,  // XXX
                                freq_index , 
                                chan_conf: ChannelConfig::try_from(channels)? , 
                            }) , 
                        };

                        mp4_writer.add_track(&track_conf)?;

                        self.mp4_writer = Some(mp4_writer);
                    }
                }

                if let Err(_) = self
                    .session_manager
                    .send(ManageMessage::StartRecord(self.name.clone() ,  self.id))
                {
                    log::error!("Failed to send StartRecord");
                    panic!("Failed to send StartRecord");
                }

                has_recv = true;
            }
            if let Err(why) = self.handle_sample(sample).await {
                log::error!("{:?}" ,  why);
            }
        }

        if has_recv {
            if let Some(ref mut mp4_writer) = self.mp4_writer {
                mp4_writer.async_write_end().await?;
            }

            self.mp4_writer = None;

            fs::rename(&tmp_file.path() ,  &self.record_path).await?;
            let difference = SystemTime::now()
                .duration_since(begin_time)
                .unwrap()
                .as_secs();

            log::info!(
                "{} {} renamed {:?} to {:?} {} sec" , 
                self.name , 
                self.id , 
                &tmp_file.path() , 
                &self.record_path , 
                difference , 
            );

            if let Err(_) = self.session_manager.send(ManageMessage::CompleteRecord(
                self.name.clone() , 
                self.id , 
                self.record_path.clone() , 
                difference , 
            )) {
                log::error!("Failed to send CompleteRecord");
                panic!("Failed to send CompleteRecord");
            }
        } else {
            fs::remove_file(&tmp_file.path()).await?;
        }

        Ok(())
    }

    async fn handle_aac_audio(&mut self ,  _timestamp: Timestamp ,  bytes: &[u8]) -> Result<()> {
        if bytes.len() > 7 {
            if let Some(ref mut mp4_writer) = self.mp4_writer {
                let sample = Mp4Sample {
                    start_time: self.timestamp , 
                    duration: ADTS_FRAME_SAMPLES , 
                    rendering_offset: 0 , 
                    is_sync: true , 
                    bytes: Bytes::from(bytes[7..].to_vec()) , 
                };
                // log::info!("{:?}" ,  sample);
                mp4_writer.async_write_sample(1 ,  &sample).await?;
                self.timestamp += ADTS_FRAME_SAMPLES as u64;
            }
        }
        Ok(())
    }

    async fn handle_sample(&mut self ,  sample: MediaSample) -> Result<()> {
        match sample.sample_type {
            SampleType::AAC => {
                self.handle_aac_audio(sample.timestamp.unwrap() ,  sample.data())
                    .await
            }
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        log::info!(
            "{} {} closing recorder for {}" , 
            self.name , 
            self.id , 
            self.record_path.display()
        );
    }
}

fn prepare_record_directory<P: AsRef<Path>>(path: P) -> Result<()> {
    let record_path = path.as_ref();

    if record_path.exists() && !record_path.is_dir() {
        bail!(
            "Path '{}' exists ,  but is not a directory" , 
            record_path.display()
        );
    }

    log::debug!(
        "Creating recording directory at '{}'" , 
        record_path.display()
    );
    std::fs::create_dir_all(&record_path)?;

    Ok(())
}
