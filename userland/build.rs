use std::env;
fn main() {
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{}/userland.ld", dir);
    println!("cargo:rerun-if-changed=userland.ld");
}
