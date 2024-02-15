use anyhow::Result;
use clap::{Parser, Subcommand};

use cargo_scaffold::{Opts, ScaffoldDescription};

#[derive(Parser)]
#[command(author, version, about, long_about=None)]
struct Cli {
    #[command(subcommand)]
    command: ScaffoldCommand,
}

#[derive(Subcommand)]
enum ScaffoldCommand {
    Scaffold(Opts),
}

fn main() -> Result<()> {
    let opts = Cli::parse();
    match opts.command {
        ScaffoldCommand::Scaffold(opts) => ScaffoldDescription::new(opts)?.scaffold(),
    }
}
