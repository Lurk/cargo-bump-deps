use cargo_bump_deps::git;
use cargo_bump_deps::manifest;
use std::env;
use std::fs;

mod common;
use common::*;

#[test]
fn add_and_commit_stages_and_commits_manifest_changes() {
    let _guard = lock_cwd();
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_workspace(tmp.path(), &[("serde", "1.0.0")]);
    init_git(tmp.path());

    let prev_cwd = env::current_dir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    let ws = manifest::load_workspace().unwrap();

    // Mutate the manifest.
    let manifest_path = tmp.path().join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("1.0.0", "1.0.1");
    fs::write(&manifest_path, content).unwrap();

    git::add_and_commit(&ws, "Upgrade serde 1.0.0 -> 1.0.1").unwrap();

    assert!(git_status_porcelain(tmp.path()).trim().is_empty());
    let log = git_log_oneline(tmp.path());
    assert!(log.contains("Upgrade serde 1.0.0 -> 1.0.1"));

    env::set_current_dir(prev_cwd).unwrap();
}

#[test]
fn restore_reverts_manifest_to_committed_state() {
    let _guard = lock_cwd();
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_workspace(tmp.path(), &[("serde", "1.0.0")]);
    init_git(tmp.path());

    let prev_cwd = env::current_dir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    let ws = manifest::load_workspace().unwrap();

    // Mutate the manifest without committing.
    let manifest_path = tmp.path().join("Cargo.toml");
    let original = fs::read_to_string(&manifest_path).unwrap();
    let modified = original.replace("1.0.0", "1.0.1");
    fs::write(&manifest_path, &modified).unwrap();

    git::restore(&ws).unwrap();

    let after = fs::read_to_string(&manifest_path).unwrap();
    assert_eq!(after, original);

    env::set_current_dir(prev_cwd).unwrap();
}
