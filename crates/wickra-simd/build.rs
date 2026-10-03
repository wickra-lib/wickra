//! Enables the AVX-512 dispatch level (`cfg(wickra_avx512)`) when the compiler
//! supports AVX-512 target features, stable since Rust 1.89. An older compiler
//! -- the workspace's minimum supported one included -- builds the crate
//! without that level; a kernel returns the same bits at every level, so only
//! the speed differs.

use std::process::Command;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(wickra_avx512)");
    println!("cargo::rerun-if-env-changed=RUSTC");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let minor = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|version| minor_version(&version));
    if minor.is_some_and(|minor| minor >= 89) {
        println!("cargo::rustc-cfg=wickra_avx512");
    }
}

/// The minor version of `rustc 1.<minor>.<patch>...` output.
fn minor_version(version: &str) -> Option<u32> {
    let numbers = version.split_whitespace().nth(1)?;
    let mut parts = numbers.split('.');
    (parts.next()? == "1").then_some(())?;
    parts.next()?.parse().ok()
}
