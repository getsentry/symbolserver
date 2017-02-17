#![macro_use]

macro_rules! try_opt {
    ($expr:expr) => {
        match $expr {
            Some(rv) => rv,
            None => { return None; }
        }
    }
}

macro_rules! iter_try {
    ($expr:expr) => {
        match $expr {
            Ok(rv) => rv,
            Err(err) => {
                return Some(Err(::std::convert::From::from(err)));
            }
        }
    }
}
