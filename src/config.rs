use std::env;
use std::path::Path;
use std::fs;
use std::path::PathBuf;
use std::borrow::Cow;

use serde_yaml;

use super::{Result, ErrorKind};


#[derive(Deserialize, Debug, Default)]
pub struct AwsConfig {
    access_key: Option<String>,
    secret_key: Option<String>,
    bucket_url: Option<String>,
}

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
    pub fn get_aws_access_key<'a>(&'a self) -> Result<Cow<'a, str>> {
        if let Ok(value) = env::var("AWS_ACCESS_KEY") { 
            Ok(Cow::Owned(value))
        } else if let Some(key) = self.aws.access_key.as_ref() {
            Ok(Cow::Borrowed(key))
        } else {
            Err(ErrorKind::MissingConfigKey("aws.access_key", Some("AWS_ACCESS_KEY")).into())
        }
    }

    /// Return the AWS secret key
    pub fn get_aws_secret_key<'a>(&'a self) -> Result<Cow<'a, str>> {
        if let Ok(value) = env::var("AWS_SECRET_KEY") { 
            Ok(Cow::Owned(value))
        } else if let Some(key) = self.aws.secret_key.as_ref() {
            Ok(Cow::Borrowed(key))
        } else {
            Err(ErrorKind::MissingConfigKey("aws.secret_key", Some("AWS_SECRET_KEY")).into())
        }
    }

    /// Return the AWS S3 bucket URL
    pub fn get_aws_bucket_url<'a>(&'a self) -> Result<Cow<'a, str>> {
        if let Ok(value) = env::var("SYMBOLSERVER_BUCKET_URL") { 
            Ok(Cow::Owned(value))
        } else if let Some(url) = self.aws.bucket_url.as_ref() {
            Ok(Cow::Borrowed(url))
        } else {
            Err(ErrorKind::MissingConfigKey("aws.bucket_url", Some("SYMBOLSERVER_BUCKET_URL")).into())
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
