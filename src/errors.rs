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
        MissingConfigKey(path: &'static str, env_var: Option<&'static str>) {
            description("encountered a missing config key")
            display("encountered missing config key '{}'{}'.", path, match env_var {
                &Some(env_var) => {
                    format!(" (can also be set with environment variable '{}')", env_var)
                }
                _ => "".into()
            })
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
