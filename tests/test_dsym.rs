extern crate symbolserver;

use symbolserver::{call_llvm_nm};

#[test]
fn test_call_llvm() {
    call_llvm_nm();
}
