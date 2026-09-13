//! Build identity: embed the exact git source identity the host binary was
//! built from so runtime `BENCHMARK_CONFIG` lines carry an unambiguous tree
//! identity (CROSS-OS-NORMALIZED-DESKTOP-STARTUP-1; benchmark discipline
//! requires exact source identity on runtime-dependent evidence).

use std::process::Command;

fn git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn main() {
    println!("cargo:rustc-env=POCKETJS_GIT_SHA={}", git(&["rev-parse", "HEAD"]));
    println!(
        "cargo:rustc-env=POCKETJS_GIT_TREE={}",
        git(&["rev-parse", "HEAD^{tree}"])
    );
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
}
