use std::io;
use std::str::Utf8Error;

use mach_object;
use zip;
use walkdir;


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
    }

    foreign_links {
        Io(io::Error);
        WalkDir(walkdir::Error);
        MachO(mach_object::Error);
        Zip(zip::result::ZipError);
        Utf8Error(Utf8Error);
    }
}
