//! Provides access to the symbolserver config
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::borrow::Cow;
use std::io::BufReader;

use num_cpus;
use serde_yaml;
use url::Url;
use rusoto::Region;
use chrono::Duration;
use log::LogLevelFilter;

use super::{Result, ResultExt, ErrorKind};
use super::utils::{is_docker, RegexFilter};


#[derive(Deserialize, Debug, Default, Clone)]
struct AwsConfig {
    access_key: Option<String>,
    secret_key: Option<String>,
    bucket_url: Option<String>,
    region: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
struct ServerConfig {
    host: Option<String>,
    port: Option<u16>,
    healthcheck_ttl: Option<i64>,
    threads: Option<usize>,
}

#[derive(Deserialize, Debug, Default, Clone)]
struct LogConfig {
    level: Option<String>,
    file: Option<PathBuf>,
}

#[derive(Deserialize, Debug, Default, Clone)]
struct SyncConfig {
    #[serde(default)]
    include: RegexFilter,
    #[serde(default)]
    exclude: RegexFilter,
    interval: Option<i64>,
}

/// Central config object that exposes the information from
/// the symbolserver yaml config.
#[derive(Deserialize, Debug, Default, Clone)]
pub struct Config {
    #[serde(default)]
    aws: AwsConfig,
    #[serde(default)]
    server: ServerConfig,
    #[serde(default)]
    log: LogConfig,
    symbol_dir: Option<PathBuf>,
    #[serde(default)]
    sync: SyncConfig,
}

impl Config {
    /// Loads a config from a given file
    pub fn load_file<P: AsRef<Path>>(path: P) -> Result<Config> {
        let f = fs::File::open(path)?;
        serde_yaml::from_reader(BufReader::new(f)).map_err(|err| {
            ErrorKind::ConfigError(err).into()
        })
    }

    /// Loads a config from the default location
    pub fn load_default() -> Result<Config> {
        let mut home = match env::home_dir() {
            Some(home) => home,
            None => { return Ok(Default::default()) },
        };
        home.push(".sentry-symbolserver.yml");

        Ok(if let Ok(_) = fs::metadata(&home) {
            Config::load_file(&home)?
        } else {
            Default::default()
        })
    }

    /// Return the AWS access key
    pub fn get_aws_access_key<'a>(&'a self) -> Option<&str> {
        self.aws.access_key.as_ref().map(|x| &**x)
    }

