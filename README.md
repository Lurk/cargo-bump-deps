# cargo-bump-deps

A Cargo subcommand that upgrades dependencies one at a time, verifying each upgrade passes `cargo check`, `cargo test`, and `cargo clippy` before committing. If an upgrade fails, it saves state so you can fix the issue and resume where you left off.

## Why

- **Lightweight, local-only** — no external service, no PRs, no bot noise. Just run it locally when you want.
- **Verified upgrades** — each dependency is checked (`check` + `test` + `clippy`) before committing, so a broken upgrade never sneaks in.
- **Clean git history** — one commit per dependency upgrade, making it easy to bisect or revert.

If Renovate or Dependabot feel like overkill for your project, this is the simpler alternative.

## Prerequisites

- A git repository with a clean working tree

## Installation

```sh
cargo install --path .
```

## Usage

```sh
# Show what would be upgraded
cargo bump-deps --dry-run

# Upgrade all dependencies (compatible + incompatible)
cargo bump-deps

# Only upgrade semver-compatible versions
cargo bump-deps --compatible-only

# Clear saved state and start fresh
cargo bump-deps --reset

# Exclude specific dependencies (repeatable)
cargo bump-deps --exclude serde --exclude tokio

# Skip a stuck package and continue with the rest
cargo bump-deps --skip regex

# Control parallel cargo search jobs during discovery (defaults to min(num_cpus, 8))
cargo bump-deps --jobs 4
```

## How it works

1. Uses `cargo metadata` to find direct dependencies, then queries `cargo search` to discover the latest version of each
2. For each outdated package:
   - Runs `cargo add <name>@<new_version>`
   - Runs `cargo check`, `cargo test`, `cargo clippy -- -D warnings`
   - If all pass: commits with message `Upgrade <name> <old> -> <new>`
   - If any fail: saves state to `cargo-bump-deps-state.json` and exits
3. To resume after fixing a failure, run `cargo bump-deps` again

## State file

On failure, a `cargo-bump-deps-state.json` file is created in the project root tracking which packages are done, failed, or pending. Running `cargo bump-deps` again resumes from the first failed/pending package. Use `cargo bump-deps --reset` to delete this file and start over.
