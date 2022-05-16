use std::{collections::BTreeMap, path::PathBuf};

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
    let mut params = BTreeMap::new();
    params.insert("players_nb".to_string(), Value::Integer(3));
    ScaffoldDescription::new(opts)?.scaffold_with_parameters(params)
}
