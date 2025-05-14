fn main() {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("can't find target");
    if arch == "x86_64" {
        println!("cargo:rustc-link-arg=-no-pie");
    }
    println!("cargo:rustc-link-arg=-Tuser/src/linker.ld");
}
