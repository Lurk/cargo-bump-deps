use clap::Parser;

#[derive(Parser)]
#[command(bin_name = "cargo")]
pub enum Cli {
    /// Upgrade Cargo dependencies one at a time with verification
    Deps(DepsArgs),
}

#[derive(Parser)]
pub struct DepsArgs {
    /// Delete state file and start fresh
    #[arg(long)]
    pub reset: bool,

    /// Show what would be upgraded without changing anything
    #[arg(long)]
    pub dry_run: bool,

    /// Only upgrade semver-compatible versions
    #[arg(long)]
    pub compatible_only: bool,

    /// Exclude specific dependencies from upgrade (repeatable)
    #[arg(long, value_name = "NAME")]
    pub exclude: Vec<String>,

    /// Skip a stuck package and continue with the rest
    #[arg(long, value_name = "NAME")]
    pub skip: Option<String>,

    /// Number of parallel cargo search jobs during discovery
    #[arg(long, default_value_t = default_jobs(), value_name = "N")]
    pub jobs: usize,
}

fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4)
}
