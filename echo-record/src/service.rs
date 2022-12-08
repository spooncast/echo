use {
    crate::recorder::Recorder , 
    anyhow::Result , 
    echo_core::{
        session::{self ,  EventKind ,  EventMessage ,  ManageMessage ,  ManagerHandle} , 
        Config , 
    } , 
    std::path::Path , 
    tokio::fs , 
};

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
        let record_root = self.config.record_root_dir.clone();
        log::info!("Recording directory located at '{}'" ,  record_root.display());
        if let Err(err) = create_dir(&record_root).await {
            panic!("{}" ,  err);
        }

        let (trigger ,  mut trigger_watcher) = session::trigger_channel();

        if let Err(_) = self.session_manager.send(ManageMessage::RegisterTrigger(
            EventKind::CreateSession , 
            trigger , 
        )) {
            log::error!("Failed to register CreateSession trigger");
            panic!("Failed to register CreateSession trigger");
        }

        while let Some((name ,  event)) = trigger_watcher.recv().await {
            match event {
                EventMessage::CreateSession(id ,  session_watcher) => {
                    let session_manager = self.session_manager.clone();
                    match Recorder::create(name ,  id ,  session_manager ,  session_watcher ,  &self.config)
                    {
                        Ok(recorder) => {
                            tokio::spawn(async move { recorder.run().await.unwrap() });
                        }
                        Err(why) => log::error!("Failed to create recorder: {:?}" ,  why) , 
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
    log::info!("create record directory {}" ,  path.as_ref().display());

    Ok(())
}
