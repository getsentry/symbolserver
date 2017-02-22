use std::env;
use std::path::Path;
use std::fs;
use std::path::PathBuf;

use serde_yaml;

use super::{Result, ErrorKind};


#[derive(Deserialize, Debug)]
pub struct AwsConfig {
    access_key: String,
    secret_key: String,
    bucket_url: String,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    aws: Option<AwsConfig>,
    symbol_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            aws: None,
            symbol_dir: None,
        }
    }
}

impl Config {
    pub fn load_file<P: AsRef<Path>>(path: P) -> Result<Config> {
        let mut f = fs::File::open(path)?;
        serde_yaml::from_reader(&mut f).map_err(|err| {
            ErrorKind::ConfigError(err).into()
        })
    }

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
}
