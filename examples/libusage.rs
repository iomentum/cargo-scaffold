use std::path::PathBuf;

use anyhow::Result;
use cargo_scaffold::{Opts, ScaffoldDescription};
use toml::Value;

fn main() -> Result<()> {
    let opts = Opts::builder()
        .project_name(String::from("testlib"))
        .template_path(PathBuf::from(
            "https://github.com/Cosmian/mpc_rust_template.git",
        ))
        .build();
    // let mut params = BTreeMap::new();
    // params.insert("players_nb".to_string(), Value::Integer(3));
    let scaffold_desc = ScaffoldDescription::new(opts)?;
    let mut params = scaffold_desc.fetch_parameters_value()?;
    params.insert("players_nb".to_string(), Value::Integer(3));
    scaffold_desc.scaffold_with_parameters(params)
    // .scaffold_with_parameters(params)
}
