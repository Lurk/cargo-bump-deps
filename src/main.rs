mod cli;
mod discovery;
mod parser;
mod runner;
mod state;
mod upgrade;

use clap::Parser;
use cli::Cli;

fn main() {
    let Cli::BumpDeps(args) = Cli::parse();

    if let Err(e) = upgrade::run(args) {
        eprintln!("\n{}: {:#}", colored::Colorize::red("Error"), e);
        std::process::exit(1);
    }
}
