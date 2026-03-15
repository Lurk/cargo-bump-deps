# cargo-bump-deps

A Cargo subcommand that upgrades dependencies one at a time, verifying each upgrade passes `cargo check`, `cargo test`, `cargo clippy`, and `cargo fmt --check` before committing. If an upgrade fails, it saves state so you can fix the issue and resume where you left off.

## Why

- **Lightweight, local-only** — no external service, no PRs, no bot noise. Just run it locally when you want.
- **Verified upgrades** — each dependency is checked (`check` + `test` + `clippy` + `fmt`) before committing, so a broken upgrade never sneaks in.
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

# Control parallel cargo search jobs during discovery (defaults to min(num_cpus, 8))
cargo bump-deps --jobs 4

# Skip specific checks
cargo bump-deps --no-clippy --no-fmt

# Keep failed changes in working tree instead of reverting
cargo bump-deps --no-revert-on-failure
```

## How it works

1. Uses `cargo metadata` to find direct dependencies (deduplicated across workspace members), then queries crates.io API to discover the latest version of each
2. For each outdated package:
   - Updates the version in `Cargo.toml` directly (supports string, inline table, and table formats across workspace root and members)
   - Runs `cargo check`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`
   - If all pass: stages only `Cargo.toml` and `Cargo.lock` files and commits with message `Upgrade <name> <old> -> <new>`
   - If any fail: reverts changes (`git checkout -- .`), saves state, and exits
3. To resume after fixing a failure, run `cargo bump-deps` again. Use `--reset --exclude <name>` to restart without a problematic package

## Flags

| Flag | Description |
|------|-------------|
| `--dry-run` | Show what would be upgraded without changing anything |
| `--compatible-only` | Only upgrade semver-compatible versions |
| `--reset` | Delete state file and start fresh |
| `--exclude <NAME>` | Exclude specific dependencies from upgrade (repeatable) |
| `--jobs <N>` | Number of parallel crates.io lookup jobs during discovery (default: min(num_cpus, 8)) |
| `--no-check` | Disable `cargo check` |
| `--no-test` | Disable `cargo test` |
| `--no-clippy` | Disable `cargo clippy` |
| `--no-fmt` | Disable `cargo fmt --check` |
| `--no-revert-on-failure` | Keep failed dependency changes in the working tree instead of reverting. Leaves uncommitted changes that block resume — you must manually commit or revert before running again |

## State file

On failure, a `cargo-bump-deps-state.json` file is created in the `target/` directory tracking which packages are done, failed, or pending. Running `cargo bump-deps` again resumes from the first failed/pending package. Use `cargo bump-deps --reset` to delete this file and start over.
