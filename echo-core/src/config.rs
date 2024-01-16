use {
    config,
    serde::Deserialize,
    std::{
        net::{IpAddr, SocketAddr},
        path::PathBuf,
        time::Duration,
    },
};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub log4rs_file: PathBuf,

    pub echo_enabled: bool,
    #[serde(default = "default_echo_addr")]
    pub echo_addr: SocketAddr,
    pub echo_priv_key: String,
    pub echo_srt_priv_ip: IpAddr,
    pub echo_srt_pub_ip: Option<IpAddr>,
    #[serde(default = "default_echo_srt_min_port")]
    pub echo_srt_min_port: u16,
    #[serde(default = "default_echo_srt_max_port")]
    pub echo_srt_max_port: u16,
    #[serde(
        default = "default_echo_srt_connection_timeout",
        with = "duration_format"
    )]
    pub echo_srt_connection_timeout: Duration,
    #[serde(default = "default_echo_srt_read_timeout", with = "duration_format")]
    pub echo_srt_read_timeout: Duration,
    #[serde(default = "default_echo_srt_latency", with = "duration_format")]
    pub echo_srt_latency: Duration,

    pub hls_enabled: bool,
    pub hls_root_dir: PathBuf,
    #[serde(default = "default_hls_target_duration", with = "duration_format")]
    pub hls_target_duration: Duration,
    #[serde(default = "default_hls_prerole_dir")]
    pub hls_prerole_dir: PathBuf,
    pub hls_web_enabled: bool,
    #[serde(default = "default_hls_web_addr")]
    pub hls_web_addr: SocketAddr,
    pub hls_web_path: String,

    pub rtmp_enabled: bool,
    #[serde(default = "default_rtmp_addr")]
    pub rtmp_addr: SocketAddr,
    #[serde(default = "default_rtmp_connection_timeout", with = "duration_format")]
    pub rtmp_connection_timeout: Duration,

    pub record_enabled: bool,
    pub record_root_dir: PathBuf,
    pub record_append: bool,

    pub stat_enabled: bool,
    pub stat_web_enabled: bool,
    #[serde(default = "default_stat_web_addr")]
    pub stat_web_addr: SocketAddr,

    #[serde(default = "default_ttl_max_duration", with = "duration_format")]
    pub ttl_max_duration: Duration,
}

fn default_echo_addr() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 5021))
}

fn default_echo_srt_connection_timeout() -> Duration {
    Duration::from_secs(1800)
}

fn default_echo_srt_read_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_hls_target_duration() -> Duration {
    Duration::from_secs(1)
}

fn default_hls_prerole_dir() -> PathBuf {
    PathBuf::from("/var/echo/prerole")
}

fn default_hls_web_addr() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 8080))
}

fn default_rtmp_addr() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 1935))
}

fn default_rtmp_connection_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_echo_srt_min_port() -> u16 {
    30000
}

fn default_echo_srt_max_port() -> u16 {
    49150
}

fn default_echo_srt_latency() -> Duration {
    Duration::from_millis(50)
}

fn default_stat_web_addr() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 8088))
}

fn default_ttl_max_duration() -> Duration {
    Duration::from_secs(2 * 3600)
}

mod duration_format {
    use {
        serde::{self, Deserialize, Deserializer},
        std::time::Duration,
    };

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let d = f64::deserialize(deserializer)?;
        Ok(Duration::from_secs_f64(d))
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            log4rs_file: PathBuf::from("/etc/echo/log4rs.yml"),

            // SRT
            echo_enabled: true,
            echo_addr: default_echo_addr(),
            echo_priv_key: String::from("0759230f81a40bef363d741f6b2ea274"),
            echo_srt_priv_ip: IpAddr::from([127, 0, 0, 1]),
            echo_srt_pub_ip: None,
            echo_srt_min_port: default_echo_srt_min_port(),
            echo_srt_max_port: default_echo_srt_max_port(),
            echo_srt_connection_timeout: default_echo_srt_connection_timeout(),
            echo_srt_read_timeout: default_echo_srt_read_timeout(),
            echo_srt_latency: default_echo_srt_latency(),

            // HLS
            hls_enabled: true,
            hls_root_dir: PathBuf::from("."),
            hls_target_duration: default_hls_target_duration(),
            hls_prerole_dir: default_hls_prerole_dir(),
            hls_web_enabled: true,
            hls_web_addr: default_hls_web_addr(),
            hls_web_path: String::from("live"),

            // RTMP
            rtmp_enabled: true,
            rtmp_addr: default_rtmp_addr(),
            rtmp_connection_timeout: default_rtmp_connection_timeout(),

            // Record
            record_enabled: true,
            record_root_dir: PathBuf::from("."),
            record_append: false,

            // stat
            stat_enabled: true,
            stat_web_enabled: false,
            stat_web_addr: default_stat_web_addr(),

            // ttl
            ttl_max_duration: default_ttl_max_duration(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Config, config::ConfigError> {
        let mut s = config::Config::default();
        s.merge(config::Environment::new())?;
        let mut config: Config = s.try_into()?;
        config.check()?;
        Ok(config)
    }

    fn check(&mut self) -> Result<(), config::ConfigError> {
        if self.echo_priv_key.len() != 32 {
            return Err(config::ConfigError::Message(String::from(
                "The length of ECHO_PRIV_KEY must be 32",
            )));
        }
        if self.echo_srt_min_port >= self.echo_srt_max_port {
            return Err(config::ConfigError::Message(String::from(
                "ECHO_SRT_MAX_PORT must be greater than ECHO_SRT_MIN_PORT",
            )));
        }
        if self.echo_srt_min_port < 6_970 {
            return Err(config::ConfigError::Message(String::from(
                "ECHO_SRT_MIN_PORT must be greater than or equal to 6970",
            )));
        }
        if self.echo_srt_max_port > 49_150 {
            return Err(config::ConfigError::Message(String::from(
                "ECHO_SRT_MAX_PORT must be less than or equal to 49150",
            )));
        }
        if self.echo_srt_connection_timeout < Duration::from_secs(10) {
            return Err(config::ConfigError::Message(String::from(
                "ECHO_SRT_CONNECTION_TIMEOUT must be greater than or equal to 10",
            )));
        }
        if self.echo_srt_read_timeout < Duration::from_secs(8) {
            return Err(config::ConfigError::Message(String::from(
                "ECHO_SRT_READ_TIMEOUT must be greater than or equal to 8",
            )));
        }

        Ok(())
    }
}
