use anyhow::{Context, Result};
use std::process::{Command, Stdio};

pub struct CommandResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_command(program: &str, args: &[&str]) -> Result<CommandResult> {
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to execute: {} {}", program, args.join(" ")))?;

    Ok(CommandResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub fn run_command_inherit(program: &str, args: &[&str]) -> Result<bool> {
    let status = Command::new(program)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute: {} {}", program, args.join(" ")))?;

    Ok(status.success())
}

pub fn cargo_metadata() -> Result<CommandResult> {
    run_command("cargo", &["metadata", "--format-version=1", "--no-deps"])
}

pub fn cargo_search(name: &str) -> Result<CommandResult> {
    run_command("cargo", &["search", name, "--limit", "1"])
}

pub fn cargo_add_package(name: &str, version: &str) -> Result<bool> {
    let spec = format!("{name}@{version}");
    run_command_inherit("cargo", &["add", &spec])
}

pub fn cargo_check() -> Result<bool> {
    run_command_inherit("cargo", &["check"])
}

pub fn cargo_test() -> Result<bool> {
    run_command_inherit("cargo", &["test"])
}

pub fn cargo_clippy() -> Result<bool> {
    run_command_inherit("cargo", &["clippy", "--", "-D", "warnings"])
}

pub fn git_add_and_commit(message: &str) -> Result<bool> {
    let add_ok = run_command_inherit("git", &["add", "-A"])?;
    if !add_ok {
        return Ok(false);
    }
    run_command_inherit("git", &["commit", "-m", message])
}

pub fn check_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn check_git_clean() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map(|o| o.status.success() && o.stdout.is_empty())
        .unwrap_or(false)
}
