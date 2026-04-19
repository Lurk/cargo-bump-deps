use anyhow::Result;
use std::process::{Command, Stdio};

use crate::cargo_cmd;
use crate::manifest::Workspace;

pub fn add_and_commit(workspace: &Workspace, message: &str) -> Result<()> {
    let lock_file = workspace.root.join("Cargo.lock");

    let mut add_args: Vec<String> = vec!["add".to_string(), "--".to_string()];
    for p in &workspace.manifest_paths {
        add_args.push(p.to_string_lossy().into_owned());
    }
    // Cargo.lock may be absent on a first-time build or in bare test fixtures.
    if lock_file.exists() {
        add_args.push(lock_file.to_string_lossy().into_owned());
    }

    let add_refs: Vec<&str> = add_args.iter().map(|s| s.as_str()).collect();
    cargo_cmd::run("git", &add_refs)?;
    cargo_cmd::run("git", &["commit", "-m", message])
}

pub fn restore(workspace: &Workspace) -> Result<()> {
    let lock_file = workspace.root.join("Cargo.lock");

    let mut args: Vec<String> = vec!["checkout".to_string(), "--".to_string()];
    for p in &workspace.manifest_paths {
        args.push(p.to_string_lossy().into_owned());
    }
    // Cargo.lock may be absent on a first-time build or in bare test fixtures.
    if lock_file.exists() {
        args.push(lock_file.to_string_lossy().into_owned());
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cargo_cmd::run("git", &arg_refs)
}

pub fn check_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn check_clean() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map(|o| o.status.success() && o.stdout.is_empty())
        .unwrap_or(false)
}
