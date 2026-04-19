use anyhow::{Context, Result};
use std::process::{Command, Stdio};

/// Spawn a subprocess with inherited stdio. Returns `Err` on spawn failure or non-zero exit.
pub fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute: {} {}", program, args.join(" ")))?;

    if !status.success() {
        anyhow::bail!(
            "{} {} exited with code {}",
            program,
            args.join(" "),
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

pub fn check() -> Result<()> {
    run("cargo", &["check"])
}

pub fn test() -> Result<()> {
    run("cargo", &["test"])
}

pub fn clippy() -> Result<()> {
    run("cargo", &["clippy", "--", "-D", "warnings"])
}

pub fn fmt() -> Result<()> {
    run("cargo", &["fmt", "--check"])
}
