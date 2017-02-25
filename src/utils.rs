//! Provides various useful utilities.
use std::io;
use std::io::{Read, Write};
use std::sync::Mutex;

use pbr;


/// A thread safe progress bar
pub struct ProgressIndicator {
    pb: Mutex<Option<pbr::ProgressBar<io::Stdout>>>,
}

fn make_progress_bar(count: usize) -> pbr::ProgressBar<io::Stdout> {
    let mut pb = pbr::ProgressBar::new(count as u64);
    pb.tick_format("⠇⠋⠙⠸⠴⠦");
    pb.format("[■□□]");
    pb.show_tick = true;
    pb.show_speed = false;
    pb.show_percent = false;
    pb.show_counter = false;
    pb.show_time_left = false;
    pb.message(&format!("{: <44}", ""));
    pb
}

impl ProgressIndicator {
    /// Creates a new progress bar for a given count
    pub fn new(count: usize) -> ProgressIndicator {
        ProgressIndicator {
            pb: Mutex::new(Some(make_progress_bar(count))),
        }
    }

    /// Creates a dummy progress bar that does nothing
    pub fn disabled() -> ProgressIndicator {
        ProgressIndicator {
            pb: Mutex::new(None),
        }
    }

    /// Increments the progress bar by a step counter
    pub fn inc(&self, step: usize) {
        if let Some(ref mut pb) = *self.pb.lock().unwrap() {
            pb.add(step as u64);
        }
    }

    /// Ticks the progress bar without advancing
    pub fn tick(&self) {
        if let Some(ref mut pb) = *self.pb.lock().unwrap() {
            pb.tick();
        }
    }

    /// Sets the current message
    pub fn set_message(&self, msg: &str) {
        if let Some(ref mut pb) = *self.pb.lock().unwrap() {
            pb.message(&format!("  ◦ {: <40}", msg));
            pb.tick();
        }
    }

    /// Marks the progress bar finished and replaces it with a message
    pub fn finish(&self, msg: &str) {
        if let Some(ref mut pb) = *self.pb.lock().unwrap() {
            pb.finish_print(&format!("  ● {}", msg));
            println!("");
        }
    }

    /// Finishes the current bar and adds a new one.  If the progress bar
    /// is disabled this does nothing instead.
    pub fn add_bar(&self, count: usize) {
        let mut pb = self.pb.lock().unwrap();
        if !pb.is_none() {
            *pb = Some(make_progress_bar(count));
        }
    }
}

/// Like ``io::copy`` but advances a progress bar set to bytes.
pub fn copy_with_progress<R: ?Sized, W: ?Sized>(progress: &ProgressIndicator,
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
        progress.inc(len);
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
