mod cli;
mod discovery;
mod parser;
mod runner;
mod state;
mod upgrade;

use clap::Parser;
use cli::Cli;

fn main() {
    let Cli::Deps(args) = Cli::parse();

    if let Err(e) = upgrade::run(
        args.dry_run,
        args.reset,
        args.compatible_only,
        args.exclude,
        args.skip,
        args.jobs,
    ) {
        eprintln!("\n{}: {:#}", colored::Colorize::red("Error"), e);
        std::process::exit(1);
    }
}
