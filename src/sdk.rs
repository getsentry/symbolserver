//! Provides SDK Information
use std::fs;
use std::path::Path;

use zip;
use walkdir;
use regex::Regex;

pub enum SdkSource {
    Zip(zip::ZipArchive<fs::File>),
    Dir(walkdir::Iter),
}

fn get_sdk_name_from_folder(folder: &str) -> Option<&'static str> {
    match folder {
        "iOS DeviceSupport" => Some("iOS"),
        "tvOS DeviceSupport" => Some("tvOS"),
        _ => None,
    }
}

/// Information of the SDK
#[derive(Debug)]
pub struct SdkInfo {
    /// The name of the SDK (iOS, tvOS etc.)
    pub name: &'static str,
    /// The major version identifier
    pub version_major: u32,
    /// The minor version identifier
    pub version_minor: u32,
    /// The patchlevel version identifier (might be 0)
    pub version_patchlevel: u32,
    /// build number.
    pub build: String,
    /// The SDK flavour (this is currently only used for watchOS)
    /// where this can be `Watch2,2` for instance.
    pub flavour: Option<String>,
}

impl SdkInfo {

    /// Load an SDK info from a given path
    ///
    /// If the parse cannot be parsed for an SDK info `None` is returned.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Option<SdkInfo> {
        lazy_static! {
            static ref SDK_FILENAME_RE: Regex = Regex::new(r"(?x)
                ^
                    (\d+)\.(\d+)(?:\.(\d+))?
                    \s+
                    \(([a-zA-Z0-9]+)\)
                    (?:\.zip)?
                $
            ").unwrap();
        }

        let p = path.as_ref();
        let folder = try_opt!(p.parent().and_then(|x| x.file_name()).and_then(|x| x.to_str()));
        let filename = try_opt!(p.file_name().and_then(|x| x.to_str()));
        let caps = try_opt!(SDK_FILENAME_RE.captures(filename));
        Some(SdkInfo {
            name: try_opt!(get_sdk_name_from_folder(folder)),
            version_major: try_opt!(caps.get(1).unwrap().as_str().parse().ok()),
            version_minor: try_opt!(caps.get(2).unwrap().as_str().parse().ok()),
            version_patchlevel: try_opt!(caps.get(3).map(|x| x.as_str()).unwrap_or("0").parse().ok()),
            build: try_opt!(caps.get(4).map(|x| x.as_str().to_string())),
            flavour: None,
        })
    }
}