    /// Return the AWS secret key
    pub fn get_aws_secret_key<'a>(&'a self) -> Option<&str> {
        self.aws.secret_key.as_ref().map(|x| &**x)
    }

    /// Return the AWS S3 bucket URL
    pub fn get_aws_bucket_url<'a>(&'a self) -> Result<Url> {
        let url = if let Some(ref value) = self.aws.bucket_url {
            Url::parse(value)?
        } else if let Ok(value) = env::var("SYMBOLSERVER_BUCKET_URL") {
            Url::parse(&value)?
        } else {
            return Err(ErrorKind::MissingConfigKey(
                "aws.bucket_url").into());
        };
        if url.scheme() != "s3" {
            return Err(ErrorKind::BadConfigKey(
                "aws.bucket_url", "The scheme for the bucket URL needs to be s3").into());
        } else if url.host_str().is_none() {
            return Err(ErrorKind::BadConfigKey(
                "aws.bucket_url", "The bucket URL is missing a name").into());
        }
        Ok(url)
    }

    /// Overrides the AWS bucket URL.
    pub fn set_aws_bucket_url(&mut self, value: &str) {
        self.aws.bucket_url = Some(value.to_string());
    }

    /// Return the AWS region
    pub fn get_aws_region(&self) -> Result<Region> {
        let region_opt = self.aws.region
            .as_ref()
            .map(|x| x.to_string())
            .or_else(|| env::var("AWS_DEFAULT_REGION").ok());

        if let Some(region) = region_opt {
            if let Ok(rv) = region.parse() {
                Ok(rv)
            } else {
                Err(ErrorKind::BadConfigKey(
                    "aws.region", "An unknown AWS region was provided").into())
            }
        } else {
            Ok(Region::UsEast1)
        }
    }

    /// Overrides the AWS region
    pub fn set_aws_region(&mut self, value: Region) {
        self.aws.region = Some(value.to_string());
    }

    /// Return the path where symbols are stored.
    pub fn get_symbol_dir<'a>(&'a self) -> Result<Cow<'a, Path>> {
        if let Some(ref path) = self.symbol_dir {
            Ok(Cow::Borrowed(path.as_path()))
        } else if let Ok(dir) = env::var("SYMBOLSERVER_SYMBOL_DIR") {
            Ok(Cow::Owned(PathBuf::from(dir)))
        } else {
            Err(ErrorKind::MissingConfigKey("symbol_dir").into())
        }
    }

    /// Override the symbol dir.
    pub fn set_symbol_dir<P: AsRef<Path>>(&mut self, value: P) {
        self.symbol_dir = Some(value.as_ref().to_path_buf());
    }

    fn get_server_host(&self) -> Result<String> {
        if let Some(ref host) = self.server.host {
            Ok(host.clone())
        } else if let Ok(var) = env::var("IP") {
            Ok(var)
        } else if is_docker() {
            Ok("0.0.0.0".into())
        } else {
            Ok("127.0.0.1".into())
        }
    }

    fn get_server_port(&self) -> Result<u16> {
        if let Some(port) = self.server.port {
            Ok(port)
        } else if let Ok(portstr) = env::var("PORT") {
            Ok(portstr.parse().chain_err(|| "Invalid value for port")?)
        } else {
            Ok(3000)
        }
    }

    /// Return the bind target for the http server
    pub fn get_server_socket_addr(&self) -> Result<(String, u16)> {
        Ok((self.get_server_host()?, self.get_server_port()?))
    }

    /// Return the server healthcheck ttl
    pub fn get_server_healthcheck_ttl(&self) -> Result<Duration> {
        let ttl = if let Some(ttl) = self.server.healthcheck_ttl {
            ttl
        } else if let Ok(ttlstr) = env::var("SYMBOLSERVER_HEALTHCHECK_TTL") {
            ttlstr.parse().chain_err(|| "Invalid value for healthcheck ttl")?
        } else {
            return Ok(Duration::minutes(2));
        };
        if ttl < 0 {
            return Err(ErrorKind::BadConfigKey(
                "server.healthcheck_ttl", "Healthcheck TTL has to be positive").into());
        }
        Ok(Duration::seconds(ttl))
    }

    /// Return the server sync interval
    pub fn get_server_sync_interval(&self) -> Result<Duration> {
        let interval = if let Some(interval) = self.sync.interval {
            interval
        } else if let Ok(intervalstr) = env::var("SYMBOLSERVER_SYNC_INTERVAL") {
            intervalstr.parse().chain_err(|| "Invalid value for sync interval")?
        } else {
            return Ok(Duration::minutes(1));
        };
        if interval < 0 {
            return Err(ErrorKind::BadConfigKey(
                "sync.interval", "Sync interval has to be positive").into());
        }
        Ok(Duration::seconds(interval))
    }

    /// Return the number of threads to listen on
    pub fn get_server_threads(&self) -> Result<usize> {
        if let Some(threads) = self.server.threads {
            Ok(threads)
        } else if let Ok(threadstr) = env::var("SYMBOLSERVER_THREADS") {
            Ok(threadstr.parse().chain_err(|| "Invalid value for thread count")?)
        } else {
            Ok(num_cpus::get() * 5 / 4)
        }
    }

    /// Return the log level filter
    pub fn get_log_level_filter(&self) -> Result<LogLevelFilter> {
        let level_opt = self.log.level
            .as_ref()
            .map(|x| x.to_string())
            .or_else(|| env::var("SYMBOLSERVER_LOG_LEVEL").ok());

        if let Some(lvl) = level_opt {
            lvl.parse().map_err(|_| ErrorKind::BadConfigKey(
                "log.level", "unknown log level").into())
        } else {
            Ok(LogLevelFilter::Info)
        }
    }

    /// Override the log level filter in the config
    pub fn set_log_level_filter(&mut self, value: LogLevelFilter) {
        self.log.level = Some(value.to_string());
    }

    /// Return the log filename
    pub fn get_log_filename<'a>(&'a self) -> Result<Option<Cow<'a, Path>>> {
        if let Some(ref path) = self.log.file {
            Ok(Some(Cow::Borrowed(&*path)))
        } else if let Ok(path) = env::var("SYMBOLSERVER_LOG_FILE") {
            Ok(Some(Cow::Owned(PathBuf::from(path))))
        } else {
            Ok(None)
        }
    }

    /// Return the sync include regex filter
    pub fn get_sync_include_filter(&self) -> Result<&RegexFilter> {
        Ok(&self.sync.include)
    }

    /// Return the sync exclude regex filter
    pub fn get_sync_exclude_filter(&self) -> Result<&RegexFilter> {
        Ok(&self.sync.exclude)
    }
}
