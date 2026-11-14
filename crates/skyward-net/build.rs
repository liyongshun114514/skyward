//! The version a PLK110 build names itself, which the update checker holds
//! a release against.
//!
//! The workspace's version stays upstream's 0.3.1, as the fork merges
//! upstream: a fork build names its own version in `SKYWARD_BUILD_VERSION`,
//! `0.3.1-plk110.21`, which semver reads as a pre-release of 0.3.1 and so
//! orders after `0.3.1-plk110.20` and before the next upstream release.
//!
//! Where the variable is not set, a build made from the release tag names
//! it from that tag. The value is handed to the crate so that `github.rs`
//! reads it with `option_env!`; Cargo is told to run this again when the
//! variable changes, so a build made after another names itself afresh.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=SKYWARD_BUILD_VERSION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let named = std::env::var("SKYWARD_BUILD_VERSION").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).or_else(from_tag);
    if let Some(named) = named {
        println!("cargo:rustc-env=SKYWARD_BUILD_VERSION={named}");
    }
}

/// The version of the release tag this checkout is at, such as
/// `0.3.1-plk110.21`; none where the checkout is not at a fork release tag.
fn from_tag() -> Option<String> {
    let out = Command::new("git").args(["describe", "--tags", "--exact-match", "HEAD"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let version = String::from_utf8(out.stdout).ok()?;
    let version = version.trim().strip_prefix('v')?.to_string();
    let build = version.split_once("-plk110.")?.1;
    build.parse::<u32>().ok()?;
    Some(version)
}
