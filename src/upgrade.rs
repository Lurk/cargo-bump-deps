use anyhow::{bail, Result};
use colored::Colorize;
use std::time::Instant;

use crate::discovery;
use crate::runner;
use crate::state::{self, PackageStatus, State};

pub fn run(
    dry_run: bool,
    reset: bool,
    compatible_only: bool,
    exclude: Vec<String>,
    skip: Option<String>,
    jobs: usize,
) -> Result<()> {
    // Verify Cargo.toml exists
    if !std::path::Path::new("Cargo.toml").exists() {
        bail!("No Cargo.toml found in current directory");
    }

    // Verify git repo
    if !runner::check_git_repo() {
        bail!("Not inside a git repository. Initialize one with `git init`.");
    }

    // Verify clean working tree (skip for dry-run)
    if !dry_run && !runner::check_git_clean() {
        bail!("Working tree is not clean. Commit or stash your changes before running cargo-bump-deps.");
    }

    // Pre-flight checks (skip for dry-run)
    if !dry_run {
        println!("\n{}", "Running pre-flight checks...".bold());

        let t = Instant::now();
        println!("\n{}", "Running cargo check...".dimmed());
        if !runner::cargo_check()? {
            bail!("Pre-flight failed: cargo check. Fix the issues before running cargo-bump-deps.");
        }
        println!("{}", format!("PASS: cargo check ({:.1}s)", t.elapsed().as_secs_f64()).green());

        let t = Instant::now();
        println!("\n{}", "Running cargo test...".dimmed());
        if !runner::cargo_test()? {
            bail!("Pre-flight failed: cargo test. Fix the issues before running cargo-bump-deps.");
        }
        println!("{}", format!("PASS: cargo test ({:.1}s)", t.elapsed().as_secs_f64()).green());

        let t = Instant::now();
        println!("\n{}", "Running cargo clippy...".dimmed());
        if !runner::cargo_clippy()? {
            bail!("Pre-flight failed: cargo clippy. Fix the issues before running cargo-bump-deps.");
        }
        println!("{}", format!("PASS: cargo clippy ({:.1}s)", t.elapsed().as_secs_f64()).green());

        let t = Instant::now();
        println!("\n{}", "Running cargo fmt --check...".dimmed());
        if !runner::cargo_fmt()? {
            bail!("Pre-flight failed: cargo fmt. Fix the issues before running cargo-bump-deps.");
        }
        println!("{}", format!("PASS: cargo fmt ({:.1}s)", t.elapsed().as_secs_f64()).green());

        println!("{}", "\nPre-flight checks passed!".green().bold());
    }

    // Handle --reset
    if reset {
        state::delete_state()?;
        println!("{}", "State file deleted.".green());
        if dry_run {
            // Continue to show dry-run output
        } else {
            return Ok(());
        }
    }

    // Handle --skip
    if let Some(ref skip_name) = skip {
        if let Some(mut existing) = state::load_state()? {
            if existing.skip_package(skip_name) {
                state::save_state(&existing)?;
                println!(
                    "{}",
                    format!("Skipped package: {}", skip_name).yellow()
                );
            } else {
                println!(
                    "{}",
                    format!("Package '{}' not found or already done.", skip_name).red()
                );
                return Ok(());
            }
        } else {
            println!("{}", "No state file found. Nothing to skip.".red());
            return Ok(());
        }
    }

    // Load or create state
    let mut state = if let Some(existing) = state::load_state()? {
        let resume_idx = existing.resume_index();
        let total = existing.packages.len();
        if resume_idx < total {
            println!(
                "{}",
                format!(
                    "Resuming from package {}/{} ({})",
                    resume_idx + 1,
                    total,
                    existing.packages[resume_idx].name
                )
                .yellow()
            );
        }
        existing
    } else {
        // Discover outdated packages
        println!("Checking for outdated dependencies...");
        let packages = discovery::find_outdated_packages(compatible_only, &exclude, jobs)?;

        if packages.is_empty() {
            println!("{}", "All dependencies are up to date!".green());
            return Ok(());
        }
        State::from_packages(packages)
    };

    // Handle --dry-run
    if dry_run {
        println!(
            "\n{}",
            format!("Found {} outdated packages:", state.packages.len()).bold()
        );
        println!(
            "  {:<30} {:<15} {}",
            "Package".bold(),
            "Current".bold(),
            "New".bold()
        );
        println!("  {}", "-".repeat(65));
        for pkg in &state.packages {
            let status = match pkg.status {
                PackageStatus::Done => " (done)".green().to_string(),
                PackageStatus::Failed => " (failed)".red().to_string(),
                PackageStatus::Skipped => " (skipped)".yellow().to_string(),
                PackageStatus::Pending => String::new(),
            };
            println!(
                "  {:<30} {:<15} {}{}",
                pkg.name, pkg.old_version, pkg.new_version, status
            );
        }
        return Ok(());
    }

    let run_start = Instant::now();
    let total = state.packages.len();
    let start = state.resume_index();

    if start >= total {
        println!("{}", "All packages already upgraded!".green());
        state::delete_state()?;
        return Ok(());
    }

    for i in start..total {
        if state.packages[i].status == PackageStatus::Skipped {
            continue;
        }

        let pkg = &state.packages[i];
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

        let name = pkg.name.clone();
        let old_version = pkg.old_version.clone();
        let new_version = pkg.new_version.clone();

        println!("\n{}", "Updating dependency version...".dimmed());
        if !runner::update_dependency_in_workspace(&name, &new_version)? {
            println!("{}", format!("SKIP: {} not found in any Cargo.toml", name).yellow());
            state.packages[i].status = PackageStatus::Skipped;
            state::save_state(&state)?;
            continue;
        }

        // cargo check
        let t = Instant::now();
        println!("\n{}", "Running cargo check...".dimmed());
        if !runner::cargo_check()? {
            println!("{}", "FAIL: cargo check".red().bold());
            state.packages[i].status = PackageStatus::Failed;
            state::save_state(&state)?;
            print_resume_instructions(&name);
            bail!("cargo check failed for {}", name);
        }
        println!("{}", format!("PASS: cargo check ({:.1}s)", t.elapsed().as_secs_f64()).green());

        // cargo test
        let t = Instant::now();
        println!("\n{}", "Running cargo test...".dimmed());
        if !runner::cargo_test()? {
            println!("{}", "FAIL: cargo test".red().bold());
            state.packages[i].status = PackageStatus::Failed;
            state::save_state(&state)?;
            print_resume_instructions(&name);
            bail!("cargo test failed for {}", name);
        }
        println!("{}", format!("PASS: cargo test ({:.1}s)", t.elapsed().as_secs_f64()).green());

        // cargo clippy
        let t = Instant::now();
        println!("\n{}", "Running cargo clippy...".dimmed());
        if !runner::cargo_clippy()? {
            println!("{}", "FAIL: cargo clippy".red().bold());
            state.packages[i].status = PackageStatus::Failed;
            state::save_state(&state)?;
            print_resume_instructions(&name);
            bail!("cargo clippy failed for {}", name);
        }
        println!("{}", format!("PASS: cargo clippy ({:.1}s)", t.elapsed().as_secs_f64()).green());

        // cargo fmt
        let t = Instant::now();
        println!("\n{}", "Running cargo fmt --check...".dimmed());
        if !runner::cargo_fmt()? {
            println!("{}", "FAIL: cargo fmt".red().bold());
            state.packages[i].status = PackageStatus::Failed;
            state::save_state(&state)?;
            print_resume_instructions(&name);
            bail!("cargo fmt failed for {}", name);
        }
        println!("{}", format!("PASS: cargo fmt ({:.1}s)", t.elapsed().as_secs_f64()).green());

        // All passed — commit
        let commit_msg = format!("Upgrade {} {} -> {}", name, old_version, new_version);
        let committed = runner::git_add_and_commit(&commit_msg)?;
        if !committed {
            println!("{}", format!("SKIP: no changes to commit for {}", name).yellow());
            state.packages[i].status = PackageStatus::Skipped;
            state::save_state(&state)?;
            continue;
        }
        println!("{}", format!("Committed: {}", commit_msg).green());

        state.packages[i].status = PackageStatus::Done;
        state::save_state(&state)?;
    }

    // All done
    state::delete_state()?;

    let done = state.packages.iter().filter(|p| p.status == PackageStatus::Done).count();
    let skipped = state.packages.iter().filter(|p| p.status == PackageStatus::Skipped).count();
    let elapsed = run_start.elapsed();

    println!(
        "\n{}",
        format!(
            "Done! {} upgraded, {} skipped in {:.1}s",
            done, skipped, elapsed.as_secs_f64()
        )
        .green()
        .bold()
    );

    Ok(())
}

fn print_resume_instructions(name: &str) {
    println!(
        "\n{}",
        "Fix the issue, then run `cargo deps` to resume.".yellow()
    );
    println!(
        "{}",
        format!("Run `cargo deps --skip {}` to skip this package.", name).yellow()
    );
    println!(
        "{}",
        "Run `cargo deps --reset` to start over.".yellow()
    );
}
