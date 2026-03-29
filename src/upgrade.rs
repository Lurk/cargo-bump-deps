use anyhow::{Result, bail};
use colored::Colorize;
use std::time::Instant;

use crate::cli::DepsArgs;
use crate::discovery;
use crate::runner;

struct CheckStep {
    name: &'static str,
    label: &'static str,
    skip: bool,
    run: fn() -> Result<bool>,
}

fn run_check_step(step: &CheckStep, context: &str) -> Result<()> {
    if step.skip {
        return Ok(());
    }
    let t = Instant::now();
    println!("\n{}", format!("Running {}...", step.label).dimmed());
    if !(step.run)()? {
        bail!("{} failed: {}", context, step.name);
    }
    println!(
        "{}",
        format!("PASS: {} ({:.1}s)", step.name, t.elapsed().as_secs_f64()).green()
    );
    Ok(())
}

pub fn run(args: DepsArgs) -> Result<()> {
    // Verify Cargo.toml exists
    if !std::path::Path::new("Cargo.toml").exists() {
        bail!("No Cargo.toml found in current directory");
    }

    // Verify git repo
    if !runner::check_git_repo() {
        bail!("Not inside a git repository. Initialize one with `git init`.");
    }

    // Verify clean working tree (skip for dry-run)
    if !args.dry_run && !runner::check_git_clean() {
        bail!(
            "Working tree is not clean. Commit or stash your changes before running cargo-bump-deps."
        );
    }

    let steps = [
        CheckStep {
            name: "cargo check",
            label: "cargo check",
            skip: args.no_check,
            run: runner::cargo_check,
        },
        CheckStep {
            name: "cargo test",
            label: "cargo test",
            skip: args.no_test,
            run: runner::cargo_test,
        },
        CheckStep {
            name: "cargo clippy",
            label: "cargo clippy",
            skip: args.no_clippy,
            run: runner::cargo_clippy,
        },
        CheckStep {
            name: "cargo fmt",
            label: "cargo fmt --check",
            skip: args.no_fmt,
            run: runner::cargo_fmt,
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

    let continue_on_failure = args.continue_on_failure || args.no_revert_on_failure;
    let manifest_paths = runner::workspace_manifest_paths()?;
    let run_start = Instant::now();
    let total = packages.len();
    let mut done = 0usize;
    let mut skipped = 0usize;
    let mut failed: Vec<(&str, &str)> = Vec::new();

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
        if !runner::update_dependency_in_workspace(
            &manifest_paths,
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

        // Run all check steps
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
                runner::git_restore()?;
            }

            if continue_on_failure {
                failed.push((&pkg.name, step_name));
                continue;
            }

            bail!(
                "{} failed for {}. Fix the issue and re-run to continue with remaining packages.",
                step_name,
                pkg.name
            );
        }

        // All passed — commit
        let commit_msg = format!(
            "Upgrade {} {} -> {}",
            pkg.name, pkg.old_version, pkg.new_version
        );
        let committed = runner::git_add_and_commit(&commit_msg)?;
        if !committed {
            println!(
                "{}",
                format!("SKIP: no changes to commit for {}", pkg.name).yellow()
            );
            skipped += 1;
            continue;
        }
        println!("{}", format!("Committed: {}", commit_msg).green());
        done += 1;
    }

    let elapsed = run_start.elapsed();

    if !failed.is_empty() {
        println!("\n{}", format!("Failed ({}):", failed.len()).red().bold());
        for (name, step) in &failed {
            println!("  {} ({})", name, step);
        }
    }

    println!(
        "\n{}",
        format!(
            "Done! {} upgraded, {} failed, {} skipped in {:.1}s",
            done,
            failed.len(),
            skipped,
            elapsed.as_secs_f64()
        )
        .green()
        .bold()
    );

    if !failed.is_empty() {
        bail!(
            "{} package(s) failed to upgrade. Fix the issues and re-run.",
            failed.len()
        );
    }

    Ok(())
}
