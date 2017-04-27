//! Provides various useful utilities.
use std::io;
use std::fs;
use std::fmt;
use std::env;
use std::panic;
use std::os::unix::io::RawFd;
use std::result::Result as StdResult;
use std::io::{Read, Write, Seek, SeekFrom};
use std::cmp::Ordering;

use globset;
use indicatif::ProgressBar;
use chrono::Duration;
use serde::{Serialize, Deserialize, de, ser};

use super::{Result, ResultExt, Error, ErrorKind};

pub const SD_LISTEN_FDS_START: RawFd = 3;

/// Helper for serializing/deserializing addresses in string format
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Addr(pub u64);

/// Reverse sort helper
#[derive(PartialEq, Eq)]
pub struct Rev<T: Ord+PartialOrd+Eq+PartialEq>(pub T);

impl<T: Ord+PartialOrd+Eq+PartialEq> PartialOrd for Rev<T> {
    fn partial_cmp(&self, other: &Rev<T>) -> Option<Ordering> {
        other.0.partial_cmp(&self.0)
    }
}

impl<T: Ord+PartialOrd+Eq+PartialEq> Ord for Rev<T> {
    fn cmp(&self, other: &Rev<T>) -> Ordering {
        other.0.cmp(&self.0)
    }
}

/// Helper for formatting durations.
pub struct HumanDuration(pub Duration);

impl Into<u64> for Addr {
    fn into(self) -> u64 {
        self.0
    }
}

impl<'a> fmt::Display for HumanDuration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        macro_rules! try_write {
            ($num:expr, $str:expr) => {
                if $num == 1 { return write!(f, "1 {}", $str); }
                else if $num > 1 { return write!(f, "{} {}s", $num, $str); }
            }
        }

        try_write!(self.0.num_hours(), "hour");
        try_write!(self.0.num_minutes(), "minute");
        try_write!(self.0.num_seconds(), "second");
        write!(f, "0 seconds")
    }
}

impl Serialize for Addr {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
        where S: ser::Serializer
    {
        serializer.serialize_str(&format!("0x{:x}", self.0))
    }
}

impl Deserialize for Addr {
    fn deserialize<D>(deserializer: D) -> StdResult<Addr, D::Error>
        where D: de::Deserializer {
        struct AddrVisitor;

        impl de::Visitor for AddrVisitor {
            type Value = u64;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an address")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> StdResult<u64, E> {
                if &value[..2] == "0x" {
                    u64::from_str_radix(&value[2..], 16)
                        .map_err(|e| E::custom(e.to_string()))
                } else {
                    u64::from_str_radix(&value, 10)
                        .map_err(|e| E::custom(e.to_string()))
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> StdResult<u64, E> {
                Ok(value)
            }
        }

        deserializer.deserialize_str(AddrVisitor).map(Addr)
    }
}

#[derive(Clone, Debug, Default)]
pub struct IgnorePatterns {
    patterns: Vec<(bool, globset::GlobMatcher)>,
}

impl Deserialize for IgnorePatterns {
    fn deserialize<D>(deserializer: D) -> StdResult<IgnorePatterns, D::Error>
        where D: de::Deserializer {
        struct FilterVisitor;

        fn make_pattern<E: de::Error>(value: &str)
            -> StdResult<(bool, globset::GlobMatcher), E>
        {
            let (negative, pattern) = if &value[..1] == "!" {
                (true, &value[1..])
            } else {
                (false, value)
            };
            Ok((negative, globset::Glob::new(pattern).map_err(|err| {
               de::Error::custom(format!(
                   "invalid pattern '{}': {}", value, err))})?.compile_matcher()))
        }

        impl de::Visitor for FilterVisitor {
            type Value = Vec<(bool, globset::GlobMatcher)>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a regular expression filter")
            }

            fn visit_seq<V: de::SeqVisitor>(self, mut visitor: V)
                -> StdResult<Vec<(bool, globset::GlobMatcher)>, V::Error>
            {
                let mut rv = vec![];
                while let Some(item) = visitor.visit::<String>()? {
                    rv.push(make_pattern(&item)?);
                }
                Ok(rv)
            }

            fn visit_unit<E: de::Error>(self)
                -> StdResult<Vec<(bool, globset::GlobMatcher)>, E>
            {
                Ok(vec![])
            }

            fn visit_str<E: de::Error>(self, value: &str)
                -> StdResult<Vec<(bool, globset::GlobMatcher)>, E>
            {
                Ok(vec![make_pattern(value)?])
            }
        }

        deserializer.deserialize_seq(FilterVisitor).map(|patterns| {
            IgnorePatterns { patterns: patterns }
        })
    }
}

