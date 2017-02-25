//! Provide access to locally cached memdb SDKs
//!
//! The `MemDbStash` pulls in remote SDKs from an S3 bucket and provides
//! access to it.  This is used by the symbol server to manage the local
//! cache and also to refer to memdb files that are mmap'ed in.
use std::fs;
use std::io;
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Values as HashMapValuesIter;

use serde_json;
use xz2::write::XzDecoder;

use super::config::Config;
use super::sdk::SdkInfo;
use super::s3::S3;
use super::{Result, ResultExt};
use super::utils::{ProgressIndicator, copy_with_progress};

/// The main memdb stash type
pub struct MemDbStash<'a> {
    path: &'a Path,
    s3: S3<'a>,
}

/// Information about a remotely available SDK
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RemoteSdk {
    filename: String,
    info: SdkInfo,
    size: u64,
    etag: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct SdkSyncState {
    sdks: HashMap<String, RemoteSdk>,
}

/// Information about the health of the stash sync
#[derive(Debug)]
pub struct SyncStatus {
    remote_total: u32,
    missing: u32,
    different: u32,
}

impl RemoteSdk {
    /// Creates a remote SDK object from some information
    pub fn new(filename: String, info: SdkInfo, etag: String, size: u64) -> RemoteSdk {
        RemoteSdk {
            filename: filename,
            info: info,
            etag: etag,
            size: size,
        }
    }

    /// The remotely visible filename for the SDK
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// The local filename the SDK has in the stash folder
    pub fn local_filename(&self) -> &str {
        self.filename.trim_right_matches('z')
    }

    /// The size of the SDK in bytes
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns the SDK info
    pub fn info(&self) -> &SdkInfo {
        &self.info
    }
}

/// Iterator over the SDKs
pub type RemoteSdkIter<'a> = HashMapValuesIter<'a, String, RemoteSdk>;

impl SdkSyncState {
    pub fn get_sdk(&self, filename: &str) -> Option<&RemoteSdk> {
        self.sdks.get(filename)
    }

    pub fn update_sdk(&mut self, sdk: &RemoteSdk) {
        self.sdks.insert(sdk.local_filename().to_string(), sdk.clone());
    }

    pub fn sdks<'a>(&'a self) -> RemoteSdkIter<'a> {
        self.sdks.values()
    }
}

impl SyncStatus {

    /// Returns the lag (number of SDKs behind upstream)
    pub fn lag(&self) -> u32 {
        self.missing + self.different
    }

    /// Returns true if the local sync is still considered healthy
    pub fn is_healthy(&self) -> bool {
        let total = self.remote_total as f32;
        let lag = self.lag() as f32;
        lag / total < 0.10
    }
}

impl<'a> MemDbStash<'a> {
    /// Opens a stash for a given config.
    pub fn new(config: &'a Config) -> Result<MemDbStash<'a>> {
        Ok(MemDbStash {
            path: config.get_symbol_dir()?,
            s3: S3::from_config(config)?,
        })
    }

    fn get_local_sync_state_filename(&self) -> PathBuf {
        self.path.join("sync.state")
    }

    fn load_state(&self, filename: &Path) -> Result<Option<SdkSyncState>> {
        match fs::File::open(filename) {
            Ok(f) => Ok(Some(serde_json::from_reader(f)
                .chain_err(|| "Parsing error on loading sync state")?)),
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(err).chain_err(|| "Error loading sync state")
                }
            }
        }
    }

    fn save_state(&self, new_state: &SdkSyncState, filename: &Path) -> Result<()> {
        let mut tmp_filename = filename.to_path_buf();
        tmp_filename.set_extension("tempstate");
        {
            let mut f = fs::File::create(&tmp_filename)?;
            serde_json::to_writer(&mut f, new_state)
                .chain_err(|| "Could not update sync state")?;
        }
        fs::rename(&tmp_filename, &filename)?;
        Ok(())
    }

    fn get_local_state(&self) -> Result<SdkSyncState> {
        match self.load_state(&self.get_local_sync_state_filename())? {
            Some(state) => Ok(state),
            None => Ok(Default::default()),
        }
    }

    fn get_remote_state(&self) -> Result<SdkSyncState> {
        let mut sdks = HashMap::new();
        for remote_sdk in self.s3.list_upstream_sdks()? {
            sdks.insert(remote_sdk.local_filename().into(), remote_sdk);
        }
        Ok(SdkSyncState { sdks: sdks })
    }

    fn save_local_state(&self, new_state: &SdkSyncState) -> Result<()> {
        self.save_state(new_state, &self.get_local_sync_state_filename())
    }

    fn update_sdk(&self, sdk: &RemoteSdk) -> Result<()> {
        // XXX: the progress bar here can stall out because we currently
        // need to buffer the download into memory in the s3 code :(
        let progress = ProgressIndicator::new(sdk.size() as usize);
        progress.set_message(&format!("Synchronizing {}", sdk.info()));
        let mut src = self.s3.download_sdk(sdk)?;
        let dst = fs::File::create(self.path.join(sdk.local_filename()))?;
        let mut dst = XzDecoder::new(dst);
        copy_with_progress(&progress, &mut src, &mut dst)?;
        progress.finish(&format!("Synchronized {}", sdk.info()));
        Ok(())
    }

    fn remove_sdk(&self, sdk: &RemoteSdk) -> Result<()> {
        println!("Deleting {}", sdk.info());
        if let Err(err) = fs::remove_file(self.path.join(sdk.local_filename())) {
            if err.kind() != io::ErrorKind::NotFound {
                return Err(err.into());
            }
        }
        Ok(())
    }

    /// Checks the local stash against the server
    pub fn get_sync_status(&self) -> Result<SyncStatus> {
        let local_state = self.get_local_state()?;
        let remote_state = self.get_remote_state()?;

        let mut remote_total = 0;
        let mut missing = 0;
        let mut different = 0;

        for sdk in remote_state.sdks() {
            if let Some(local_sdk) = local_state.get_sdk(sdk.local_filename()) {
                if local_sdk != sdk {
                    different += 1;
                }
            } else {
                missing += 1;
            }
            remote_total += 1;
        }

        Ok(SyncStatus {
            remote_total: remote_total as u32,
            missing: missing as u32,
            different: different as u32,
        })
    }

    /// Synchronize the local stash with the server
    pub fn sync(&self) -> Result<()> {
        let mut local_state = self.get_local_state()?;
        let remote_state = self.get_remote_state()?;

        let mut to_delete : HashSet<_> = HashSet::from_iter(
            local_state.sdks().map(|x| x.local_filename().to_string()));

        for sdk in remote_state.sdks() {
            if let Some(local_sdk) = local_state.get_sdk(sdk.local_filename()) {
                if local_sdk != sdk {
                    self.update_sdk(&sdk)?;
                } else {
                    println!("  ⸰ Unchanged {}", sdk.info());
                }
            } else {
                self.update_sdk(&sdk)?;
            }
            to_delete.remove(sdk.local_filename());
            local_state.update_sdk(&sdk);
            self.save_local_state(&local_state)?;
        }

        for local_filename in to_delete.iter() {
            if let Some(sdk) = local_state.get_sdk(local_filename) {
                self.remove_sdk(sdk)?;
            }
        }

        println!("Done synching");

        // if we get this far, the remote state is indeed the local state.
        self.save_local_state(&remote_state)?;

        Ok(())
    }
}
