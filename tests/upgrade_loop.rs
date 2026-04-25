use cargo_bump_deps::cli::DepsArgs;
use cargo_bump_deps::parser::OutdatedPackage;
use cargo_bump_deps::upgrade::{CheckStep, run_upgrade_loop};
use semver::Version;
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;
use common::*;

fn base_args() -> DepsArgs {
    DepsArgs {
        dry_run: false,
        compatible_only: false,
        exclude: vec![],
        jobs: 1,
        no_check: true,
        no_test: true,
        no_clippy: true,
        no_fmt: true,
        pre: false,
        continue_on_failure: false,
        no_revert_on_failure: false,
    }
}

fn pkg(name: &str, old: &str, new: &str) -> OutdatedPackage {
    OutdatedPackage {
        name: name.to_string(),
        old_version: Version::parse(old).unwrap(),
        new_version: Version::parse(new).unwrap(),
    }
}

fn ok_step() -> CheckStep {
    fn run() -> anyhow::Result<()> {
        Ok(())
    }
    CheckStep {
        name: "fake_ok",
        label: "fake ok",
        skip: false,
        run,
    }
}

fn fail_step() -> CheckStep {
    fn run() -> anyhow::Result<()> {
        anyhow::bail!("fake failure")
    }
    CheckStep {
        name: "fake_fail",
        label: "fake fail",
        skip: false,
        run,
    }
}

#[test]
fn happy_path_commits_and_updates_manifest() {
    let _guard = lock_cwd();
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_workspace(tmp.path(), &[("serde", "1.0.0")]);
    init_git(tmp.path());

    let prev_cwd = env::current_dir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    let ws = load_fixture_workspace();
    let steps = [ok_step()];
    let packages = vec![pkg("serde", "1.0.0", "1.0.1")];
    let summary = run_upgrade_loop(&ws, &packages, &base_args(), &steps).unwrap();

    assert_eq!(summary.upgraded.len(), 1);
    assert_eq!(summary.upgraded[0].name, "serde");
    assert_eq!(summary.upgraded[0].old_version, "1.0.0");
    assert_eq!(summary.upgraded[0].new_version, "1.0.1");
    assert!(summary.skipped.is_empty());
    assert!(summary.failed.is_empty());

    let manifest = read_manifest(tmp.path());
    assert!(manifest.contains("serde = \"1.0.1\""));
    let log = git_log_oneline(tmp.path());
    assert!(log.contains("Upgrade serde 1.0.0 -> 1.0.1"));

    env::set_current_dir(prev_cwd).unwrap();
}

#[test]
fn dep_not_in_manifest_is_skipped() {
    let _guard = lock_cwd();
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_workspace(tmp.path(), &[("serde", "1.0.0")]);
    init_git(tmp.path());

    let prev_cwd = env::current_dir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    let ws = load_fixture_workspace();
    let steps = [ok_step()];
    let packages = vec![pkg("tokio", "1.0.0", "1.0.1")];
    let summary = run_upgrade_loop(&ws, &packages, &base_args(), &steps).unwrap();

    assert!(summary.upgraded.is_empty());
    assert_eq!(summary.skipped.len(), 1);
    assert_eq!(summary.skipped[0].name, "tokio");
    assert_eq!(summary.skipped[0].reason, "not found in any Cargo.toml");
    assert!(summary.failed.is_empty());

    env::set_current_dir(prev_cwd).unwrap();
}

#[test]
fn check_failure_reverts_and_fails_fast() {
    let _guard = lock_cwd();
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_workspace(tmp.path(), &[("serde", "1.0.0")]);
    init_git(tmp.path());

    let prev_cwd = env::current_dir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    let ws = load_fixture_workspace();
    let steps = [fail_step()];
    let packages = vec![pkg("serde", "1.0.0", "1.0.1")];
    let err = run_upgrade_loop(&ws, &packages, &base_args(), &steps).unwrap_err();
    assert!(format!("{:#}", err).contains("fake_fail"));

    assert!(git_status_porcelain(tmp.path()).trim().is_empty());
    let manifest = read_manifest(tmp.path());
    assert!(manifest.contains("serde = \"1.0.0\""));

    env::set_current_dir(prev_cwd).unwrap();
}

#[test]
fn no_revert_on_failure_leaves_manifest_dirty() {
    let _guard = lock_cwd();
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_workspace(tmp.path(), &[("serde", "1.0.0")]);
    init_git(tmp.path());

    let prev_cwd = env::current_dir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    let ws = load_fixture_workspace();
    let steps = [fail_step()];
    let packages = vec![pkg("serde", "1.0.0", "1.0.1")];

    let mut args = base_args();
    args.no_revert_on_failure = true;
    let summary = run_upgrade_loop(&ws, &packages, &args, &steps).unwrap();

    assert!(summary.upgraded.is_empty());
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.failed[0].name, "serde");
    assert_eq!(summary.failed[0].step, "fake_fail");
    let manifest = read_manifest(tmp.path());
    assert!(manifest.contains("serde = \"1.0.1\""));

    env::set_current_dir(prev_cwd).unwrap();
}

#[test]
fn continue_on_failure_processes_remaining_packages() {
    let _guard = lock_cwd();
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_workspace(tmp.path(), &[("serde", "1.0.0"), ("anyhow", "1.0.0")]);
    init_git(tmp.path());

    let prev_cwd = env::current_dir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    // fn items can't close over state, so we thread through a static counter.
    // CWD_LOCK serializes tests, so this static isn't observed by concurrent runs.
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn sometimes_fail() -> anyhow::Result<()> {
        let n = CALLS.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            anyhow::bail!("first-only failure")
        } else {
            Ok(())
        }
    }
    CALLS.store(0, Ordering::SeqCst);

    let ws = load_fixture_workspace();
    let steps = [CheckStep {
        name: "fake_sometimes",
        label: "fake sometimes",
        skip: false,
        run: sometimes_fail,
    }];
    let packages = vec![
        pkg("serde", "1.0.0", "1.0.1"),
        pkg("anyhow", "1.0.0", "1.0.1"),
    ];

    let mut args = base_args();
    args.continue_on_failure = true;

    let summary = run_upgrade_loop(&ws, &packages, &args, &steps).unwrap();

    assert_eq!(summary.upgraded.len(), 1);
    assert_eq!(summary.upgraded[0].name, "anyhow");
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.failed[0].name, "serde");
    assert_eq!(summary.failed[0].step, "fake_sometimes");

    let manifest = read_manifest(tmp.path());
    assert!(manifest.contains("serde = \"1.0.0\""));
    assert!(manifest.contains("anyhow = \"1.0.1\""));

    env::set_current_dir(prev_cwd).unwrap();
}
