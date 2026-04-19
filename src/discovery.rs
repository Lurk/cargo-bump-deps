use anyhow::{Context, Result};
use semver::{Version, VersionReq};

use crate::parser::{self, OutdatedPackage};

/// Return the set of names that appear more than once, in the order first duplicated.
#[cfg(test)]
fn find_duplicates(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut duplicates: Vec<String> = Vec::new();
    for name in names {
        if !seen.insert(name.clone()) && !duplicates.contains(&name) {
            duplicates.push(name);
        }
    }
    duplicates
}

pub fn find_outdated_packages(
    compatible_only: bool,
    pre: bool,
    exclude: &[String],
    jobs: usize,
) -> Result<Vec<OutdatedPackage>> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to run cargo metadata")?;

    let mut seen = std::collections::HashSet::new();
    let mut duplicate_names: Vec<String> = Vec::new();
    let workspace_deps: Vec<_> = metadata
        .packages
        .iter()
        .filter(|pkg| metadata.workspace_members.contains(&pkg.id))
        .flat_map(|pkg| &pkg.dependencies)
        .filter(|dep| dep.path.is_none())
        .filter(|dep| !exclude.iter().any(|e| e == &dep.name))
        .filter(|dep| {
            if seen.insert(dep.name.clone()) {
                true
            } else {
                if !duplicate_names.contains(&dep.name) {
                    duplicate_names.push(dep.name.clone());
                }
                false
            }
        })
        .collect();

    if !duplicate_names.is_empty() {
        eprintln!(
            "\n{} duplicate dependency name(s) across workspace members — using first occurrence:",
            duplicate_names.len()
        );
        for name in &duplicate_names {
            eprintln!("  - {}", name);
        }
        eprintln!();
    }

    let total = workspace_deps.len();
    let concurrency = jobs.max(1);
    let mut outdated = Vec::new();
    let mut warnings = Vec::new();
    let mut progress = 0;

    let client =
        crates_io_api::SyncClient::new("cargo-bump-deps", std::time::Duration::from_millis(100))
            .context("Failed to create crates.io API client")?;

    for chunk in workspace_deps.chunks(concurrency) {
        let chunk_results = std::thread::scope(|s| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|dep| {
                    let name = dep.name.clone();
                    let req = dep.req.to_string();
                    let client = &client;
                    s.spawn(move || search_dep(client, &name, &req, compatible_only, pre))
                })
                .collect();

            handles
                .into_iter()
                .map(|h| {
                    h.join().map_err(|_| {
                        anyhow::anyhow!("Worker thread panicked during crates.io lookup")
                    })
                })
                .collect::<Result<Vec<_>>>()
        })?;

        for result in chunk_results {
            progress += 1;
            match result {
                Ok(Some(pkg)) => outdated.push(pkg),
                Ok(None) => {}
                Err(warning) => warnings.push(warning),
            }
        }
        eprint!("\rChecking {}/{}...", progress, total);
    }

    eprintln!(); // finish progress line

    // Print warning summary
    if !warnings.is_empty() {
        eprintln!("\n{} package(s) skipped during discovery:", warnings.len());
        for w in &warnings {
            eprintln!("  - {}", w);
        }
        eprintln!();
    }

    // Sort by name for deterministic output
    outdated.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(outdated)
}

