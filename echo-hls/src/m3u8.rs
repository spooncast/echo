use {
    crate::{
        service::PREROLE_PATH , 
        session_cleaner::{self ,  CleanerItem} , 
    } , 
    anyhow::Result , 
    m3u8_rs::playlist::{MediaPlaylist ,  MediaSegment} , 
    echo_core::session::{AppName ,  ManagerHandle ,  SessionId} , 
    std::{cmp ,  path::PathBuf ,  time::Duration} , 
    tempfile::NamedTempFile , 
    tokio::{
        fs::{self ,  File} , 
        io::AsyncWriteExt , 
    } , 
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug ,  PartialEq)]
pub(crate) enum PlaylistState {
    NotReady , 
    Ready , 
    Unchanged , 
}

pub struct Playlist {
    file_path: PathBuf , 
    current_duration: Duration , 
    playlist_duration: Duration , 
    playlist_min_duration: Duration , 
    cache_duration: Duration , 
    state: PlaylistState , 
    playlist: MediaPlaylist , 
    session_cleaner: session_cleaner::Sender , 
}

impl Playlist {
    pub(crate) fn new<P>(
        path: P , 
        prerole: &MediaPlaylist , 
        prerole_dur: &Duration , 
        target_duration: Duration , 
        session_cleaner: session_cleaner::Sender , 
    ) -> Self
    where
        P: Into<PathBuf> , 
    {
        let mut tar_dur = target_duration.as_millis() as u64;
        tar_dur = if tar_dur < 1000 {
            log::warn!("target duration is smaller than 1 second");
            1000
        } else if tar_dur > 8000 {
            log::warn!("target duration is larger than 8 seconds");
            8000
        } else {
            // 1000 <= tar_dur && tar_dur <= 8000
            tar_dur
        };
        log::warn!("hls target duration {}", tar_dur);

        let playlist_duration = Duration::from_millis(12_000 * 1000 / tar_dur);
        let playlist_min_duration = Duration::from_millis(6_000 * 1000 / tar_dur);
        let cache_duration = Duration::from_millis(38_000 * 1000 / tar_dur);

        let mut playlist = prerole.clone();
        playlist.version = 3;
        playlist.target_duration = Duration::from_millis(tar_dur);
        playlist.media_sequence = 0;

        Self {
            file_path: path.into() , 
            current_duration: prerole_dur.clone() , 
            playlist_duration , 
            playlist_min_duration , 
            cache_duration , 
            state: PlaylistState::NotReady , 
            playlist , 
            session_cleaner , 
        }
    }

    fn schedule_for_deletion(&mut self ,  amount: usize) {
        let segments_to_delete: Vec<_> = self.playlist.segments.drain(..amount).collect();
        let paths: Vec<_> = segments_to_delete
            .iter()
            .filter_map(|seg| {
                self.current_duration -= seg.duration;
                if (&seg.uri).starts_with(PREROLE_PATH) {
                    None
                } else {
                    Some(self.file_path.parent().unwrap().join(&seg.uri))
                }
            })
            .collect();

        let _ = self
            .session_cleaner
            .send((self.cache_duration ,  CleanerItem::Chunks(paths)))
            .map_err(|_| log::error!("failed to send file to be deleted"));
    }

    pub(crate) async fn add_media_segment<S>(
        &mut self , 
        uri: S , 
        duration: Duration , 
        discontinuity: bool , 
    ) -> Result<PlaylistState>
    where
        S: Into<String> , 
    {
        let mut segment = MediaSegment::empty();
        segment.duration = duration;
        segment.title = Some("".into()); // XXX adding empty title here ,  because implementation is broken
        segment.uri = uri.into();
        if discontinuity {
            segment.discontinuity = true;
        }

        if self.current_duration >= self.playlist_duration {
            self.schedule_for_deletion(1);
        }

        self.playlist.media_sequence += 1;
        self.current_duration += duration;
        self.playlist.segments.push(segment);

        if let Err(err) = self.atomic_update().await {
            log::error!("Failed to update playlist: {:?}" ,  err);
            Err(err)
        } else if self.current_duration >= self.playlist_min_duration {
            if self.state == PlaylistState::NotReady {
                self.state = PlaylistState::Ready;
                Ok(PlaylistState::Ready)
            } else {
                Ok(PlaylistState::Unchanged)
            }
        } else {
            if self.state == PlaylistState::Ready {
                self.state = PlaylistState::NotReady;
                Ok(PlaylistState::NotReady)
            } else {
                Ok(PlaylistState::Unchanged)
            }
        }
    }

    async fn atomic_update(&mut self) -> Result<()> {
        let tmp_file = tempfile::Builder::new()
            .prefix(".playlist.m3u8")
            .suffix(".tmp")
            .tempfile_in(&self.hls_root())?;

        self.write_temporary_file(&tmp_file).await?;
        fs::rename(&tmp_file.path() ,  &self.file_path).await?;

        Ok(())
    }

    fn hls_root(&self) -> PathBuf {
        self.file_path
            .parent()
            .expect("No parent directory for playlist")
            .into()
    }

    async fn write_temporary_file(&mut self ,  tmp_file: &NamedTempFile) -> Result<()> {
        let mut buffer: Vec<u8> = Vec::new(); // XXX
        self.playlist.write_to(&mut buffer)?;

        let mut file = File::create(tmp_file.path()).await?;
        file.write_all(&buffer).await?;

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&tmp_file.path()).await?.permissions();
            perms.set_mode(0o644);
            fs::set_permissions(&tmp_file.path() ,  perms).await?;
        }

        Ok(())
    }

    pub(crate) async fn release(
        &mut self , 
        name: AppName , 
        id: SessionId , 
        session_manager: ManagerHandle , 
    ) {
        let current_duration = cmp::min(self.current_duration ,  self.cache_duration);

        self.schedule_for_deletion(self.playlist.segments.len()); // remove all TS files

        let _ = self
            .session_cleaner
            .send((
                self.cache_duration + self.playlist_min_duration ,  // XXX
                CleanerItem::Manifest(self.file_path.clone()) , 
            ))
            .map_err(|_| log::error!("failed to send file to be deleted"));

        let parent_dir = self.file_path.parent().unwrap().to_owned();
        let _ = self
            .session_cleaner
            .send((
                self.cache_duration + (self.playlist_min_duration * 2) ,  // XXX
                CleanerItem::Directory(parent_dir) , 
            ))
            .map_err(|_| log::error!("failed to send directory to be deleted"));

        let _ = self
            .session_cleaner
            .send((
                current_duration , 
                CleanerItem::Session(name ,  id ,  session_manager) , 
            ))
            .map_err(|_| log::error!("failed to send session to be deleted"));
    }
}
