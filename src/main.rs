use cargo_bump_deps::cli::Cli;
use cargo_bump_deps::upgrade;
use clap::Parser;

fn main() {
    let Cli::BumpDeps(args) = Cli::parse();

    if let Err(e) = upgrade::run(args) {
        eprintln!("\n{}: {:#}", colored::Colorize::red("Error"), e);
        std::process::exit(1);
    }
}
