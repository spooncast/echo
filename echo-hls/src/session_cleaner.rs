use {
    echo_core::session::{AppName ,  ManageMessage ,  ManagerHandle ,  SessionId} , 
    std::path::PathBuf , 
    tokio::{
        fs , 
        stream::StreamExt , 
        sync::mpsc , 
        time::{DelayQueue ,  Duration} , 
    } , 
};

#[derive(Debug)]
pub(crate) enum CleanerItem {
    Chunks(Vec<PathBuf>) , 
    Manifest(PathBuf) , 
    Directory(PathBuf) , 
    Session(AppName ,  SessionId ,  ManagerHandle) , 
}

type Batch = CleanerItem;
type Message = (Duration ,  Batch);
pub(crate) type Sender = mpsc::UnboundedSender<Message>;
type Receiver = mpsc::UnboundedReceiver<Message>;

pub struct SessionCleaner {
    items: DelayQueue<Batch> , 
    sender: Sender , 
    receiver: Receiver , 
}

impl SessionCleaner {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (sender ,  receiver) = mpsc::unbounded_channel();

        Self {
            items: DelayQueue::new() , 
            sender , 
            receiver , 
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some((duration ,  item)) = self.receiver.next() => {
                    self.items.insert(item ,  duration);
                }
                Some(item) = self.items.next() => {
                    match item {
                        Ok(expired) => remove_item(expired.get_ref()).await , 
                        Err(err) => log::error!("{}" ,  err) , 
                    }
                }
                else => {
                    break;
                }
            }
        }
        log::info!("end file cleaner")
    }

    pub(crate) fn sender(&self) -> Sender {
        self.sender.clone()
    }
}

async fn remove_item(item: &CleanerItem) {
    match item {
        CleanerItem::Chunks(paths) => {
            for path in paths {
                if let Err(err) = fs::remove_file(path).await {
                    log::error!("Failed to remove file '{}': {}" ,  path.display() ,  err);
                }
            }
        }
        CleanerItem::Manifest(path) => {
            let parent_dir = path.parent().unwrap().to_owned();
            if let Ok(entries) = fs::read_dir(parent_dir).await {
                // only manifest. if there are other files ,  it has been restarted.
                if entries.collect::<Vec<_>>().await.len() == 1 {
                    if let Err(err) = fs::remove_file(path).await {
                        log::error!("Failed to remove file '{}': {}" ,  path.display() ,  err);
                    }
                }
            }
        }
        CleanerItem::Directory(path) => {
            // If remove_dir_all() is called ,  the session restart fails.
            if let Err(err) = fs::remove_dir(path).await {
                log::error!("Failed to remove directory '{}': {}" ,  path.display() ,  err);
            }
        }
        CleanerItem::Session(name ,  id ,  session_manager) => {
            if let Err(_) =
                session_manager.send(ManageMessage::ReleaseHlsSession(name.clone() ,  *id))
            {
                log::error!("Failed to send ReleaseHlsSession");
                panic!("Failed to send ReleaseHlsSession");
            }
        }
    }
}
