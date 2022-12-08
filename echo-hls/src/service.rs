use {
    crate::{session_cleaner ,  writer::Writer} , 
    anyhow::{bail ,  Result} , 
    m3u8_rs::playlist::{MediaPlaylist ,  Playlist} , 
    echo_core::{
        session::{self ,  EventKind ,  EventMessage ,  ManageMessage ,  ManagerHandle} , 
        Config , 
    } , 
    std::{collections::HashMap ,  path::Path ,  time::Duration ,  time::SystemTime} , 
    tokio::{fs ,  io::AsyncReadExt} , 
    warp::{
        http::header::{self ,  HeaderMap ,  HeaderValue} , 
        Filter ,  Reply , 
    } , 
};

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
pub(crate) const PREROLE: &'static str = "prerole";
pub(crate) const PREROLE_PATH: &'static str = "/prerole";

pub struct Service {
    config: Config , 
    session_manager: ManagerHandle , 
}

impl Service {
    pub fn new(session_manager: ManagerHandle ,  config: Config) -> Self {
        Self {
            config , 
            session_manager , 
        }
    }

    pub async fn run(self) {
        let hls_root = self.config.hls_root_dir.clone();
        log::info!("HLS directory located at '{}'" ,  hls_root.display());

        if let Err(err) = create_dir(&hls_root).await {
            panic!("{}" ,  err);
        }

        if let Err(err) = cleanup_dir(&hls_root).await {
            log::error!("{}" ,  err);
            return;
        }

        let prerole_dir = self.config.hls_prerole_dir.clone();
        let prerole_path = prerole_dir.join("live.m3u8");
        let prerole_pl = match read_prerole_m3u8(prerole_path).await {
            Ok(pl) => pl , 
            Err(err) => panic!("{}" ,  err) , 
        };
        let prerole_dur = prerole_pl
            .segments
            .iter()
            .fold(Duration::default() ,  |acc ,  seg| acc + seg.duration);

        let sess_cleaner = session_cleaner::SessionCleaner::new();
        let sess_cleaner_sender = sess_cleaner.sender();
        tokio::spawn(async move { sess_cleaner.run().await });

        if self.config.hls_web_enabled {
            let addr = self.config.hls_web_addr;
            let web_path = self.config.hls_web_path.clone();

            let mut headers = HeaderMap::new();
            headers.insert(
                header::SERVER , 
                HeaderValue::from_str(&format!("Echo/{}" ,  VERSION.unwrap())).unwrap() , 
            );
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_ORIGIN , 
                HeaderValue::from_static("*") , 
            );
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_CREDENTIALS , 
                HeaderValue::from_static("true") , 
            );
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_METHODS , 
                HeaderValue::from_static("OPTIONS ,  GET ,  HEAD") , 
            );
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS , 
                HeaderValue::from_static(
                    "Content-Type ,  User-Agent ,  If-Modified-Since ,  Cache-Control ,  Range" , 
                ) , 
            );
            let routes = warp::path(PREROLE)
                .and(warp::fs::dir(prerole_dir))
                .or(warp::path(web_path).and(warp::fs::dir(hls_root)))
                .unify()
                .map(|reply: warp::fs::File| {
                    let path = reply.path().to_string_lossy();
                    if path.ends_with(".m3u8") {
                        let mut res = reply.into_response();
                        res.headers_mut().insert(
                            header::CONTENT_TYPE , 
                            HeaderValue::from_static("application/vnd.apple.mpegurl") , 
                        );
                        res.headers_mut()
                            .insert(header::CACHE_CONTROL ,  HeaderValue::from_static("max-age=1"));
                        res
                    } else if path.ends_with(".ts") {
                        let mut res = reply.into_response();
                        res.headers_mut()
                            .insert(header::CONTENT_TYPE ,  HeaderValue::from_static("video/mp2t"));
                        res.headers_mut().insert(
                            header::CACHE_CONTROL , 
                            HeaderValue::from_static("max-age=600") , 
                        );
                        res
                    } else {
                        reply.into_response()
                    }
                })
                // cors
                .with(warp::reply::with::headers(headers))
                .with(warp::log("echo-hls-web"));

            log::info!("Start HLS web server");
            tokio::spawn(async move {
                warp::serve(routes).run(addr).await;
            });
        }

        let (trigger ,  mut trigger_watcher) = session::trigger_channel();

        if let Err(_) = self.session_manager.send(ManageMessage::RegisterTrigger(
            EventKind::CreateSession , 
            trigger , 
        )) {
            log::error!("Failed to register CreateSession trigger");
            panic!("Failed to register CreateSession trigger");
        }

        // hls ext-x-sequence 처리를 위한 map 정의
        let mut hs: HashMap<String ,  (u32 ,  SystemTime)> = HashMap::new();

        while let Some((name ,  event)) = trigger_watcher.recv().await {
            match event {
                EventMessage::CreateSession(id ,  session_watcher) => {
                    let session_manager = self.session_manager.clone();
                    let begin_time = SystemTime::now();

                    // 동일한 name 이면 기존의 seq를 기존 값+100000 부터 시작.
                    // 재연결시 ext-x-sequence 연속성을 위한 부분
                    let seq = match hs.get(&name) {
                        Some(val) => val.0 + 100000 , 
                        None => 0 , 
                    };

                    hs.insert(name.clone() ,  (seq ,  begin_time));

                    match Writer::create(
                        name.clone() , 
                        id , 
                        session_manager , 
                        session_watcher , 
                        sess_cleaner_sender.clone() , 
                        &self.config , 
                        &prerole_pl , 
                        &prerole_dur , 
                        seq , 
                    ) {
                        Ok(writer) => {
                            tokio::spawn(async move { writer.run().await.unwrap() });
                        }
                        Err(err) => log::error!("Failed to create writer: {:?}" ,  err) , 
                    }

                    // 2시간 넘어서는 hls_sequence 삭제
                    // writer객체에서 해주는 방행으로 개선되어야 함
                    let mut delete_list: Vec<String> = vec![];
                    let mut total_hashmap = 0;

                    for (key ,  value) in &hs {
                        let difference = begin_time.duration_since(value.1).unwrap().as_secs();
                        if difference > self.config.ttl_max_duration.as_secs() + 600 {
                            delete_list.push(String::from(key));
                        }
                        total_hashmap += 1;
                    }

                    for delete_key in delete_list.iter_mut() {
                        let val = hs.remove(delete_key);
                        log::info!(
                            "remove hls-seq {} {:?} ,  total: {}" , 
                            delete_key , 
                            val , 
                            total_hashmap
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

async fn create_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    if let Ok(attr) = fs::metadata(&path).await {
        if attr.is_dir() {
            return Ok(());
        }
    }

    fs::create_dir_all(&path).await?;
    log::info!("create HLS directory {}" ,  path.as_ref().display());

    Ok(())
}

async fn cleanup_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();

    let attr = fs::metadata(&path).await?;

    if attr.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let child_path = entry?.path();

            if child_path.is_dir() {
                fs::remove_dir_all(&child_path).await?;
                log::info!("remove old directory {}" ,  child_path.display());
            } else {
                fs::remove_file(&child_path).await?;
                log::info!("remove old file {}" ,  child_path.display());
            }
        }
    } else {
        bail!("HLS root is not a directory")
    }

    log::info!("HLS directory purged");

    Ok(())
}

async fn read_prerole_m3u8<P: AsRef<Path>>(path: P) -> Result<MediaPlaylist> {
    let path = path.as_ref();

    let mut file = fs::File::open(path).await?;
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).await?;

    let parsed = m3u8_rs::parse_playlist_res(&bytes);

    let mut pl = match parsed {
        Ok(Playlist::MediaPlaylist(pl)) => pl , 
        Ok(Playlist::MasterPlaylist(_)) => {
            bail!("must be media playlist: {}" ,  path.to_string_lossy())
        }
        Err(e) => bail!("{:?}" ,  e) , 
    };

    let segs: Vec<_> = pl
        .segments
        .iter()
        .map(|seg| {
            let mut seg = seg.clone();
            seg.uri = format!("{}/{}" ,  PREROLE_PATH ,  seg.uri);
            seg
        })
        .collect();
    pl.segments = segs;

    Ok(pl)
}
