extern crate gcc;

fn main() {
    gcc::compile_library("libshoco.a", &["shoco/shoco.c"]);
    println!("cargo:rustc-link-lib=shoco");
}
