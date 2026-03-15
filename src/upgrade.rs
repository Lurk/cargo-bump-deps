use anyhow::{Result, bail};
use colored::Colorize;
use std::time::Instant;

use crate::cli::DepsArgs;
use crate::discovery;
use crate::runner;
use crate::state::{self, PackageStatus, State};

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

    // Handle --reset
    if args.reset {
        state::delete_state()?;
        println!("{}", "State file deleted.".green());
        if args.dry_run {
            // Continue to show dry-run output
        } else {
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
        let packages =
            discovery::find_outdated_packages(args.compatible_only, &args.exclude, args.jobs)?;

        if packages.is_empty() {
            println!("{}", "All dependencies are up to date!".green());
            return Ok(());
        }
        State::from_packages(packages)
    };

    // Handle --dry-run
    if args.dry_run {
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
            println!(
                "{}",
                format!("SKIP: {} not found in any Cargo.toml", name).yellow()
            );
            state.packages[i].status = PackageStatus::Skipped;
            state::save_state(&state)?;
            continue;
        }

        // Run all check steps
        let check_failed = steps
            .iter()
            .find_map(|step| match run_check_step(step, &name) {
                Ok(()) => None,
                Err(e) => Some((step.name, e)),
            });

        if let Some((step_name, _err)) = check_failed {
            println!("{}", format!("FAIL: {}", step_name).red().bold());
            state.packages[i].status = PackageStatus::Failed;
            state::save_state(&state)?;

            if !args.no_revert_on_failure {
                println!("{}", "Reverting changes...".dimmed());
                runner::git_restore()?;
            }

            print_resume_instructions(&name);
            bail!("{} failed for {}", step_name, name);
        }

        // All passed — commit
        let commit_msg = format!("Upgrade {} {} -> {}", name, old_version, new_version);
        let committed = runner::git_add_and_commit(&commit_msg)?;
        if !committed {
            println!(
                "{}",
                format!("SKIP: no changes to commit for {}", name).yellow()
            );
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

    let done = state
        .packages
        .iter()
        .filter(|p| p.status == PackageStatus::Done)
        .count();
    let skipped = state
        .packages
        .iter()
        .filter(|p| p.status == PackageStatus::Skipped)
        .count();
    let elapsed = run_start.elapsed();

    println!(
        "\n{}",
        format!(
            "Done! {} upgraded, {} skipped in {:.1}s",
            done,
            skipped,
            elapsed.as_secs_f64()
        )
        .green()
        .bold()
    );

    Ok(())
}

fn print_resume_instructions(name: &str) {
    println!(
        "\n{}",
        "Fix the issue, then run `cargo bump-deps` to resume.".yellow()
    );
    println!(
        "{}",
        format!(
            "Run `cargo bump-deps --reset --exclude {}` to restart without this package.",
            name
        )
        .yellow()
    );
    println!(
        "{}",
        "Run `cargo bump-deps --reset` to start over.".yellow()
    );
}
