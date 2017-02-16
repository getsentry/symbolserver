use std::io;


error_chain! {
    foreign_links {
        IoError(io::Error);
    }
}