/// Search for a single dependency. Returns:
/// - Ok(Some(pkg)) if outdated
/// - Ok(None) if up to date or filtered
/// - Err(warning) if something went wrong
fn search_dep(
    client: &crates_io_api::SyncClient,
    name: &str,
    req: &str,
    compatible_only: bool,
    pre: bool,
) -> std::result::Result<Option<OutdatedPackage>, String> {
    let crate_response = client
        .get_crate(name)
        .map_err(|e| format!("{}: crates.io lookup failed: {}", name, e))?;

    let latest_str = &crate_response.crate_data.max_version;

    let current_version_str = parser::version_from_req(req);
    let mut current_version = Version::parse(&current_version_str).map_err(|e| {
        format!(
            "{}: failed to parse current version '{}': {}",
            name, current_version_str, e
        )
    })?;
    let mut latest_version = Version::parse(latest_str).map_err(|e| {
        format!(
            "{}: failed to parse latest version '{}': {}",
            name, latest_str, e
        )
    })?;

    // Build metadata has no semver precedence and Cargo warns when it appears in
    // version requirements. Strip it once at the discovery boundary so downstream
    // code (manifest writes, commit messages, CLI output) stays clean.
    current_version.build = semver::BuildMetadata::EMPTY;
    latest_version.build = semver::BuildMetadata::EMPTY;

    if !pre && !latest_version.pre.is_empty() {
        return Ok(None);
    }

    if latest_version <= current_version {
        return Ok(None);
    }

    if compatible_only {
        let version_req = VersionReq::parse(req)
            .map_err(|e| format!("{}: failed to parse version req '{}': {}", name, req, e))?;
        if !version_req.matches(&latest_version) {
            return Ok(None);
        }
    }

    Ok(Some(OutdatedPackage {
        name: name.to_string(),
        old_version: current_version,
        new_version: latest_version,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulate the core comparison logic of search_dep without needing a crates.io client.
    fn search_dep_result(
        current_str: &str,
        latest_str: &str,
        compatible_only: bool,
        pre: bool,
    ) -> Option<OutdatedPackage> {
        let mut current_version = Version::parse(current_str).unwrap();
        let mut latest_version = Version::parse(latest_str).unwrap();

        // Mirror production: strip build metadata at the boundary.
        current_version.build = semver::BuildMetadata::EMPTY;
        latest_version.build = semver::BuildMetadata::EMPTY;

        if !pre && !latest_version.pre.is_empty() {
            return None;
        }

        if latest_version <= current_version {
            return None;
        }

        if compatible_only {
            let req = format!("^{}", current_str);
            let version_req = VersionReq::parse(&req).unwrap();
            if !version_req.matches(&latest_version) {
                return None;
            }
        }

        Some(OutdatedPackage {
            name: "test".to_string(),
            old_version: current_version,
            new_version: latest_version,
        })
    }

    #[test]
    fn build_metadata_only_difference_is_not_an_upgrade() {
        let current = Version::parse("1.0.6").unwrap();
        let latest = Version::parse("1.0.6+spec-1.1.0").unwrap();

        // Without stripping, semver considers 1.0.6+spec > 1.0.6
        assert!(latest > current);

        // After stripping build metadata, they should be equal
        let mut current_cmp = current.clone();
        current_cmp.build = semver::BuildMetadata::EMPTY;
        let mut latest_cmp = latest.clone();
        latest_cmp.build = semver::BuildMetadata::EMPTY;

        assert!(latest_cmp <= current_cmp);
    }

    #[test]
    fn prerelease_skipped_by_default() {
        let result = search_dep_result("0.9.0", "1.0.0-rc.1", false, false);
        assert!(
            result.is_none(),
            "prerelease should be skipped when pre=false"
        );
    }

    #[test]
    fn prerelease_included_when_flag_set() {
        let result = search_dep_result("0.9.0", "1.0.0-rc.1", false, true);
        assert!(
            result.is_some(),
            "prerelease should be included when pre=true"
        );
    }

    #[test]
    fn stable_upgrade_still_works_when_pre_disabled() {
        let result = search_dep_result("1.0.0", "1.1.0", false, false);
        assert!(
            result.is_some(),
            "stable upgrade should work when pre=false"
        );
    }

    #[test]
    fn real_upgrade_not_filtered_by_build_metadata_stripping() {
        let current = Version::parse("1.0.6").unwrap();
        let latest = Version::parse("1.0.7+build").unwrap();

        let mut current_cmp = current.clone();
        current_cmp.build = semver::BuildMetadata::EMPTY;
        let mut latest_cmp = latest.clone();
        latest_cmp.build = semver::BuildMetadata::EMPTY;

        assert!(latest_cmp > current_cmp);
    }

    #[test]
    fn find_duplicates_returns_names_seen_more_than_once() {
        let names = ["serde", "anyhow", "serde", "anyhow", "tokio"]
            .into_iter()
            .map(String::from);
        assert_eq!(
            find_duplicates(names),
            vec!["serde".to_string(), "anyhow".to_string()]
        );
    }

    #[test]
    fn find_duplicates_is_empty_when_all_unique() {
        let names = ["a", "b", "c"].into_iter().map(String::from);
        assert!(find_duplicates(names).is_empty());
    }

    #[test]
    fn outdated_package_strips_build_metadata_from_new_version() {
        let result = search_dep_result("1.0.0", "1.0.1+spec-1.1.0", false, false)
            .expect("1.0.1+spec-1.1.0 should be an upgrade from 1.0.0");

        assert!(
            result.new_version.build.is_empty(),
            "new_version.build should be empty, got `{}`",
            result.new_version
        );
        assert_eq!(
            result.new_version.to_string(),
            "1.0.1",
            "stringified new_version must not contain a `+...` suffix"
        );
    }
}
