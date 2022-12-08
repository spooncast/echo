use {
    chrono::{DateTime ,  Utc} , 
    echo_core::session::{AppName ,  InputQuality ,  SessionId} , 
    echo_types::Protocol , 
    serde::Serialize , 
    std::{collections::HashMap ,  convert::Infallible ,  path::PathBuf ,  sync::Arc} , 
    tokio::sync::RwLock , 
};

pub(crate) type Sessions = Arc<RwLock<HashMap<SessionId ,  Session>>>;
pub(crate) type SessionCount = Arc<RwLock<Count>>;

#[derive(Serialize)]
pub(crate) struct Session {
    // remove pub
    pub(crate) name: AppName , 
    pub(crate) protocol: Protocol , 
    pub(crate) ingest_start_time: DateTime<Utc> , 
    pub(crate) ingest_end_time: Option<DateTime<Utc>> , 
    pub(crate) hls_ready_time: Option<DateTime<Utc>> , 
    pub(crate) hls_end_time: Option<DateTime<Utc>> , 
    pub(crate) hls_path: Option<String> , 
    pub(crate) record_start_time: Option<DateTime<Utc>> , 
    pub(crate) record_complete_time: Option<DateTime<Utc>> , 
    pub(crate) record_path: Option<PathBuf> , 
    pub(crate) ingest_quality: Option<InputQuality> , 
}

impl Session {
    pub(crate) fn new(name: AppName ,  id: SessionId ,  protocol: Protocol) -> Self {
        let ret = Self {
            name , 
            protocol , 
            ingest_start_time: Utc::now() , 
            ingest_end_time: None , 
            hls_ready_time: None , 
            hls_end_time: None , 
            hls_path: None , 
            record_start_time: None , 
            record_complete_time: None , 
            record_path: None , 
            ingest_quality: None , 
        };
        log::info!(
            "{{\"session_id\":{} , \"session_event\":\"created\" , \"session_info\":{}}}" , 
            id , 
            serde_json::to_string(&ret).unwrap()
        );
        ret
    }

    pub(crate) fn release_ingest(&mut self) {
        self.ingest_end_time = Some(Utc::now());
    }

    pub(crate) fn ready_hls(&mut self ,  path: String) -> bool {
        if self.hls_ready_time.is_none() {
            self.hls_ready_time = Some(Utc::now());
            self.hls_path = Some(path);
            true
        } else {
            false
        }
    }

    pub(crate) fn release_hls(&mut self) -> bool {
        if self.hls_end_time.is_none() {
            self.hls_end_time = Some(Utc::now());
            self.hls_ready_time.is_some()
        } else {
            false
        }
    }

    pub(crate) fn start_record(&mut self) -> bool {
        if self.record_start_time.is_none() {
            self.record_start_time = Some(Utc::now());
            true
        } else {
            false
        }
    }

    pub(crate) fn complete_record(&mut self ,  path: PathBuf) -> bool {
        if self.record_complete_time.is_none() {
            self.record_complete_time = Some(Utc::now());
            self.record_path = Some(path);
            self.record_start_time.is_some()
        } else {
            false
        }
    }

    pub(crate) fn is_finished_and_log(&self ,  id: SessionId) -> bool {
        if self.ingest_end_time.is_some()
            && (self.hls_ready_time.is_none() || self.hls_end_time.is_some())
            && (self.record_start_time.is_none() || self.record_complete_time.is_some())
        {
            log::info!(
                "{{\"session_id\":{} , \"session_event\":\"destroyed\" , \"session_info\":{}}}" , 
                id , 
                serde_json::to_string(self).unwrap()
            );
            true
        } else {
            false
        }
    }

    pub(crate) fn quality_log(&mut self ,  id: SessionId ,  quality: InputQuality) {
        self.ingest_quality = Some(quality);
        log::info!(
            "{{\"session_id\":{} , \"session_event\":\"report\" , \"session_info\":{}}}" , 
            id , 
            serde_json::to_string(self).unwrap()
        );
    }
}

#[derive(Debug ,  Default ,  Clone ,  Copy ,  Serialize)]
pub(crate) struct InputCount {
    pub(crate) total: usize , 
    pub(crate) srt: usize , 
    pub(crate) rtmp: usize , 
}

#[derive(Debug ,  Default ,  Clone ,  Copy ,  Serialize)]
pub(crate) struct OutputCount {
    pub(crate) total: usize , 
    pub(crate) hls: usize , 
    pub(crate) record: usize , 
}

#[derive(Debug ,  Default ,  Clone ,  Copy ,  Serialize)]
pub(crate) struct Count {
    pub(crate) total: usize , 
    pub(crate) input: InputCount , 
    pub(crate) output: OutputCount , 
}

pub(crate) async fn count_input_sessions(
    count: SessionCount , 
) -> Result<impl warp::Reply ,  Infallible> {
    let count = count.read().await;
    let input = count.input;
    Ok(warp::reply::json(&input))
}

pub(crate) async fn count_sessions(count: SessionCount) -> Result<impl warp::Reply ,  Infallible> {
    let count = count.read().await;
    Ok(warp::reply::json(&*count))
}

pub(crate) async fn list_sessions(sessions: Sessions) -> Result<impl warp::Reply ,  Infallible> {
    let sessions = sessions.read().await;
    let ids = sessions.keys().collect::<Vec<_>>();
    Ok(warp::reply::json(&ids))
}

pub(crate) async fn get_session(
    sessions: Sessions , 
    id: SessionId , 
) -> Result<impl warp::Reply ,  warp::Rejection> {
    let sessions = sessions.read().await;
    if let Some(session) = sessions.get(&id) {
        Ok(warp::reply::json(session))
    } else {
        Err(warp::reject::not_found())
    }
}
