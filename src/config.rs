//! Provides access to the symbolserver config
use std::env;
use std::path::Path;
use std::fs;
use std::path::PathBuf;

use serde_yaml;
use url::Url;
use rusoto::Region;

use super::{Result, ErrorKind};


#[derive(Deserialize, Debug, Default)]
struct AwsConfig {
    access_key: Option<String>,
    secret_key: Option<String>,
    bucket_url: Option<String>,
    region: Option<String>,
}

/// Central config object that exposes the information from
/// the symbolserver yaml config.
#[derive(Deserialize, Debug, Default)]
pub struct Config {
    #[serde(default)]
    aws: AwsConfig,
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
                "aws.bucket_url", None).into());
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
        match self.aws.region {
            Some(ref region) => {
                if let Ok(rv) = region.parse() {
                    Ok(rv)
                } else {
                    Err(ErrorKind::BadConfigKey(
                        "aws.region", "An unknown AWS region was provided").into())
                }
            }
            None => Ok(Region::UsEast1)
        }
    }

    /// Return the path where symbols are stored.
    pub fn get_symbol_dir(&self) -> Result<&Path> {
        if let Some(ref path) = self.symbol_dir {
            Ok(path.as_path())
        } else {
            Err(ErrorKind::MissingConfigKey("symbol_dir", None).into())
        }
    }
}
