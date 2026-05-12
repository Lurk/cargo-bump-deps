use cargo_bump_deps::git;
use cargo_bump_deps::manifest;
use std::env;
use std::fs;
use std::process::Command;

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

#[test]
fn add_and_commit_skips_gitignored_lockfile() {
    let _guard = lock_cwd();
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_workspace(tmp.path(), &[("serde", "1.0.0")]);
    // Mark Cargo.lock as gitignored and write a fake lockfile BEFORE init_git
    // so the baseline commit excludes it. init_git runs `git add .` which honors
    // .gitignore automatically.
    fs::write(tmp.path().join(".gitignore"), "Cargo.lock\n").unwrap();
    fs::write(tmp.path().join("Cargo.lock"), "# fake lockfile\n").unwrap();
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

    // Lockfile content the test captures before the call, to assert it's untouched.
    let lockfile_path = tmp.path().join("Cargo.lock");
    let lockfile_before = fs::read_to_string(&lockfile_path).unwrap();

    git::add_and_commit(&ws, "Upgrade serde 1.0.0 -> 1.0.1").unwrap();

    // Working tree is clean. (Cargo.lock is ignored, so `status --porcelain`
    // omits it even if changed.)
    assert!(git_status_porcelain(tmp.path()).trim().is_empty());

    // Commit log contains our message.
    let log = git_log_oneline(tmp.path());
    assert!(log.contains("Upgrade serde 1.0.0 -> 1.0.1"));

    // The new commit's tree must NOT contain Cargo.lock.
    let files_in_head = Command::new("git")
        .current_dir(tmp.path())
        .args(["show", "--name-only", "--pretty=", "HEAD"])
        .output()
        .unwrap();
    let files = String::from_utf8(files_in_head.stdout).unwrap();
    assert!(
        !files.lines().any(|line| line == "Cargo.lock"),
        "HEAD commit unexpectedly contains Cargo.lock; files: {files:?}"
    );
    assert!(
        files.lines().any(|line| line == "Cargo.toml"),
        "HEAD commit missing Cargo.toml; files: {files:?}"
    );

    // On-disk lockfile is byte-identical.
    assert_eq!(fs::read_to_string(&lockfile_path).unwrap(), lockfile_before);

    env::set_current_dir(prev_cwd).unwrap();
}

#[test]
fn restore_skips_gitignored_lockfile() {
    let _guard = lock_cwd();
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_workspace(tmp.path(), &[("serde", "1.0.0")]);
    fs::write(tmp.path().join(".gitignore"), "Cargo.lock\n").unwrap();
    fs::write(tmp.path().join("Cargo.lock"), "# fake lockfile\n").unwrap();
    init_git(tmp.path());

    let prev_cwd = env::current_dir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    let ws = manifest::load_workspace().unwrap();

    // Mutate the manifest (uncommitted) and the lockfile.
    let manifest_path = tmp.path().join("Cargo.toml");
    let manifest_original = fs::read_to_string(&manifest_path).unwrap();
    let manifest_modified = manifest_original.replace("1.0.0", "1.0.1");
    fs::write(&manifest_path, &manifest_modified).unwrap();

    let lockfile_path = tmp.path().join("Cargo.lock");
    let lockfile_modified = "# changed lockfile\n";
    fs::write(&lockfile_path, lockfile_modified).unwrap();

    git::restore(&ws).unwrap();

    // Manifest reverts to its committed content.
    assert_eq!(
        fs::read_to_string(&manifest_path).unwrap(),
        manifest_original
    );

    // Lockfile is left as-is on disk: not reverted (it was never committed)
    // and not deleted.
    assert_eq!(
        fs::read_to_string(&lockfile_path).unwrap(),
        lockfile_modified
    );

    env::set_current_dir(prev_cwd).unwrap();
}
