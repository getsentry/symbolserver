//! This crate implements symbol handling for system libraries
#![recursion_limit = "1024"]

#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate serde_json;
#[macro_use] extern crate error_chain;
extern crate zip;
extern crate walkdir;
extern crate uuid;
extern crate regex;
#[macro_use] extern crate lazy_static;
extern crate mach_object;
extern crate memmap;

pub use errors::{Result, Error, ErrorKind};

mod macros;
mod errors;
mod memdbdump;
mod memdbtypes;
pub mod dsym;
pub mod sdk;
pub mod memdb;

// public for the tests but hidden away
#[doc(hidden)]
pub mod shoco;
