use std::fs;
use std::io;
use std::iter;
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Values as HashMapValuesIter;

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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RemoteSdk {
    filename: String,
    info: SdkInfo,
    size: u64,
    etag: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct SyncState {
    sdks: HashMap<String, RemoteSdk>,
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

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn local_filename(&self) -> &str {
        self.filename.trim_right_matches('z')
    }

    pub fn info(&self) -> &SdkInfo {
        &self.info
    }
}

pub type SdksIter<'a> = HashMapValuesIter<'a, String, RemoteSdk>;

impl SyncState {
    pub fn get_sdk(&self, filename: &str) -> Option<&RemoteSdk> {
        self.sdks.get(filename)
    }

    pub fn sdks<'a>(&'a self) -> SdksIter<'a> {
        self.sdks.values()
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
        let mut sdks = HashMap::new();
        for remote_sdk in self.s3.list_upstream_sdks()? {
            sdks.insert(remote_sdk.local_filename().into(), remote_sdk);
        }
        Ok(SyncState { sdks: sdks })
    }

    fn update_sdk(&self, sdk: &RemoteSdk) -> Result<()> {
        println!("Synching {}", sdk.info());
        let mut src = self.s3.download_sdk(sdk)?;
        let mut dst = fs::File::create(self.path.join(sdk.local_filename()))?;
        io::copy(&mut src, &mut dst);
        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        let local_state = self.get_local_state()?;
        let remote_state = self.get_remote_state()?;
        let mut to_delete : HashSet<&str> = HashSet::from_iter(
            local_state.sdks().map(|x| x.local_filename()));

        for sdk in remote_state.sdks() {
            if let Some(local_sdk) = local_state.get_sdk(sdk.local_filename()) {
                if local_sdk != sdk {
                    self.update_sdk(&sdk)?;
                }
            } else {
                self.update_sdk(&sdk)?;
            }
            to_delete.remove(sdk.local_filename());
        }

        Ok(())
    }
}
