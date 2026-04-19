use anyhow::{Context, Result, bail};
use colored::Colorize;
use std::time::Instant;

use crate::cargo_cmd;
use crate::cli::DepsArgs;
use crate::discovery;
use crate::git;
use crate::manifest;
use crate::parser;

#[derive(Debug)]
pub struct UpgradeSummary {
    pub done: usize,
    pub skipped: usize,
    pub failed: Vec<(String, &'static str)>,
}

pub struct CheckStep {
    pub name: &'static str,
    pub label: &'static str,
    pub skip: bool,
    pub run: fn() -> Result<()>,
}

fn run_check_step(step: &CheckStep, context: &str) -> Result<()> {
    if step.skip {
        return Ok(());
    }
    let t = Instant::now();
    println!("\n{}", format!("Running {}...", step.label).dimmed());
    (step.run)().with_context(|| format!("{}: {}", context, step.name))?;
    println!(
        "{}",
        format!("PASS: {} ({:.1}s)", step.name, t.elapsed().as_secs_f64()).green()
    );
    Ok(())
}

pub fn run_upgrade_loop(
    workspace: &manifest::Workspace,
    packages: &[parser::OutdatedPackage],
    args: &DepsArgs,
    steps: &[CheckStep],
) -> Result<UpgradeSummary> {
    let continue_on_failure = args.continue_on_failure || args.no_revert_on_failure;
    let total = packages.len();
    let mut done = 0usize;
    let mut skipped = 0usize;
    let mut failed: Vec<(String, &'static str)> = Vec::new();

    for (i, pkg) in packages.iter().enumerate() {
        println!(
            "\n{}",
            format!(
                "[{}/{}] Upgrading {} {} -> {}",
                i + 1,
                total,
                pkg.name,
                pkg.old_version,
                pkg.new_version
            )
            .bold()
            .cyan()
        );

        println!("\n{}", "Updating dependency version...".dimmed());
        if !manifest::update_dependency_in_workspace(
            workspace,
            &pkg.name,
            &pkg.new_version.to_string(),
        )? {
            println!(
                "{}",
                format!("SKIP: {} not found in any Cargo.toml", pkg.name).yellow()
            );
            skipped += 1;
            continue;
        }

        let check_failed = steps
            .iter()
            .find_map(|step| match run_check_step(step, &pkg.name) {
                Ok(()) => None,
                Err(e) => Some((step.name, e)),
            });

        if let Some((step_name, _err)) = check_failed {
            println!("{}", format!("FAIL: {}", step_name).red().bold());

            if !args.no_revert_on_failure {
                println!("{}", "Reverting changes...".dimmed());
                git::restore(workspace)?;
            }

            if continue_on_failure {
                failed.push((pkg.name.clone(), step_name));
                continue;
            }

            bail!(
                "{} failed for {}. Fix the issue and re-run to continue with remaining packages.",
                step_name,
                pkg.name
            );
        }

        let commit_msg = format!(
            "Upgrade {} {} -> {}",
            pkg.name, pkg.old_version, pkg.new_version
        );
        git::add_and_commit(workspace, &commit_msg)?;
        println!("{}", format!("Committed: {}", commit_msg).green());
        done += 1;
    }

    Ok(UpgradeSummary {
        done,
        skipped,
        failed,
    })
}

pub fn run(args: DepsArgs) -> Result<()> {
    // Verify Cargo.toml exists
    if !std::path::Path::new("Cargo.toml").exists() {
        bail!("No Cargo.toml found in current directory");
    }

    // Verify git repo
    if !git::check_repo() {
        bail!("Not inside a git repository. Initialize one with `git init`.");
    }

    // Verify clean working tree (skip for dry-run)
    if !args.dry_run && !git::check_clean() {
        bail!(
            "Working tree is not clean. Commit or stash your changes before running cargo-bump-deps."
        );
    }

    let steps = [
        CheckStep {
            name: "cargo check",
            label: "cargo check",
            skip: args.no_check,
            run: cargo_cmd::check,
        },
        CheckStep {
            name: "cargo test",
            label: "cargo test",
            skip: args.no_test,
            run: cargo_cmd::test,
        },
        CheckStep {
            name: "cargo clippy",
            label: "cargo clippy",
            skip: args.no_clippy,
            run: cargo_cmd::clippy,
        },
        CheckStep {
            name: "cargo fmt",
            label: "cargo fmt --check",
            skip: args.no_fmt,
            run: cargo_cmd::fmt,
        },
    ];

    // Pre-flight checks (skip for dry-run)
    if !args.dry_run {
        println!("\n{}", "Running pre-flight checks...".bold());

        for step in &steps {
            run_check_step(step, "Pre-flight")?;
        }

        println!("{}", "\nPre-flight checks passed!".green().bold());
    }

    // Discover outdated packages
    println!("Checking for outdated dependencies...");
    let packages = discovery::find_outdated_packages(
        args.compatible_only,
        args.pre,
        &args.exclude,
        args.jobs,
    )?;

    if packages.is_empty() {
        println!("{}", "All dependencies are up to date!".green());
        return Ok(());
    }

    // Handle --dry-run
    if args.dry_run {
        println!(
            "\n{}",
            format!("Found {} outdated packages:", packages.len()).bold()
        );
        println!(
            "  {:<30} {:<15} {}",
            "Package".bold(),
            "Current".bold(),
            "New".bold()
        );
        println!("  {}", "-".repeat(65));
        for pkg in &packages {
            println!(
                "  {:<30} {:<15} {}",
                pkg.name, pkg.old_version, pkg.new_version
            );
        }
        return Ok(());
    }

    let workspace = manifest::load_workspace()?;
    let run_start = Instant::now();
    let summary = run_upgrade_loop(&workspace, &packages, &args, &steps)?;
    let elapsed = run_start.elapsed();

    if !summary.failed.is_empty() {
        println!(
            "\n{}",
            format!("Failed ({}):", summary.failed.len()).red().bold()
        );
        for (name, step) in &summary.failed {
            println!("  {} ({})", name, step);
        }
    }

    println!(
        "\n{}",
        format!(
            "Done! {} upgraded, {} failed, {} skipped in {:.1}s",
            summary.done,
            summary.failed.len(),
            summary.skipped,
            elapsed.as_secs_f64()
        )
        .green()
        .bold()
    );

    if !summary.failed.is_empty() {
        bail!(
            "{} package(s) failed to upgrade. Fix the issues and re-run.",
            summary.failed.len()
        );
    }

    Ok(())
}
