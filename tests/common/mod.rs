use cargo_bump_deps::manifest::Workspace;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

// Integration tests chdir into a tempdir; serialize them so cargo_metadata and
// git always see the expected cwd.
pub static CWD_LOCK: Mutex<()> = Mutex::new(());

pub fn lock_cwd() -> std::sync::MutexGuard<'static, ()> {
    CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn write_fixture_workspace(dir: &Path, deps: &[(&str, &str)]) {
    let mut toml = String::from(
        "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n",
    );
    for (name, version) in deps {
        toml.push_str(&format!("{} = \"{}\"\n", name, version));
    }
    std::fs::write(dir.join("Cargo.toml"), toml).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("lib.rs"), "").unwrap();
}

pub fn init_git(dir: &Path) {
    run_git(dir, &["init", "--quiet", "--initial-branch=main"]);
    run_git(dir, &["config", "user.email", "test@example.com"]);
    run_git(dir, &["config", "user.name", "Test"]);
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "--quiet", "-m", "baseline"]);
}

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .expect("git command failed to spawn");
    assert!(status.success(), "git {:?} failed", args);
}

#[allow(dead_code)]
pub fn load_fixture_workspace() -> Workspace {
    // load_workspace reads cwd; the caller must have chdir'd into the fixture dir.
    cargo_bump_deps::manifest::load_workspace().expect("load_workspace failed")
}

#[allow(dead_code)]
pub fn read_manifest(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("Cargo.toml")).unwrap()
}

pub fn git_log_oneline(dir: &Path) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["log", "--oneline"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

pub fn git_status_porcelain(dir: &Path) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}
