use std::io;

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
    }

    foreign_links {
        Io(io::Error);
        WalkDir(walkdir::Error);
        MachO(mach_object::Error);
        Zip(zip::result::ZipError);
    }
}
