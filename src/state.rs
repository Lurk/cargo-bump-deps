use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::parser::OutdatedPackage;

fn state_file_path() -> PathBuf {
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    PathBuf::from(target_dir).join("cargo-bump-deps-state.json")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PackageStatus {
    Pending,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageEntry {
    pub name: String,
    pub old_version: String,
    pub new_version: String,
    pub status: PackageStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub packages: Vec<PackageEntry>,
}

impl State {
    pub fn from_packages(packages: Vec<OutdatedPackage>) -> Self {
        State {
            packages: packages
                .into_iter()
                .map(|p| PackageEntry {
                    name: p.name,
                    old_version: p.old_version.to_string(),
                    new_version: p.new_version.to_string(),
                    status: PackageStatus::Pending,
                })
                .collect(),
        }
    }

    pub fn resume_index(&self) -> usize {
        self.packages
            .iter()
            .position(|p| p.status == PackageStatus::Failed || p.status == PackageStatus::Pending)
            .unwrap_or(self.packages.len())
    }

    pub fn skip_package(&mut self, name: &str) -> bool {
        for pkg in &mut self.packages {
            if pkg.name == name && pkg.status != PackageStatus::Done {
                pkg.status = PackageStatus::Skipped;
                return true;
            }
        }
        false
    }

}

pub fn load_state() -> Result<Option<State>> {
    let path = state_file_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).context("Failed to read state file")?;
    let state: State = serde_json::from_str(&content).context("Failed to parse state file")?;
    Ok(Some(state))
}

pub fn save_state(state: &State) -> Result<()> {
    let path = state_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create target directory")?;
    }
    let content = serde_json::to_string_pretty(state)?;
    fs::write(&path, content).context("Failed to write state file")?;
    Ok(())
}

pub fn delete_state() -> Result<()> {
    let path = state_file_path();
    if path.exists() {
        fs::remove_file(&path).context("Failed to delete state file")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str, status: PackageStatus) -> PackageEntry {
        PackageEntry {
            name: name.to_string(),
            old_version: "1.0.0".to_string(),
            new_version: "2.0.0".to_string(),
            status,
        }
    }

    #[test]
    fn test_resume_index_all_pending() {
        let state = State {
            packages: vec![
                make_entry("a", PackageStatus::Pending),
                make_entry("b", PackageStatus::Pending),
            ],
        };
        assert_eq!(state.resume_index(), 0);
    }

    #[test]
    fn test_resume_index_some_done() {
        let state = State {
            packages: vec![
                make_entry("a", PackageStatus::Done),
                make_entry("b", PackageStatus::Done),
                make_entry("c", PackageStatus::Pending),
            ],
        };
        assert_eq!(state.resume_index(), 2);
    }

    #[test]
    fn test_resume_index_failed() {
        let state = State {
            packages: vec![
                make_entry("a", PackageStatus::Done),
                make_entry("b", PackageStatus::Failed),
                make_entry("c", PackageStatus::Pending),
            ],
        };
        assert_eq!(state.resume_index(), 1);
    }

    #[test]
    fn test_resume_index_all_done() {
        let state = State {
            packages: vec![
                make_entry("a", PackageStatus::Done),
                make_entry("b", PackageStatus::Done),
            ],
        };
        assert_eq!(state.resume_index(), 2);
    }

    #[test]
    fn test_resume_index_skipped_are_skipped() {
        let state = State {
            packages: vec![
                make_entry("a", PackageStatus::Done),
                make_entry("b", PackageStatus::Skipped),
                make_entry("c", PackageStatus::Pending),
            ],
        };
        assert_eq!(state.resume_index(), 2);
    }

    #[test]
    fn test_skip_package() {
        let mut state = State {
            packages: vec![
                make_entry("a", PackageStatus::Done),
                make_entry("b", PackageStatus::Failed),
                make_entry("c", PackageStatus::Pending),
            ],
        };
        assert!(state.skip_package("b"));
        assert_eq!(state.packages[1].status, PackageStatus::Skipped);
    }

    #[test]
    fn test_skip_package_not_found() {
        let mut state = State {
            packages: vec![make_entry("a", PackageStatus::Pending)],
        };
        assert!(!state.skip_package("nonexistent"));
    }

    #[test]
    fn test_skip_package_already_done() {
        let mut state = State {
            packages: vec![make_entry("a", PackageStatus::Done)],
        };
        assert!(!state.skip_package("a"));
    }

    #[test]
    fn test_state_serialization_roundtrip() {
        let state = State {
            packages: vec![
                make_entry("a", PackageStatus::Done),
                make_entry("b", PackageStatus::Failed),
                make_entry("c", PackageStatus::Pending),
                make_entry("d", PackageStatus::Skipped),
            ],
        };
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: State = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.packages.len(), 4);
        assert_eq!(deserialized.packages[0].status, PackageStatus::Done);
        assert_eq!(deserialized.packages[1].status, PackageStatus::Failed);
        assert_eq!(deserialized.packages[2].status, PackageStatus::Pending);
        assert_eq!(deserialized.packages[3].status, PackageStatus::Skipped);
    }
}
