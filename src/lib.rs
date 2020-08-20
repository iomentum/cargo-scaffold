use std::fs::{self, File};
use std::io::Read;
use std::string::ToString;
use std::{collections::BTreeMap, env, path::PathBuf};

use anyhow::{anyhow, Result};
use clap::{App, Arg, ArgMatches};
use dialoguer::{Confirm, Input};
use handlebars::Handlebars;
use heck::KebabCase;
use serde::{Deserialize, Serialize};
use toml::Value;
use walkdir::WalkDir;

pub fn init() -> Result<()> {
    let matches = App::new("cargo")
        .subcommand(
            App::new("scaffold")
                .about("Scaffold a new project from a template")
                .args(&[
                    Arg::with_name("template")
                        .help("specifiy your template location")
                        .required(true),
                    Arg::with_name("target-directory")
                        .short("t")
                        .long("target-directory")
                        .help("specifiy the target directory")
                        .takes_value(true),
                ]),
        )
        .get_matches();

    match matches.subcommand() {
        ("scaffold", Some(subcmd)) => ScaffoldDescription::new(subcmd)?.scaffold(),
        _ => Err(anyhow!("cannot fin corresponding command")),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ScaffoldDescription {
    parameters: Option<BTreeMap<String, Parameter>>,
    exclude: Option<Vec<String>>,
    #[serde(skip)]
    target_dir: Option<PathBuf>,
    #[serde(skip)]
    template_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Parameter {
    message: String,
    #[serde(default)]
    required: bool,
    r#type: ParameterType,
    default: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
enum ParameterType {
    String,
    Integer,
    Float,
    Boolean,
}

impl ScaffoldDescription {
    pub fn new(matches: &ArgMatches) -> Result<Self> {
        let template_path = matches.value_of("template").unwrap();
        let mut scaffold_desc: ScaffoldDescription = {
            let mut scaffold_file =
                File::open(PathBuf::from(template_path).join(".scaffold.toml"))?;
            let mut scaffold_desc_str = String::new();
            scaffold_file.read_to_string(&mut scaffold_desc_str)?;
            toml::from_str(&scaffold_desc_str)?
        };

        scaffold_desc.target_dir = matches.value_of("target-directory").map(PathBuf::from);
        scaffold_desc.template_path = PathBuf::from(template_path);

        Ok(scaffold_desc)
    }

    fn create_dir(&self, name: &str) -> Result<PathBuf> {
        let dir_name = name.to_kebab_case();
        let dir_path = self
            .target_dir
            .clone()
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| ".".into()))
            .join(&dir_name);

        // TODO: add force flag
        if dir_path.exists() {
            Err(anyhow!(
                "cannot create {} because it already exists",
                dir_path.to_string_lossy()
            ))
        } else {
            fs::create_dir(&dir_path)?;
            Ok(dir_path)
        }
    }

    fn scaffold(&self) -> Result<()> {
        let excludes = self.exclude.clone().unwrap_or_default();

        let name: String = Input::new()
            .with_prompt("What is the name of your generated project ?")
            .interact()?;

        let mut parameters: BTreeMap<String, Value> = BTreeMap::new();
        if let Some(parameter_list) = self.parameters.clone() {
            for (parameter_name, parameter) in parameter_list {
                let value: Value = match parameter.r#type {
                    ParameterType::String => {
                        Value::String(Input::new().with_prompt(parameter.message).interact()?)
                    }
                    ParameterType::Float => Value::Float(
                        Input::<f64>::new()
                            .with_prompt(parameter.message)
                            .interact()?,
                    ),
                    ParameterType::Integer => Value::Integer(
                        Input::<i64>::new()
                            .with_prompt(parameter.message)
                            .interact()?,
                    ),
                    ParameterType::Boolean => {
                        Value::Boolean(Confirm::new().with_prompt(parameter.message).interact()?)
                    }
                };
                parameters.insert(parameter_name, value);
            }
        }

        let dir_path = self.create_dir(&name)?;

        let entries = WalkDir::new(&self.template_path)
            .into_iter()
            .filter_entry(|entry| {
                // Do not include git files
                if entry
                    .path()
                    .components()
                    .any(|c| c == std::path::Component::Normal(".git".as_ref()))
                {
                    return false;
                }

                if entry.file_name() == ".scaffold.toml" {
                    return false;
                }

                for excl in &excludes {
                    if entry
                        .path()
                        .to_str()
                        .map(|s| s.starts_with(excl))
                        .unwrap_or(false)
                    {
                        return false;
                    }
                }

                true
            });

        for entry in entries {
            let entry = entry.map_err(|e| anyhow!("cannot read entry : {}", e))?;
            if excludes.contains(&entry.path().to_string_lossy().to_string()) {
                continue;
            }
            // TODO: check to ignore .git dir
            if entry.file_type().is_dir() {
                if entry.path().to_str() == Some(".") {
                    continue;
                }
                fs::create_dir(dir_path.join(entry.path()))
                    .map_err(|e| anyhow!("cannot create dir : {}", e))?;
                continue;
            }

            let filename = entry.path();
            let template_engine = Handlebars::new();
            let mut content = String::new();
            {
                let mut file =
                    File::open(filename).map_err(|e| anyhow!("cannot open file : {}", e))?;
                file.read_to_string(&mut content)
                    .map_err(|e| anyhow!("cannot read file : {}", e))?;
            }
            let rendered_content = template_engine
                .render_template(&content, &parameters)
                .map_err(|e| anyhow!("cannot render template : {}", e))?;

            fs::write(dir_path.join(filename), rendered_content)
                .map_err(|e| anyhow!("cannot create file : {}", e))?;
        }

        Ok(())
    }
}
