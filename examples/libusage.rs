use anyhow::Result;
use toml::Value;

use cargo_scaffold::{Opts, ScaffoldDescription};
fn main() -> Result<()> {
    let opts = Opts::default()
        .project_name("testlib")
        .template_path("https://github.com/Cosmian/mpc_rust_template.git");

    let scaffold_desc = ScaffoldDescription::new(opts)?;

    let mut params = scaffold_desc.fetch_parameters_value()?;
    params.insert("players_nb".to_string(), Value::Integer(3));

    scaffold_desc.scaffold_with_parameters(params)
}
