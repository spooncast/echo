use {
    crate::{peer::Peer ,  Error} , 
    anyhow::Result , 
    echo_core::{
        session::{IdGenerator ,  ManagerHandle} , 
        Config , 
    } , 
    std::{io::ErrorKind as IoErrorKind ,  time::Duration} , 
    tokio::{net::TcpListener ,  prelude::*} , 
};

pub struct Service {
    config: Config , 
    session_manager: ManagerHandle , 
    id_gen: IdGenerator , 
}

impl Service {
    pub fn new(session_manager: ManagerHandle ,  config: Config ,  id_gen: IdGenerator) -> Self {
        Self {
            config , 
            session_manager , 
            id_gen , 
        }
    }

    pub async fn run(self) {
        if let Err(err) = self.handle_rtmp().await {
            log::error!("{}" ,  err);
        }
    }

    async fn handle_rtmp(&self) -> Result<()> {
        let addr = &self.config.rtmp_addr;
        let mut listener = TcpListener::bind(addr).await?;
        log::info!("Listening for RTMP connections on {}" ,  addr);

        loop {
            let (tcp_stream ,  _addr) = listener.accept().await?;
            tcp_stream.set_keepalive(Some(Duration::from_secs(30)))?;
            self.process(tcp_stream);
        }
    }

    fn process<S>(&self ,  stream: S)
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static , 
    {
        let id = self.id_gen.fetch_next();
        log::info!("New client connection: {}" ,  id);
        let peer = Peer::new(
            id , 
            stream , 
            self.session_manager.clone() , 
            self.config.clone() , 
        );

        tokio::spawn(async move {
            if let Err(err) = peer.run().await {
                match err {
                    Error::Disconnected(e) if e.kind() == IoErrorKind::ConnectionReset => () , 
                    e => log::error!("{}" ,  e) , 
                }
            }
        });
    }
}
