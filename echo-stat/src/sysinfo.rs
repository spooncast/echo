use {
    serde::Serialize , 
    std::convert::Infallible , 
    sysinfo::{System ,  SystemExt} , 
};

#[derive(Serialize)]
struct SysLoad {
    one: f64 , 
    five: f64 , 
    fifteen: f64 , 
}

#[derive(Serialize)]
pub(crate) struct SysUsage {
    load: SysLoad , 
    memory: f32 , 
}

impl SysUsage {
    pub(crate) fn now() -> Self {
        let sys = System::new_all();
        let sysload = sys.get_load_average();
        SysUsage {
            load: SysLoad {
                one: sysload.one , 
                five: sysload.five , 
                fifteen: sysload.fifteen , 
            } , 
            memory: sys.get_used_memory() as f32 * 100.0 / sys.get_total_memory() as f32 , 
        }
    }
}

pub(crate) async fn sys_usage() -> Result<impl warp::Reply ,  Infallible> {
    let sysusage = SysUsage::now();
    Ok(warp::reply::json(&sysusage))
}
