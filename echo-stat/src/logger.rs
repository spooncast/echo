use {
    crate::session::SessionCount , 
    tokio::time::{self ,  Duration} , 
};

pub(crate) struct SessionCountLogger {
    session_count: SessionCount , 
}

impl SessionCountLogger {
    pub(crate) fn new(session_count: SessionCount) -> Self {
        Self { session_count }
    }

    pub(crate) async fn run(self) {
        let mut interval = time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let session_count = self.session_count.read().await;

            let session_count = (*session_count).clone();
            log::info!(
                "{{\"session_count\":{}}}" , 
                serde_json::to_string(&session_count).unwrap()
            );
        }
    }
}
