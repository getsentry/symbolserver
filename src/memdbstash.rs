use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use serde_json;

use super::config::Config;
use super::sdk::SdkInfo;
use super::s3::S3;
use super::{Result, ResultExt};

pub struct MemDbStash<'a> {
    config: &'a Config,
    path: &'a Path,
    s3: S3<'a>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteSdk {
    filename: String,
    info: SdkInfo,
    size: u64,
    etag: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct SyncState {
    files: HashMap<String, RemoteSdk>,
}

impl RemoteSdk {
    pub fn new(filename: String, info: SdkInfo, etag: String, size: u64) -> RemoteSdk {
        RemoteSdk {
            filename: filename,
            info: info,
            etag: etag,
            size: size,
        }
    }

    pub fn local_filename(&self) -> &str {
        self.filename.trim_right_matches('z')
    }
}

impl<'a> MemDbStash<'a> {
    pub fn new(config: &'a Config) -> Result<MemDbStash<'a>> {
        Ok(MemDbStash {
            config: config,
            path: config.get_symbol_dir()?,
            s3: S3::from_config(config)?,
        })
    }

    fn get_sync_state_filename(&self) -> PathBuf {
        self.path.join("sync.state")
    }

    fn get_local_state(&self) -> Result<SyncState> {
        let filename = self.get_sync_state_filename();
        match fs::File::open(filename) {
            Ok(f) => Ok(serde_json::from_reader(f)
                .chain_err(|| "Parsing error on loading sync state")?),
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    Ok(Default::default())
                } else {
                    Err(err).chain_err(|| "Error loading sync state")
                }
            }
        }
    }

    fn get_remote_state(&self) -> Result<SyncState> {
        let mut files = HashMap::new();
        for remote_sdk in self.s3.list_upstream_sdks()? {
            files.insert(remote_sdk.local_filename().into(), remote_sdk);
        }
        Ok(SyncState { files: files })
    }

    pub fn sync(&self) -> Result<()> {
        let local_state = self.get_local_state()?;
        let remote_state = self.get_remote_state()?;

        println!("LOCAL:  {:?}", local_state);
        println!("REMOTE: {:?}", remote_state);

        Ok(())
    }
}
