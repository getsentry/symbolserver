//! Central error handling for the symbol server
use std::io;
use std::str::Utf8Error;
use std::string::FromUtf8Error;

use super::api::types::ApiError;

use mach_object;
use zip;
use walkdir;
use serde_yaml;
use serde_xml;
use url;
use hyper;


error_chain! {
    errors {
        UnknownSdk {
            description("unknown SDK")
        }
        UnknownArchitecture(arch: String) {
            description("unknown architecture")
            display("unknown architecture: '{}'", arch)
        }
        MissingArchitecture(arch: String) {
            description("missing architecture")
            display("missing architecture: '{}'", arch)
        }
        UnsupportedMemDbVersion {
            description("unsupported memdb version")
        }
        BadMemDb {
            description("bad memdb file")
        }
        ConfigError(err: serde_yaml::Error) {
            description("failed to load config file")
            display("failed to load config file: {}", err)
        }
        BadConfigKey(path: &'static str, msg: &'static str) {
            description("encountered a bad config key")
            display("config key '{}' has a bad value: {}", path, msg)
        }
        MissingConfigKey(path: &'static str) {
            description("encountered a missing config key")
            display("encountered missing config key '{}'", path)
        }
        BadEnvVar(path: &'static str, msg: &'static str) {
            description("bad environment variable")
            display("bad environment variable '{}': {}", path, msg)
        }
        S3Unavailable(msg: String) {
            description("S3 is unavailable")
            display("S3 is unavailable: {}", msg)
        }
    }

    foreign_links {
        Io(io::Error);
        WalkDir(walkdir::Error);
        MachO(mach_object::Error);
        Zip(zip::result::ZipError);
        Utf8Error(Utf8Error);
        FromUtf8Error(FromUtf8Error);
        YamlError(serde_yaml::Error);
        XmlError(serde_xml::Error);
        UrlParseError(url::ParseError);
        WebError(hyper::Error);
        ApiError(ApiError);
    }
}
