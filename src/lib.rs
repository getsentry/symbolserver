//! This crate implements symbol handling for system libraries
#![recursion_limit = "1024"]

#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
#[macro_use] extern crate error_chain;
extern crate zip;
extern crate walkdir;
extern crate uuid;
extern crate regex;
#[macro_use] extern crate lazy_static;
extern crate mach_object;
extern crate memmap;
extern crate clap;
extern crate pbr;
extern crate xz2;
extern crate tempfile;
extern crate humansize;

pub use errors::{Result, Error, ErrorKind, ResultExt};

mod macros;
mod errors;
mod memdbdump;
mod memdbtypes;
mod utils;
mod config;

pub mod cli;
pub mod dsym;
pub mod sdk;
pub mod memdb;
