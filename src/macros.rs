#![macro_use]

macro_rules! try_opt {
    ($expr:expr) => {
        match $expr {
            Some(rv) => rv,
            None => { return None; }
        }
    }
}
