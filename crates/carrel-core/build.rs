//! Records the git commit Carrel is built from as `CARREL_GIT_COMMIT`, for feedback
//! ([`carrel_core::feedback::GIT_COMMIT`]), when the build is in a git checkout.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn main() {
    println!("cargo:rerun-if-env-changed=CARREL_GIT_COMMIT");
    if std::env::var_os("CARREL_GIT_COMMIT").is_some() {
        return;
    }
    // Rebuilt when this checkout's HEAD moves: its reflog changes on every commit and switch.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/logs/HEAD");
    }
    if let Some(commit) = git(&["rev-parse", "--short=10", "HEAD"]) {
        println!("cargo:rustc-env=CARREL_GIT_COMMIT={commit}");
    }
}
