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
pub struct UpgradedEntry {
    pub name: String,
    pub old_version: String,
    pub new_version: String,
}

#[derive(Debug)]
pub struct FailedEntry {
    pub name: String,
    pub old_version: String,
    pub new_version: String,
    pub step: &'static str,
}

#[derive(Debug)]
pub struct SkippedEntry {
    pub name: String,
    pub old_version: String,
    pub new_version: String,
    pub reason: &'static str,
}

#[derive(Debug, Default)]
pub struct UpgradeSummary {
    pub upgraded: Vec<UpgradedEntry>,
    pub failed: Vec<FailedEntry>,
    pub skipped: Vec<SkippedEntry>,
}

pub fn render_summary(summary: &UpgradeSummary) -> String {
    let mut sections: Vec<String> = Vec::new();

    if !summary.upgraded.is_empty() {
        let mut s = format!(
            "{}\n",
            format!("Upgraded ({}):", summary.upgraded.len())
                .green()
                .bold()
        );
        for e in &summary.upgraded {
            s.push_str(&format!(
                "  {} {} -> {}\n",
                e.name, e.old_version, e.new_version
            ));
        }
        sections.push(s);
    }

    if !summary.failed.is_empty() {
        let mut s = format!(
            "{}\n",
            format!("Failed ({}):", summary.failed.len()).red().bold()
        );
        for e in &summary.failed {
            s.push_str(&format!(
                "  {} {} -> {} ({})\n",
                e.name, e.old_version, e.new_version, e.step
            ));
        }
        sections.push(s);
    }

    if !summary.skipped.is_empty() {
        let mut s = format!(
            "{}\n",
            format!("Skipped ({}):", summary.skipped.len())
                .yellow()
                .bold()
        );
        for e in &summary.skipped {
            s.push_str(&format!(
                "  {} {} -> {} ({})\n",
                e.name, e.old_version, e.new_version, e.reason
            ));
        }
        sections.push(s);
    }

    sections.join("\n")
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
    let mut summary = UpgradeSummary::default();

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
            summary.skipped.push(SkippedEntry {
                name: pkg.name.clone(),
                old_version: pkg.old_version.to_string(),
                new_version: pkg.new_version.to_string(),
                reason: "not found in any Cargo.toml",
            });
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
                summary.failed.push(FailedEntry {
                    name: pkg.name.clone(),
                    old_version: pkg.old_version.to_string(),
                    new_version: pkg.new_version.to_string(),
                    step: step_name,
                });
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
        summary.upgraded.push(UpgradedEntry {
            name: pkg.name.clone(),
            old_version: pkg.old_version.to_string(),
            new_version: pkg.new_version.to_string(),
        });
    }

    Ok(summary)
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

    let summary_text = render_summary(&summary);
    if !summary_text.is_empty() {
        println!("\n{}", summary_text);
    }

    println!(
        "{}",
        format!(
            "Done! {} upgraded, {} failed, {} skipped in {:.1}s",
            summary.upgraded.len(),
            summary.failed.len(),
            summary.skipped.len(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_upgraded(name: &str, old: &str, new: &str) -> UpgradedEntry {
        UpgradedEntry {
            name: name.into(),
            old_version: old.into(),
            new_version: new.into(),
        }
    }

    fn mk_failed(name: &str, old: &str, new: &str, step: &'static str) -> FailedEntry {
        FailedEntry {
            name: name.into(),
            old_version: old.into(),
            new_version: new.into(),
            step,
        }
    }

    fn mk_skipped(name: &str, old: &str, new: &str, reason: &'static str) -> SkippedEntry {
        SkippedEntry {
            name: name.into(),
            old_version: old.into(),
            new_version: new.into(),
            reason,
        }
    }

    fn disable_color() {
        // set_override is process-global; tests in this module never assert
        // on color codes, so permanently forcing it off is safe here.
        colored::control::set_override(false);
    }

    #[test]
    fn renders_all_three_sections() {
        disable_color();
        let summary = UpgradeSummary {
            upgraded: vec![
                mk_upgraded("clap", "4.6.0", "4.6.1"),
                mk_upgraded("semver", "1.0.27", "1.0.28"),
            ],
            failed: vec![mk_failed("serde", "1.0.210", "1.0.220", "cargo test")],
            skipped: vec![mk_skipped(
                "foo",
                "0.1.0",
                "0.2.0",
                "not found in any Cargo.toml",
            )],
        };
        let out = render_summary(&summary);
        assert!(out.contains("Upgraded (2):"), "output:\n{out}");
        assert!(out.contains("  clap 4.6.0 -> 4.6.1"), "output:\n{out}");
        assert!(out.contains("  semver 1.0.27 -> 1.0.28"), "output:\n{out}");
        assert!(out.contains("Failed (1):"), "output:\n{out}");
        assert!(
            out.contains("  serde 1.0.210 -> 1.0.220 (cargo test)"),
            "output:\n{out}"
        );
        assert!(out.contains("Skipped (1):"), "output:\n{out}");
        assert!(
            out.contains("  foo 0.1.0 -> 0.2.0 (not found in any Cargo.toml)"),
            "output:\n{out}"
        );
    }

    #[test]
    fn omits_empty_sections() {
        disable_color();
        let summary = UpgradeSummary {
            upgraded: vec![mk_upgraded("clap", "4.6.0", "4.6.1")],
            failed: vec![],
            skipped: vec![],
        };
        let out = render_summary(&summary);
        assert!(out.contains("Upgraded (1):"));
        assert!(!out.contains("Failed"), "should omit empty Failed section");
        assert!(
            !out.contains("Skipped"),
            "should omit empty Skipped section"
        );
    }

    #[test]
    fn empty_summary_renders_empty_string() {
        disable_color();
        let summary = UpgradeSummary::default();
        assert_eq!(render_summary(&summary), "");
    }

    #[test]
    fn entries_preserve_insertion_order() {
        disable_color();
        let summary = UpgradeSummary {
            upgraded: vec![
                mk_upgraded("b", "1.0.0", "1.0.1"),
                mk_upgraded("a", "2.0.0", "2.0.1"),
            ],
            failed: vec![],
            skipped: vec![],
        };
        let out = render_summary(&summary);
        let b_pos = out.find("  b 1.0.0").expect("b missing");
        let a_pos = out.find("  a 2.0.0").expect("a missing");
        assert!(b_pos < a_pos, "order should be preserved; output:\n{out}");
    }
}
