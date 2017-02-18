//! Provides SDK Information
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use zip;
use walkdir;
use regex::Regex;
use mach_object::Error as MachError;

use super::{Result, Error, ErrorKind};
use super::dsym::Object;


enum ObjectIterSource {
    Zip {
        archive: zip::ZipArchive<fs::File>,
        idx: usize,
    },
    Dir {
        dir_iter: walkdir::Iter,
    }
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

pub struct ObjectsIter {
    source: ObjectIterSource,
}

pub struct SdkProcessor {
    path: PathBuf,
    info: SdkInfo,
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

impl ObjectIterSource {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<ObjectIterSource> {
        let md = fs::metadata(path.as_ref())?;
        if md.is_file() {
            let f = fs::File::open(path.as_ref())?;
            let zip = zip::ZipArchive::new(f)?;
            Ok(ObjectIterSource::Zip {
                archive: zip,
                idx: 0,
            })
        } else {
            Ok(ObjectIterSource::Dir {
                dir_iter: walkdir::WalkDir::new(path.as_ref()).into_iter(),
            })
        }
    }
}

impl<'a> Iterator for ObjectsIter {
    type Item = Result<Object<'static>>;

    fn next(&mut self) -> Option<Result<Object<'static>>> {
        macro_rules! try_return_obj {
            ($expr:expr) => {
                match $expr {
                    Ok(rv) => {
                        return Some(Ok(rv));
                    }
                    Err(err) => {
                        if let &ErrorKind::MachO(ref mach_err) = err.kind() {
                            if let &MachError::LoadError(_) = mach_err {
                                continue;
                            }
                        }
                        return Some(Err(err.into()));
                    }
                }
            }
        }

        loop {
            match self.source {
                ObjectIterSource::Zip { ref mut archive, ref mut idx } => {
                    if *idx >= archive.len() {
                        break;
                    }
                    let mut f = iter_try!(archive.by_index(*idx));
                    *idx += 1;
                    let mut buf : Vec<u8> = vec![];
                    iter_try!(f.read_to_end(&mut buf));
                    try_return_obj!(Object::from_vec(buf));
                }
                ObjectIterSource::Dir { ref mut dir_iter } => {
                    if let Some(dent_res) = dir_iter.next() {
                        let dent = iter_try!(dent_res);
                        let md = iter_try!(dent.metadata());
                        if md.is_file() && md.len() > 0 {
                            try_return_obj!(Object::from_path(dent.path()));
                        }
                    } else {
                        break;
                    }
                }
            }
        }
        None
    }
}

impl SdkProcessor {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<SdkProcessor> {
        let p = path.as_ref().to_path_buf();
        let sdk_info = SdkInfo::from_path(&p).ok_or_else(|| {
            Error::from(ErrorKind::UnknownSdk)
        })?;
        Ok(SdkProcessor {
            path: p,
            info: sdk_info,
        })
    }

    pub fn info(&self) -> &SdkInfo {
        &self.info
    }

    pub fn objects<'a>(&'a self) -> Result<ObjectsIter> {
        Ok(ObjectsIter {
            source: ObjectIterSource::from_path(&self.path)?,
        })
    }
}
