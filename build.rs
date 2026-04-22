/// Build script that adds the nightly sysroot's lib directory to the linker
/// and crate search paths.  This is required to use `#![feature(rustc_private)]`
/// crates (rustc_driver, rustc_middle, etc.) without manually setting
/// `RUSTFLAGS="-L …/sysroot/lib"`.
fn main() {
    // Tell Cargo not to re-run this script unless it or the env changes.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RUSTUP_TOOLCHAIN");

    let output = std::process::Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .expect("failed to run `rustc --print sysroot`");

    let sysroot = std::str::from_utf8(&output.stdout)
        .expect("sysroot is not valid UTF-8")
        .trim()
        .to_owned();

    // Add the sysroot lib directory so `extern crate rustc_xxx` resolves.
    println!("cargo:rustc-link-search={sysroot}/lib");
}
