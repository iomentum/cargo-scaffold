use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Result;
use cargo_scaffold::{Opts, ScaffoldDescription};
use toml::Value;

fn main() -> Result<()> {
    let opts = Opts {
        append: false,
        force: false,
        passphrase_needed: false,
        project_name: String::from("testlib").into(),
        template_path: PathBuf::from("https://github.com/Cosmian/mpc_rust_template.git"),
        target_dir: None,
        template_commit: None,
    };
    let mut params = BTreeMap::new();
    params.insert("players_nb".to_string(), Value::Integer(3));
    ScaffoldDescription::new(opts)?.scaffold_with_parameters(params)
}
