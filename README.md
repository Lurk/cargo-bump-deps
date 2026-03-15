# cargo-deps

A Cargo subcommand that upgrades dependencies one at a time, verifying each upgrade passes `cargo check`, `cargo test`, and `cargo clippy` before committing. If an upgrade fails, it saves state so you can fix the issue and resume where you left off.

## Prerequisites

- A git repository with a clean working tree

## Installation

```sh
cargo install --path .
```

## Usage

```sh
# Show what would be upgraded
cargo deps --dry-run

# Upgrade all dependencies (compatible + incompatible)
cargo deps

# Only upgrade semver-compatible versions
cargo deps --compatible-only

# Clear saved state and start fresh
cargo deps --reset
```

## How it works

1. Uses `cargo metadata` to find direct dependencies, then queries `cargo search` to discover the latest version of each
2. For each outdated package:
   - Runs `cargo add <name>@<new_version>`
   - Runs `cargo check`, `cargo test`, `cargo clippy -- -D warnings`
   - If all pass: commits with message `Upgrade <name> <old> -> <new>`
   - If any fail: saves state to `cargo-deps-state.json` and exits
3. To resume after fixing a failure, run `cargo deps` again

## State file

On failure, a `cargo-deps-state.json` file is created in the project root tracking which packages are done, failed, or pending. Running `cargo deps` again resumes from the first failed/pending package. Use `cargo deps --reset` to delete this file and start over.
