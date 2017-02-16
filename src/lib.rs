#![recursion_limit = "1024"]

#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate serde_json;
#[macro_use] extern crate error_chain;
extern crate zip;
extern crate walkdir;
extern crate regex;
#[macro_use] extern crate lazy_static;
extern crate mach_object;
extern crate memmap;

pub use errors::{Result, Error, ErrorKind};

mod macros;
mod errors;
pub mod dsym;
pub mod sdk;
