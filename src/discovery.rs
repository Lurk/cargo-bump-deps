use anyhow::{bail, Result};
use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::parser::{self, OutdatedPackage};
use crate::runner;

#[derive(Deserialize)]
struct MetadataOutput {
    packages: Vec<MetadataPackage>,
    workspace_members: Vec<String>,
}

#[derive(Deserialize)]
struct MetadataPackage {
    id: String,
    #[allow(dead_code)]
    name: String,
    dependencies: Vec<MetadataDep>,
}

#[derive(Deserialize)]
struct MetadataDep {
    name: String,
    req: String,
    #[allow(dead_code)]
    kind: Option<String>,
    path: Option<String>,
}

pub fn find_outdated_packages(
    compatible_only: bool,
    exclude: &[String],
    jobs: usize,
) -> Result<Vec<OutdatedPackage>> {
    let result = runner::cargo_metadata()?;
    if !result.success {
        bail!("cargo metadata failed: {}", result.stderr);
    }

    let metadata: MetadataOutput = serde_json::from_str(&result.stdout)?;

    let workspace_deps: Vec<&MetadataDep> = metadata
        .packages
        .iter()
        .filter(|pkg| {
            metadata
                .workspace_members
                .iter()
                .any(|wm| wm.starts_with(&format!("{} ", pkg.name)) || wm.contains(&pkg.id))
        })
        .flat_map(|pkg| &pkg.dependencies)
        .filter(|dep| dep.path.is_none())
        .filter(|dep| !exclude.iter().any(|e| e == &dep.name))
        .collect();

    let total = workspace_deps.len();
    let concurrency = jobs.max(1);
    let mut outdated = Vec::new();
    let mut warnings = Vec::new();
    let mut progress = 0;

    for chunk in workspace_deps.chunks(concurrency) {
        let chunk_results: Vec<_> = std::thread::scope(|s| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|dep| {
                    let name = dep.name.clone();
                    let req = dep.req.clone();
                    s.spawn(move || search_dep(&name, &req, compatible_only))
                })
                .collect();

            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect()
        });

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
        eprintln!(
            "\n{} package(s) skipped during discovery:",
            warnings.len()
        );
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
    name: &str,
    req: &str,
    compatible_only: bool,
) -> std::result::Result<Option<OutdatedPackage>, String> {
    let search_result = runner::cargo_search(name)
        .map_err(|e| format!("{}: cargo search failed: {}", name, e))?;

    let latest_str = parser::parse_cargo_search_output(&search_result.stdout)
        .ok_or_else(|| format!("{}: could not parse search output", name))?;

    // Verify the search result matches the dep name
    let first_word = search_result
        .stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("");
    if first_word != name {
        return Err(format!(
            "{}: search returned different package '{}'",
            name, first_word
        ));
    }

    let current_version_str = parser::version_from_req(req);
    let current_version = Version::parse(&current_version_str)
        .map_err(|e| format!("{}: failed to parse current version '{}': {}", name, current_version_str, e))?;
    let latest_version = Version::parse(&latest_str)
        .map_err(|e| format!("{}: failed to parse latest version '{}': {}", name, latest_str, e))?;

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
