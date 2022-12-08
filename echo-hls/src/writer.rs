use {
    crate::{
        m3u8::{Playlist ,  PlaylistState} , 
        session_cleaner , 
    } , 
    anyhow::{bail ,  Result} , 
    chrono::Utc , 
    m3u8_rs::playlist::MediaPlaylist , 
    echo_codec::mpegts::TransportStream , 
    echo_core::{
        session::{AppName ,  ManageMessage ,  ManagerHandle ,  SessionId ,  SessionWatcher} , 
        Config , 
    } , 
    echo_types::{MediaSample ,  SampleType ,  Timestamp} , 
    std::{
        path::{Path ,  PathBuf} , 
        time::Duration , 
    } , 
    tokio::{fs::File ,  io::AsyncWriteExt} , 
};

static PLAYLIST_NAME: &str = "playlist.m3u8";
static INVALID_TIMESTAMP_MS: u64 = std::u64::MAX;

pub struct Writer {
    name: AppName , 
    id: SessionId , 
    session_manager: ManagerHandle , 
    session_watcher: SessionWatcher , 
    write_interval: u64 , 
    next_write: u64 , 
    last_timestamp: u64 , 
    prev_timestamp: u64 , 
    media_sequence: u32 , 
    discontinuity: bool , 
    buffer: TransportStream , 
    playlist: Playlist , 
    stream_path: PathBuf , 
}

const AAC_FRAME_DURATION: u64 = 22; // XXX

impl Writer {
    pub(crate) fn create(
        name: AppName , 
        id: SessionId , 
        session_manager: ManagerHandle , 
        session_watcher: SessionWatcher , 
        session_cleaner: session_cleaner::Sender , 
        config: &Config , 
        prerole: &MediaPlaylist , 
        prerole_dur: &Duration , 
        seq: u32 , 
    ) -> Result<Self> {
        let write_interval = config.hls_target_duration.as_millis() as u64 - AAC_FRAME_DURATION;
        let next_write = write_interval;

        let hls_root = config.hls_root_dir.clone();
        let stream_path = hls_root.join(&name);
        let playlist_path = stream_path.join(PLAYLIST_NAME);

        prepare_stream_directory(&stream_path)?;

        let playlist = Playlist::new(
            playlist_path , 
            prerole , 
            prerole_dur , 
            config.hls_target_duration , 
            session_cleaner , 
        );

        Ok(Self {
            name , 
            id , 
            session_manager , 
            session_watcher , 
            write_interval , 
            next_write , 
            last_timestamp: 0 , 
            prev_timestamp: INVALID_TIMESTAMP_MS , 
            media_sequence: seq , 
            discontinuity: true , 
            buffer: TransportStream::new() , 
            playlist , 
            stream_path , 
        })
    }

    pub async fn run(mut self) -> Result<()> {
        log::info!("{} {} create HLS" ,  self.name ,  self.id);

        let mut has_recv = false;
        let mut sid = 0;
        while let Ok(sample) = self.session_watcher.recv().await {
            if sample.sid < sid {
                continue;
            } else if sample.sid > sid {
                sid = sample.sid;
            }
            if !has_recv {
                has_recv = true;
            }
            if let Err(why) = self.handle_sample(sample).await {
                log::error!("{:?}" ,  why);
            }
        }

        if has_recv {
            self.playlist
                .release(self.name.clone() ,  self.id ,  self.session_manager.clone())
                .await;
        }

        log::info!("{} {} destroy HLS" ,  self.name ,  self.id);

        Ok(())
    }

    async fn write_segment(&mut self ,  timestamp: u64 ,  discontinuity: bool) -> Result<()> {
        let duration = timestamp - self.last_timestamp;

        let filename = format!("{}-{}.ts" ,  self.media_sequence ,  Utc::now().timestamp());
        let path = self.stream_path.join(&filename);

        let mut buffer: Vec<u8> = Vec::new(); // XXX
        self.buffer.write(&mut buffer)?;

        let mut file = File::create(&path).await?;
        file.write_all(&buffer).await?;

        match self
            .playlist
            .add_media_segment(
                filename , 
                Duration::from_millis(duration) , 
                self.discontinuity , 
            )
            .await
        {
            Ok(PlaylistState::Ready) => {
                let hls_path = format!("{}/{}" ,  self.name ,  PLAYLIST_NAME);
                if let Err(_) = self.session_manager.send(ManageMessage::ReadyHlsSession(
                    self.name.clone() , 
                    self.id , 
                    hls_path , 
                )) {
                    log::error!("Failed to send ReadyHlsSession");
                    panic!("Failed to send ReadyHlsSession");
                }
            }
            Ok(PlaylistState::NotReady) => {
                log::warn!("{} HLS not ready" ,  self.name);
                // TODO: Paused
                // if let Err(_) = self
                //     .session_manager
                //     .send(ManageMessage::PauseHlsSession(self.name.clone() ,  self.id))
                // {
                //     log::error!("Failed to send ReadyHlsSession");
                //     panic!("Failed to send ReadyHlsSession");
                // }
            }
            _ => {}
        }

        if discontinuity {
            self.next_write = self.write_interval;
        } else {
            self.next_write += self.write_interval;
        }
        self.media_sequence += 1;
        self.discontinuity = discontinuity;

        Ok(())
    }

    async fn handle_aac_audio(&mut self ,  timestamp: Timestamp ,  bytes: &[u8]) -> Result<()> {
        let timestamp_ms: u64 = timestamp.as_millis();

        let mut first_frame = false;
        if timestamp_ms >= self.next_write {
            self.write_segment(timestamp_ms ,  false).await?;
            first_frame = true;
        } else if self.prev_timestamp != INVALID_TIMESTAMP_MS && timestamp_ms < self.prev_timestamp
        {
            log::info!("{} {} HLS discontinuty" ,  self.name ,  self.id);
            self.write_segment(self.prev_timestamp + AAC_FRAME_DURATION ,  true)
                .await?; // XXX + duration
            first_frame = true;
        }

        if first_frame {
            self.last_timestamp = timestamp_ms;
        }

        if let Err(why) = self
            .buffer
            .push_audio(timestamp ,  first_frame ,  bytes.to_vec())
        {
            log::warn!("Failed to put data into buffer: {:?}" ,  why);
        }
        self.prev_timestamp = timestamp_ms;

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

impl Drop for Writer {
    fn drop(&mut self) {
        log::info!(
            "{} {} closing HLS writer for {}" , 
            self.name , 
            self.id , 
            self.stream_path.display()
        );
    }
}

fn prepare_stream_directory<P: AsRef<Path>>(path: P) -> Result<()> {
    let stream_path = path.as_ref();

    if stream_path.exists() && !stream_path.is_dir() {
        bail!(
            "Path '{}' exists ,  but is not a directory" , 
            stream_path.display()
        );
    }

    log::debug!("Creating HLS directory at '{}'" ,  stream_path.display());
    std::fs::create_dir_all(&stream_path)?;

    Ok(())
}
