use anyhow::Result;
use cargo_scaffold::{Cargo, ScaffoldDescription};
use structopt::StructOpt;

fn main() -> Result<()> {
    let Cargo::Scaffold(opts) = Cargo::from_args();
    ScaffoldDescription::new(opts)?.scaffold()
}
