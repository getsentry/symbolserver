//! Provides access to the symbolserver config
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_yaml;
use url::Url;
use rusoto::Region;
use chrono::Duration;
use log::LogLevelFilter;

use super::{Result, ErrorKind};


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
    healthcheck_ttl: Option<u32>,
    sync_interval: Option<u32>,
}

#[derive(Deserialize, Debug, Default, Clone)]
struct LogConfig {
    level: Option<String>,
    file: Option<PathBuf>,
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
}

impl Config {
    /// Loads a config from a given file
    pub fn load_file<P: AsRef<Path>>(path: P) -> Result<Config> {
        let mut f = fs::File::open(path)?;
        serde_yaml::from_reader(&mut f).map_err(|err| {
            ErrorKind::ConfigError(err).into()
        })
    }

    /// Loads a config from the default location
    pub fn load_default() -> Result<Config> {
        let mut home = match env::home_dir() {
            Some(home) => home,
            None => { return Ok(Default::default()) },
        };
        home.push(".symbolserver.yml");

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
        let url = if let Some(value) = self.aws.bucket_url.as_ref() {
            Url::parse(value)?
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

    /// Return the path where symbols are stored.
    pub fn get_symbol_dir(&self) -> Result<&Path> {
        if let Some(ref path) = self.symbol_dir {
            Ok(path.as_path())
        } else {
            Err(ErrorKind::MissingConfigKey("symbol_dir").into())
        }
    }

    /// Override the symbol dir.
    pub fn set_symbol_dir<P: AsRef<Path>>(&mut self, value: P) {
        self.symbol_dir = Some(value.as_ref().to_path_buf());
    }

    /// Return the bind target for the http server
    pub fn get_server_socket_addr(&self) -> Result<(&str, u16)> {
        let host = self.server.host.as_ref().map(|x| x.as_str()).unwrap_or("127.0.0.1");
        let port = self.server.port.unwrap_or(3000);
        Ok((host, port))
    }

    /// Return the server healthcheck ttl
    pub fn get_server_healthcheck_ttl(&self) -> Result<Duration> {
        let ttl = self.server.healthcheck_ttl.unwrap_or(60);
        Ok(Duration::seconds(ttl as i64))
    }

    /// Return the server sync interval
    pub fn get_server_sync_interval(&self) -> Result<Duration> {
        let ttl = self.server.sync_interval.unwrap_or(60);
        if ttl <= 0 {
            Err(ErrorKind::BadConfigKey(
                "server.sync_interval", "Sync interval has to be positive").into())
        } else {
            Ok(Duration::seconds(ttl as i64))
        }
    }

    /// Return the log level filter
    pub fn get_log_level_filter(&self) -> Result<LogLevelFilter> {
        if let Some(ref lvl) = self.log.level {
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
    pub fn get_log_filename(&self) -> Result<Option<&Path>> {
        if let Some(ref path) = self.log.file {
            Ok(Some(&*path))
        } else {
            Ok(None)
        }
    }
}
