use std::io;

use mach_object;


error_chain! {
    errors {
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
        MachO(mach_object::Error);
    }
}
