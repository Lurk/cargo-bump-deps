use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Workspace {
    pub manifest_paths: Vec<PathBuf>,
    pub root: PathBuf,
}

/// Enumerate all manifests that should be edited during an upgrade: workspace
/// members plus the workspace root (for `[workspace.dependencies]`). The root
/// is added only if it isn't already a member, so a single-crate repo doesn't
/// duplicate its root manifest in the list.
pub fn load_workspace() -> Result<Workspace> {
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

    let root = metadata.workspace_root.as_std_path().to_path_buf();
    let root_manifest = root.join("Cargo.toml");
    if !manifest_paths.iter().any(|p| p == &root_manifest) {
        manifest_paths.push(root_manifest);
    }

    Ok(Workspace {
        manifest_paths,
        root,
    })
}

pub fn update_dependency_in_workspace(
    workspace: &Workspace,
    name: &str,
    version: &str,
) -> Result<bool> {
    let mut updated = false;
    for manifest_path in &workspace.manifest_paths {
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

    #[test]
    fn test_update_dep_full_header_table() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        let mut f = fs::File::create(&manifest).unwrap();
        writeln!(
            f,
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n\n[dependencies.serde]\nversion = \"1.0.0\"\nfeatures = [\"derive\"]"
        )
        .unwrap();

        let updated = update_dependency_version(&manifest, "serde", "2.0.0").unwrap();
        assert!(updated);

        let content = fs::read_to_string(&manifest).unwrap();
        assert!(content.contains("version = \"2.0.0\""));
        assert!(content.contains("features = [\"derive\"]"));
    }
}
