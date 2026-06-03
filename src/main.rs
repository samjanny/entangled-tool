//! entangled-tool: publisher command-line tooling for the Entangled v1.0
//! protocol, built on the `entangled-core` library.

mod cli;
mod commands;
mod markdown;

use clap::Parser;
use cli::{Cli, Command};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Keygen(args) => commands::keygen::run(args),
        Command::Build(args) => commands::build::run(args),
        Command::Verify(args) => commands::verify::run(args),
        Command::Init(args) => commands::init::run(args),
        Command::Content(args) => commands::content::run(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
