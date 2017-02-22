use std::io;
use std::str::Utf8Error;
use std::string::FromUtf8Error;

use mach_object;
use zip;
use walkdir;
use serde_yaml;


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
    }

    foreign_links {
        Io(io::Error);
        WalkDir(walkdir::Error);
        MachO(mach_object::Error);
        Zip(zip::result::ZipError);
        Utf8Error(Utf8Error);
        FromUtf8Error(FromUtf8Error);
        YamlError(serde_yaml::Error);
    }
}
