# cargo-bump-deps

A Cargo subcommand that upgrades dependencies one at a time, verifying each upgrade passes `cargo check`, `cargo test`, `cargo clippy`, and `cargo fmt --check` before committing. If an upgrade fails, it reverts the change and stops (or continues with `--continue-on-failure`). Since each upgrade is committed individually, re-running picks up where you left off.

## Why

- **Lightweight, local-only** — no external service, no PRs, no bot noise. Just run it locally when you want.
- **Verified upgrades** — each dependency is checked (`check` + `test` + `clippy` + `fmt`) before committing, so a broken upgrade never sneaks in.
- **Clean git history** — one commit per dependency upgrade, making it easy to bisect or revert.

If Renovate or Dependabot feel like overkill for your project, this is the simpler alternative.

## Prerequisites

- A git repository with a clean working tree

## Installation

```sh
cargo install --git https://github.com/Lurk/cargo-bump-deps.git
```

## Usage

```sh
# Show what would be upgraded
cargo bump-deps --dry-run

# Upgrade all dependencies (compatible + incompatible)
cargo bump-deps

# Only upgrade semver-compatible versions
cargo bump-deps --compatible-only

# Include prerelease versions
cargo bump-deps --pre

# Exclude specific dependencies (repeatable)
cargo bump-deps --exclude serde --exclude tokio

# Control parallel cargo search jobs during discovery (defaults to min(num_cpus, 8))
cargo bump-deps --jobs 4

# Skip specific checks
cargo bump-deps --no-clippy --no-fmt

# Skip failed dependencies and continue upgrading the rest
cargo bump-deps --continue-on-failure

# Keep failed changes in working tree instead of reverting
cargo bump-deps --no-revert-on-failure
```

## How it works

1. Uses `cargo metadata` to find direct dependencies (deduplicated across workspace members), then queries crates.io API to discover the latest version of each
2. For each outdated package:
   - Updates the version in `Cargo.toml` directly (supports string, inline table, and table formats across workspace root and members)
   - Runs `cargo check`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`
   - If all pass: stages only `Cargo.toml` and `Cargo.lock` files and commits with message `Upgrade <name> <old> -> <new>`
   - If any fail: reverts `Cargo.toml` and `Cargo.lock` changes and exits (or continues to the next package with `--continue-on-failure`)
3. Since each successful upgrade is committed immediately, re-running after a failure automatically skips already-upgraded dependencies

## Flags

| Flag | Description |
|------|-------------|
| `--dry-run` | Show what would be upgraded without changing anything |
| `--compatible-only` | Only upgrade semver-compatible versions |
| `--pre` | Include prerelease versions in upgrade candidates |
| `--exclude <NAME>` | Exclude specific dependencies from upgrade (repeatable) |
| `--jobs <N>` | Number of parallel crates.io lookup jobs during discovery (default: min(num_cpus, 8)) |
| `--no-check` | Disable `cargo check` |
| `--no-test` | Disable `cargo test` |
| `--no-clippy` | Disable `cargo clippy` |
| `--no-fmt` | Disable `cargo fmt --check` |
| `--continue-on-failure` | Skip failed dependencies and continue upgrading the rest |
| `--no-revert-on-failure` | Keep failed dependency changes in the working tree instead of reverting. Implies `--continue-on-failure` |
