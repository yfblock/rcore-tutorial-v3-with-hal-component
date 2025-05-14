use std::{env, io::Result};

static TARGET_PATH: &str = "../user/target/riscv64gc-unknown-none-elf/release/";

fn main() -> Result<()> {
    gen_linker_script()?;
    println!("cargo:rerun-if-changed=../user/src/");
    println!("cargo:rerun-if-changed=fs-img.img");
    println!("cargo:rerun-if-changed={}", TARGET_PATH);
    Ok(())
}

fn gen_linker_script() -> Result<()> {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("can't find target");
    let fname = format!("linker_{}.lds", arch);
    let (output_arch, kernel_base) = if arch == "x86_64" {
        println!("cargo:rustc-link-arg=-no-pie");
        ("i386:x86-64", "0xffff800000200000")
    } else if arch.contains("riscv64") {
        ("riscv", "0xffffffc080200000")
    } else if arch.contains("aarch64") {
        ("aarch64", "0xffff000040080000")
    } else if arch.contains("loongarch64") {
        ("loongarch64", "0x9000000080000000")
    } else {
        (arch.as_str(), "0")
    };
    let ld_content = std::fs::read_to_string("linker.lds")?;
    let ld_content = ld_content.replace("%ARCH%", output_arch);
    let ld_content = ld_content.replace("%KERNEL_BASE%", kernel_base);

    std::fs::write(&fname, ld_content)?;
    println!("cargo:rustc-link-arg=-Tos/{}", fname);
    println!("cargo:rerun-if-env-changed=CARGO_CFG_KERNEL_BASE");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=linker.lds");
    Ok(())
}
