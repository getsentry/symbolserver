//! Provides SDK Information
use std::fs;
use std::io::{Read, Write, Seek};
use std::path::{Path, PathBuf};

use zip;
use walkdir;
use regex::Regex;
use mach_object::Error as MachError;

use super::{Result, Error, ErrorKind};
use super::dsym::Object;
use super::memdbdump::MemDbBuilder;


enum ObjectIterSource {
    Zip {
        archive: zip::ZipArchive<fs::File>,
        idx: usize,
    },
    Dir {
        base: PathBuf,
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
#[derive(Debug, Clone, PartialEq)]
pub struct SdkInfo {
    name: String,
    version_major: u32,
    version_minor: u32,
    version_patchlevel: u32,
    build: String,
}

/// Iterates over all objects in an SDK
pub struct ObjectsIter {
    source: ObjectIterSource,
}

/// Helper struct to process an SDK from the FS or a ZIP
pub struct Sdk {
    path: PathBuf,
    info: SdkInfo,
}

impl SdkInfo {

    /// Manually construct an SDK info
    pub fn new(name: &str, version_major: u32, version_minor: u32,
               version_patchlevel: u32, build: &str)
        -> SdkInfo
    {
        SdkInfo {
            name: name.to_string(),
            version_major: version_major,
            version_minor: version_minor,
            version_patchlevel: version_patchlevel,
            build: build.to_string(),
        }
    }

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
            static ref MEMDB_FILENAME_RE: Regex = Regex::new(r"(?x)
                ^
                    ([^-]+)_
                    (\d+)\.(\d+)(?:\.(\d+))?_
                    ([a-zA-Z0-9]+)
                    (?:\.memdb)?
                $
            ").unwrap();
        }

        let p = path.as_ref();
        let filename = try_opt!(p.file_name().and_then(|x| x.to_str()));
        if let Some(caps) = MEMDB_FILENAME_RE.captures(filename) {
            return Some(SdkInfo::new(
                caps.get(1).unwrap().as_str(),
                try_opt!(caps.get(2).unwrap().as_str().parse().ok()),
                try_opt!(caps.get(3).unwrap().as_str().parse().ok()),
                try_opt!(caps.get(4).map(|x| x.as_str()).unwrap_or("0").parse().ok()),
                try_opt!(caps.get(5).map(|x| x.as_str())),
            ));
        }

        let folder = try_opt!(p.parent().and_then(|x| x.file_name()).and_then(|x| x.to_str()));
        let caps = try_opt!(SDK_FILENAME_RE.captures(filename));
        Some(SdkInfo::new(
            try_opt!(get_sdk_name_from_folder(folder)),
            try_opt!(caps.get(1).unwrap().as_str().parse().ok()),
            try_opt!(caps.get(2).unwrap().as_str().parse().ok()),
            try_opt!(caps.get(3).map(|x| x.as_str()).unwrap_or("0").parse().ok()),
            try_opt!(caps.get(4).map(|x| x.as_str())),
        ))
    }

    /// The SDK name (iOS, tvOS etc.)
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The major version of the SDK.
    pub fn version_major(&self) -> u32 {
        self.version_major
    }

    /// The minor version of the SDK
    pub fn version_minor(&self) -> u32 {
        self.version_minor
    }

    /// The patchlevel version of the SDK
    pub fn version_patchlevel(&self) -> u32 {
        self.version_patchlevel
    }

    /// The build of the SDK
    pub fn build(&self) -> &str {
        &self.build
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
                base: path.as_ref().join("Symbols"),
                dir_iter: walkdir::WalkDir::new(path.as_ref()).into_iter(),
            })
        }
    }
}

fn strip_archive_file_prefix(path: &str) -> &str {
    let mut iter = path.splitn(2, '/');

    // Symbols/foo/bar -> foo/bar
    if let Some("Symbols") = iter.next() {
        if let Some(rest) = iter.next() {
            return rest;
        }
    }

    // Foo/Symbols/foo/bar -> foo/bar
    let mut iter = path.splitn(3, '/');
    if let Some(_) = iter.next() {
        if let Some("Symbols") = iter.next() {
            if let Some(rest) = iter.next() {
                return rest;
            }
        }
    }

    path
}

impl<'a> Iterator for ObjectsIter {
    type Item = Result<(String, Object<'static>)>;

    fn next(&mut self) -> Option<Result<(String, Object<'static>)>> {
        macro_rules! try_return_obj {
            ($expr:expr, $name:expr) => {
                match $expr {
                    Ok(rv) => {
                        return Some(Ok(($name.to_string(), rv)));
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
                    if iter_try!(f.read_to_end(&mut buf)) > 0 {
                        try_return_obj!(Object::from_vec(buf),
                            strip_archive_file_prefix(f.name()));
                    }
                }
                ObjectIterSource::Dir { ref base, ref mut dir_iter } => {
                    if let Some(dent_res) = dir_iter.next() {
                        let dent = iter_try!(dent_res);
                        let md = iter_try!(dent.metadata());
                        if md.is_file() && md.len() > 0 {
                            let rp = dent.path().strip_prefix(base).unwrap_or(dent.path());
                            try_return_obj!(
                                Object::from_path(dent.path()),
                                rp.display());
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

impl Sdk {
    /// Constructs a processor from a file system path
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Sdk> {
        let p = path.as_ref().to_path_buf();
        let sdk_info = SdkInfo::from_path(&p).ok_or_else(|| {
            Error::from(ErrorKind::UnknownSdk)
        })?;
        Ok(Sdk {
            path: p,
            info: sdk_info,
        })
    }

    /// Returns the SDK info (derived from the path)
    pub fn info(&self) -> &SdkInfo {
        &self.info
    }

    /// Returns an object iterator
    pub fn objects<'a>(&'a self) -> Result<ObjectsIter> {
        Ok(ObjectsIter {
            source: ObjectIterSource::from_path(&self.path)?,
        })
    }

    /// Writes a memdb file for the SDK
    ///
    /// This can then be later read with the `MemDb` type.
    pub fn dump_memdb<W: Write + Seek>(&self, writer: W) -> Result<()> {
        let mut builder = MemDbBuilder::new(writer, self.info())?;
        for obj_res in self.objects()? {
            let (filename, obj) = obj_res?;
            builder.write_object(&obj, Some(&filename))?;
        }
        builder.flush()?;
        Ok(())
    }
}
