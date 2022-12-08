use {
    anyhow::Result , 
    git_version::git_version , 
    echo_core::{session::SessionManager ,  Config} , 
    echo_transfer , 
};

const GIT_VERSION: &str = git_version!();

#[actix_rt::main]
async fn main() -> Result<()> {
    let config = match Config::load() {
        Ok(c) => c , 
        Err(err) => return Err(err.into()) , 
    };

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        if args[1] == "-v" {
            println!("{}" ,  GIT_VERSION);
            println!("{:?}" ,  config);
            std::process::exit(0);
        } else {
            println!("usage: echoserver [-v]");
            std::process::exit(1);
        }
    }

    log4rs::init_file(config.log4rs_file.clone() ,  Default::default()).unwrap();

    log::info!("START SERVER");
    log::info!("version = {}" ,  GIT_VERSION);
    log::info!("config = {:?}" ,  config);

    let mut handles = Vec::new();

    let session_manager = SessionManager::new(config.clone());
    let manager_handle = session_manager.handle();
    let id_gen = session_manager.id_generator();
    handles.push(tokio::spawn(session_manager.run()));

    if config.hls_enabled {
        handles.push(tokio::spawn({
            echo_hls::Service::new(manager_handle.clone() ,  config.clone()).run()
        }));
    }

    #[cfg(feature = "rtmp")]
    if config.rtmp_enabled {
        handles.push(tokio::spawn({
            echo_rtmp::Service::new(manager_handle.clone() ,  config.clone() ,  id_gen.clone()).run()
        }));
    }

    #[cfg(feature = "record")]
    if config.record_enabled {
        handles.push(tokio::spawn({
            echo_record::Service::new(manager_handle.clone() ,  config.clone()).run()
        }))
    }

    // #[cfg(feature = "stat")]
    // if config.stat_enabled {
    //     handles.push(tokio::spawn({
    //         echo_stat::Service::new(manager_handle.clone() ,  config.clone()).run()
    //     }))
    // }

    echo_transfer::Service::new(manager_handle ,  config.clone() ,  id_gen)
        .run()
        .await?;

    log::info!("STOPPING SERVER");

    // TODO: graceful shutdown
    // wait for all spawned processes to complete
    // for handle in handles {
    //     handle.await?;
    // }

    Ok(())
}
