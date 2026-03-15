use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn run_command_inherit(program: &str, args: &[&str]) -> Result<bool> {
    let status = Command::new(program)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute: {} {}", program, args.join(" ")))?;

    Ok(status.success())
}

pub fn update_dependency_in_workspace(name: &str, version: &str) -> Result<bool> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to run cargo metadata")?;

    let mut manifest_paths: Vec<_> = metadata
        .packages
        .iter()
        .filter(|pkg| metadata.workspace_members.contains(&pkg.id))
        .map(|pkg| pkg.manifest_path.as_std_path().to_path_buf())
        .collect();

    // Also include workspace root Cargo.toml for [workspace.dependencies]
    let root_manifest = metadata.workspace_root.as_std_path().join("Cargo.toml");
    if !manifest_paths.iter().any(|p| p == &root_manifest) {
        manifest_paths.push(root_manifest);
    }

    let mut updated = false;
    for manifest_path in &manifest_paths {
        if update_dependency_version(manifest_path, name, version)? {
            updated = true;
        }
    }

    Ok(updated)
}

fn update_dependency_version(manifest_path: &Path, name: &str, version: &str) -> Result<bool> {
    let content = fs::read_to_string(manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    let mut updated = false;

    let dep_tables = ["dependencies", "dev-dependencies", "build-dependencies"];
    for table_name in &dep_tables {
        if let Some(table) = doc.get_mut(table_name)
            && update_dep_in_table(table, name, version)
        {
            updated = true;
        }
    }

    // Check [workspace.dependencies]
    if let Some(workspace) = doc.get_mut("workspace")
        && let Some(deps) = workspace.get_mut("dependencies")
        && update_dep_in_table(deps, name, version)
    {
        updated = true;
    }

    if updated {
        fs::write(manifest_path, doc.to_string())
            .with_context(|| format!("Failed to write {}", manifest_path.display()))?;
    }

    Ok(updated)
}

fn update_dep_in_table(table: &mut toml_edit::Item, name: &str, version: &str) -> bool {
    let Some(table) = table.as_table_like_mut() else {
        return false;
    };
    let Some(dep) = table.get_mut(name) else {
        return false;
    };

    if let Some(value) = dep.as_value_mut() {
        if value.is_str() {
            // name = "version"
            *value = version.into();
            return true;
        }
        if let Some(inline_table) = value.as_inline_table_mut() {
            // name = { version = "...", ... }
            if let Some(ver) = inline_table.get_mut("version") {
                *ver = version.into();
                return true;
            }
        }
        return false;
    }

    if let Some(table) = dep.as_table_mut() {
        // [dependencies.name]
        // version = "..."
        if let Some(ver) = table.get_mut("version") {
            *ver = toml_edit::value(version);
            return true;
        }
    }

    false
}

pub fn cargo_check() -> Result<bool> {
    run_command_inherit("cargo", &["check"])
}

pub fn cargo_test() -> Result<bool> {
    run_command_inherit("cargo", &["test"])
}

pub fn cargo_clippy() -> Result<bool> {
    run_command_inherit("cargo", &["clippy", "--", "-D", "warnings"])
}

pub fn cargo_fmt() -> Result<bool> {
    run_command_inherit("cargo", &["fmt", "--check"])
}

pub fn git_add_and_commit(message: &str) -> Result<bool> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to run cargo metadata")?;

    let mut paths_to_stage: Vec<String> = metadata
        .packages
        .iter()
        .filter(|pkg| metadata.workspace_members.contains(&pkg.id))
        .map(|pkg| {
            pkg.manifest_path
                .as_std_path()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    // Add workspace root Cargo.toml
    let root_manifest = metadata
        .workspace_root
        .as_std_path()
        .join("Cargo.toml")
        .to_string_lossy()
        .into_owned();
    if !paths_to_stage.contains(&root_manifest) {
        paths_to_stage.push(root_manifest);
    }

    // Add Cargo.lock
    let lock_file = metadata
        .workspace_root
        .as_std_path()
        .join("Cargo.lock")
        .to_string_lossy()
        .into_owned();
    paths_to_stage.push(lock_file);

    let path_refs: Vec<&str> = paths_to_stage.iter().map(|s| s.as_str()).collect();
    let mut add_args = vec!["add", "--"];
    add_args.extend(path_refs.iter());

    let add_ok = run_command_inherit("git", &add_args)?;
    if !add_ok {
        return Ok(false);
    }
    run_command_inherit("git", &["commit", "-m", message])
}

pub fn git_restore() -> Result<bool> {
    run_command_inherit("git", &["checkout", "--", "."])
}

pub fn check_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn check_git_clean() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map(|o| o.status.success() && o.stdout.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_update_dep_string_format() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        let mut f = fs::File::create(&manifest).unwrap();
        writeln!(
            f,
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1.0.0\""
        )
        .unwrap();

        let updated = update_dependency_version(&manifest, "serde", "2.0.0").unwrap();
        assert!(updated);

        let content = fs::read_to_string(&manifest).unwrap();
        assert!(content.contains("serde = \"2.0.0\""));
    }

    #[test]
    fn test_update_dep_table_format() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        let mut f = fs::File::create(&manifest).unwrap();
        writeln!(f, "[package]\nname = \"test\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = {{ version = \"1.0.0\", features = [\"derive\"] }}").unwrap();

        let updated = update_dependency_version(&manifest, "serde", "2.0.0").unwrap();
        assert!(updated);

        let content = fs::read_to_string(&manifest).unwrap();
        assert!(content.contains("\"2.0.0\""));
        assert!(content.contains("features"));
    }

    #[test]
    fn test_update_dep_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        let mut f = fs::File::create(&manifest).unwrap();
        writeln!(
            f,
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1.0.0\""
        )
        .unwrap();

        let updated = update_dependency_version(&manifest, "tokio", "2.0.0").unwrap();
        assert!(!updated);
    }

    #[test]
    fn test_update_dep_workspace_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        let mut f = fs::File::create(&manifest).unwrap();
        writeln!(
            f,
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.dependencies]\nserde = \"1.0.0\""
        )
        .unwrap();

        let updated = update_dependency_version(&manifest, "serde", "2.0.0").unwrap();
        assert!(updated);

        let content = fs::read_to_string(&manifest).unwrap();
        assert!(content.contains("serde = \"2.0.0\""));
    }
}
