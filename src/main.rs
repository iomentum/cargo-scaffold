use anyhow::Result;
use clap::Parser;

use cargo_scaffold::{Opts, ScaffoldDescription};

fn main() -> Result<()> {
    let opts = Opts::parse();
    ScaffoldDescription::new(opts)?.scaffold()
}
