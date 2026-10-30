//! The server's version with the commit it was built from,
//! `ORG_LSP_VERSION`: `0.1.0-f7e7b68` for the commit in `ORG_LSP_COMMIT`,
//! which the release workflow sets; `0.1.0-f7e7b68-dev` for a build of the
//! checkout's commit anywhere else; `0.1.0-dev` outside a checkout.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=ORG_LSP_COMMIT");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let released = std::env::var("ORG_LSP_COMMIT").ok().filter(|commit| !commit.is_empty());
    let full = match (released, checkout_commit()) {
        (Some(commit), _) => format!("{version}-{}", &commit[..commit.len().min(7)]),
        (None, Some(commit)) => format!("{version}-{commit}-dev"),
        (None, None) => format!("{version}-dev"),
    };
    println!("cargo:rustc-env=ORG_LSP_VERSION={full}");
}

/// The commit the checkout is at, and the files that change with it, for
/// Cargo to build again when it moves.
fn checkout_commit() -> Option<String> {
    let commit = git(&["rev-parse", "--short=7", "HEAD"])?;
    let mut watched = vec![git(&["rev-parse", "--git-path", "HEAD"]), git(&["rev-parse", "--git-path", "packed-refs"])];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.push(git(&["rev-parse", "--git-path", &branch]));
    }
    // A file that is not there would build the crate again every time.
    for path in watched.into_iter().flatten().filter(|path| Path::new(path).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
    Some(commit)
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !text.is_empty()).then_some(text)
}
