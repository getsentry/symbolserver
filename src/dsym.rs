use std::io::prelude::BufRead;
use std::io::BufReader;
use std::process;

use super::Result;

pub struct SymbolIterator {
    buf: String,
    prc: process::Child,
    rdr: BufReader<process::ChildStdout>,
}

impl SymbolIterator {
    pub fn new(arch: &str, dsym_path: &str) -> Result<SymbolIterator> {
        let mut child = process::Command::new("nm")
            .arg("-numeric-sort")
            .arg("-arch")
            .arg(arch)
            .arg(dsym_path)
            .stdout(process::Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().unwrap();
        Ok(SymbolIterator {
            buf: String::new(),
            prc: child,
            rdr: BufReader::new(stdout),
        })
    }
}

impl Iterator for SymbolIterator {
    type Item = (u64, String);

    fn next(&mut self) -> Option<(u64, String)> {
        loop {
            self.buf.clear();
            if self.rdr.read_line(&mut self.buf).unwrap_or(0) == 0 {
                break;
            }
            let v : Vec<_> = self.buf.trim().split(' ').collect();
            if v[1] != "t" && v[1] != "T" {
                continue;
            }
            if let Ok(addr) = u64::from_str_radix(v[0], 16) {
                return Some((addr, v[2].into()));
            }
        }
        None
    }
}

impl Drop for SymbolIterator {
    fn drop(&mut self) {
        self.prc.kill().ok();
    }
}
