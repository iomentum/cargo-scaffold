use anyhow::Result;
use clap::Parser;
use toml::Value;

use cargo_scaffold::{Opts, ScaffoldDescription};
fn main() -> Result<()> {
    let args = vec![
        "libusage",
        "-n",
        "testlib",
        "https://github.com/Cosmian/mpc_rust_template.git",
    ];
    let opts = Opts::parse_from(args);

    // let mut params = BTreeMap::new();
    // params.insert("players_nb".to_string(), Value::Integer(3));
    let scaffold_desc = ScaffoldDescription::new(opts)?;
    let mut params = scaffold_desc.fetch_parameters_value()?;
    params.insert("players_nb".to_string(), Value::Integer(3));
    scaffold_desc.scaffold_with_parameters(params)
    // .scaffold_with_parameters(params)
}
