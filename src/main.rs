mod check;
mod cli;
mod cluster;
mod config;
mod extract;
mod fingerprint;
mod gitsnap;
mod history;
mod index;
mod lang;
mod normalize;
mod report;
mod scan;
mod score;
mod style;
mod walk;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    let code = match run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("dupehound: {err:#}");
            2
        }
    };
    std::process::exit(code);
}

fn run(cli: cli::Cli) -> anyhow::Result<i32> {
    match cli.command {
        cli::Command::Scan(args) => scan::run(args),
        cli::Command::History(args) => history::run(args),
        cli::Command::Check(args) => check::run(args),
    }
}