impl IgnorePatterns {
    pub fn is_match(&self, value: &str) -> bool {
        let mut rv = false;
        for &(negative, ref pattern) in self.patterns.iter() {
            if pattern.is_match(value) {
                rv = !negative;
            }
        }
        rv
    }
}

/// Helper that runs a function and captures panics.
///
/// The function needs to be reasonably protected against panics.  This
/// might poison mutexes and similar things.
pub fn run_isolated<F>(f: F)
    where F: FnOnce() -> Result<()>, F: Send
{
    let rv = panic::catch_unwind(panic::AssertUnwindSafe(move || {
        if let Err(err) = f() {
            use std::error::Error;
            error!("task failed: {}", &err);
            let mut cause = err.cause();
            while let Some(the_cause) = cause {
                error!("  caused by: {}", the_cause);
                cause = the_cause.cause();
            }
            if let Some(backtrace) = err.backtrace() {
                debug!("  Traceback: {:?}", backtrace);
            }
        }
    }));

    // the default panic handler will already have printed here
    if let Err(_) = rv {
        error!("task panicked!");
    }
}

pub struct ProgressReader<R: Read + Seek> {
    rdr: R,
    pb: ProgressBar,
}

/// Like ``io::copy`` but advances a progress bar set to bytes.
pub fn copy_with_progress<R: ?Sized, W: ?Sized>(progress: &ProgressBar,
                                                reader: &mut R, writer: &mut W)
    -> io::Result<u64>
    where R: Read, W: Write
{
    let mut buf = [0; 16384];
    let mut written = 0;
    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => return Ok(written),
            Ok(len) => len,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        writer.write_all(&buf[..len])?;
        written += len as u64;
        progress.inc(len as u64);
    }
}

impl<R: Read + Seek> ProgressReader<R> {
    pub fn new(mut rdr: R) -> Result<ProgressReader<R>> {
        let len = rdr.seek(SeekFrom::End(0))?;
        rdr.seek(SeekFrom::Start(0))?;
        Ok(ProgressReader {
            rdr: rdr,
            pb: ProgressBar::new(len),
        })
    }
}

impl<R: Read + Seek> ProgressReader<R> {
    pub fn progress(&self) -> &ProgressBar {
        &self.pb
    }
}

impl<R: Read + Seek> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let rv = self.rdr.read(buf)?;
        self.pb.inc(rv as u64);
        Ok(rv)
    }
}

/// Formats a file size for human readable display.
pub fn file_size_format(bytes: usize) -> String {
    use humansize::FileSize;
    use humansize::file_size_opts::BINARY;
    bytes.file_size(BINARY)
        .map(|x| x.replace(" ", ""))
        .unwrap_or_else(|_| bytes.to_string())
}

/// Checks if we are running in docker
pub fn is_docker() -> bool {
    if fs::metadata("/.dockerenv").is_ok() {
        return true;
    }
    if let Ok(mut f) = fs::File::open("/proc/self/cgroup") {
        let mut s = String::new();
        if f.read_to_string(&mut s).is_ok() && s.find("/docker").is_some() {
            return true;
        }
    }
    false
}

/// Returns a single systemd socket fd if there is one.
pub fn get_systemd_fd() -> Result<Option<RawFd>> {
    let var = match env::var("LISTEN_FDS") {
        Ok(var) => {
            if &var == "0" || &var == "" {
                return Ok(None);
            }
            var
        }
        Err(_) => { return Ok(None); }
    };

    let fds : u32 = var.parse().chain_err(
        || Error::from(ErrorKind::BadEnvVar("LISTEN_FDS", "Not an integer")))?;
    if fds != 1 {
        return Err(ErrorKind::BadEnvVar(
            "LISTEN_FDS", "Exactly one socket needs to be passed").into());
    }

    env::remove_var("LISTEN_FDS");
    Ok(Some(SD_LISTEN_FDS_START))
}

/// A quick binary search by key.
pub fn binsearch_by_key<'a, T, B, F>(slice: &'a [T], item: B, mut f: F) -> Option<&'a T>
    where B: Ord, F: FnMut(&T) -> B
{
    let mut low = 0;
    let mut high = slice.len();

    while low < high {
        let mid = (low + high) / 2;
        let cur_item = &slice[mid as usize];
        if item < f(cur_item) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    if low > 0 && low <= slice.len() {
        Some(&slice[low - 1])
    } else {
        None
    }
}

#[test]
fn test_binsearch() {
    let seq = [0u32, 2, 4, 6, 8, 10];
    let m = binsearch_by_key(&seq[..], 5, |&x| x);
    assert_eq!(*m.unwrap(), 4);
}
