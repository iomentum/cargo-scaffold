mod git;

use crate::git::clone;

use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::Read,
    path::PathBuf,
    string::ToString,
};

use anyhow::{anyhow, Result};
use clap::{App, Arg, ArgMatches};
use dialoguer::{Confirm, Input, MultiSelect, Select};
use handlebars::Handlebars;
use heck::KebabCase;
use serde::{Deserialize, Serialize};
use toml::Value;
use walkdir::WalkDir;

pub fn cli_init() -> Result<()> {
    let matches = App::new("cargo")
        .subcommand(
            App::new("scaffold")
                .about("Scaffold a new project from a template")
                .args(&[
                    Arg::with_name("template")
                        .help("Specifiy your template location")
                        .required(true),
                    Arg::with_name("name")
                        .short("n")
                        .long("name")
                        .help("Specify the name of your generated project (and so skip the prompt asking for it)")
                        .takes_value(true),
                    Arg::with_name("force")
                        .short("f")
                        .long("force")
                        .help("Override target directory if it exists")
                        .takes_value(false),
                    Arg::with_name("target-directory")
                        .short("d")
                        .long("target-directory")
                        .help("Specifiy the target directory")
                        .takes_value(true),
                    Arg::with_name("passphrase")
                        .short("p")
                        .long("passphrase")
                        .help("Specify if your ssh key is protected by a passphrase")
                        .takes_value(false),
                ]),
        )
        .get_matches();

    match matches.subcommand() {
        ("scaffold", Some(subcmd)) => ScaffoldDescription::from_cli(subcmd)?.scaffold(),
        _ => Err(anyhow!("cannot fin corresponding command")),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScaffoldDescription {
    template: TemplateDescription,
    parameters: Option<BTreeMap<String, Parameter>>,
    #[serde(skip)]
    target_dir: Option<PathBuf>,
    #[serde(skip)]
    template_path: PathBuf,
    #[serde(skip)]
    force: bool,
    #[serde(skip)]
    project_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateDescription {
    exclude: Option<Vec<String>>,
    notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Parameter {
    message: String,
    #[serde(default)]
    required: bool,
    r#type: ParameterType,
    default: Option<Value>,
    values: Option<Vec<Value>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    String,
    Integer,
    Float,
    Boolean,
    Select,
    MultiSelect,
}

pub struct Opts {
    pub template_path: PathBuf,
    pub project_name: Option<String>,
    pub target_dir: Option<PathBuf>,
    pub force: bool,
    pub passphrase_needed: bool,
}

impl ScaffoldDescription {
    pub fn from_cli(matches: &ArgMatches) -> Result<Self> {
        let mut template_path = matches.value_of("template").unwrap().to_string();
        let mut scaffold_desc: ScaffoldDescription = {
            if template_path.ends_with(".git") {
                let tmp_dir = env::temp_dir().join(format!("{:x}", md5::compute(&template_path)));
                if tmp_dir.exists() {
                    fs::remove_dir_all(&tmp_dir)?;
                }
                fs::create_dir_all(&tmp_dir)?;
                clone(&template_path, &tmp_dir, matches.is_present("passphrase"))?;
                template_path = tmp_dir.to_string_lossy().to_string();
            }
            let mut scaffold_file =
                File::open(PathBuf::from(&template_path).join(".scaffold.toml"))?;
            let mut scaffold_desc_str = String::new();
            scaffold_file.read_to_string(&mut scaffold_desc_str)?;
            toml::from_str(&scaffold_desc_str)?
        };

        scaffold_desc.target_dir = matches.value_of("target-directory").map(PathBuf::from);
        scaffold_desc.force = matches.is_present("force");
        scaffold_desc.template_path = PathBuf::from(template_path);
        scaffold_desc.project_name = matches.value_of("name").map(String::from);

        Ok(scaffold_desc)
    }

    pub fn new(opts: Opts) -> Result<Self> {
        let mut template_path = opts.template_path.to_string_lossy().to_string();
        let mut scaffold_desc: ScaffoldDescription = {
            if template_path.ends_with(".git") {
                let tmp_dir = env::temp_dir().join(format!("{:x}", md5::compute(&template_path)));
                if tmp_dir.exists() {
                    fs::remove_dir_all(&tmp_dir)?;
                }
                fs::create_dir_all(&tmp_dir)?;
                clone(&template_path, &tmp_dir, opts.passphrase_needed)?;
                template_path = tmp_dir.to_string_lossy().to_string();
            }
            let mut scaffold_file =
                File::open(PathBuf::from(&template_path).join(".scaffold.toml"))?;
            let mut scaffold_desc_str = String::new();
            scaffold_file.read_to_string(&mut scaffold_desc_str)?;
            toml::from_str(&scaffold_desc_str)?
        };

        scaffold_desc.target_dir = opts.target_dir;
        scaffold_desc.force = opts.force;
        scaffold_desc.template_path = opts.template_path;
        scaffold_desc.project_name = opts.project_name;

        Ok(scaffold_desc)
    }

    fn create_dir(&self, name: &str) -> Result<PathBuf> {
        let dir_name = name.to_kebab_case();
        let dir_path = self
            .target_dir
            .clone()
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| ".".into()))
            .join(&dir_name);

        if dir_path.exists() {
            if !self.force {
                return Err(anyhow!(
                    "cannot create {} because it already exists",
                    dir_path.to_string_lossy()
                ));
            } else {
                fs::remove_dir_all(&dir_path)?;
            }
        }

        fs::create_dir_all(&dir_path)?;
        let path = fs::canonicalize(dir_path)?;

        Ok(path)
    }

    fn fetch_parameters_value(&self) -> Result<BTreeMap<String, Value>> {
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
                    ParameterType::Select => {
                        let idx_selected = Select::new()
                            .items(
                                parameter
                                    .values
                                    .as_ref()
                                    .expect("cannot make a select parameter with empty values"),
                            )
                            .with_prompt(parameter.message)
                            .default(0)
                            .interact()?;
                        parameter
                            .values
                            .as_ref()
                            .expect("cannot make a select parameter with empty values")
                            .get(idx_selected)
                            .unwrap()
                            .clone()
                    }
                    ParameterType::MultiSelect => {
                        let idxs_selected = MultiSelect::new()
                            .items(
                                parameter
                                    .values
                                    .as_ref()
                                    .expect("cannot make a select parameter with empty values"),
                            )
                            .with_prompt(parameter.message.clone())
                            .interact()?;
                        let values = idxs_selected
                            .into_iter()
                            .map(|idx| {
                                parameter
                                    .values
                                    .as_ref()
                                    .expect("cannot make a select parameter with empty values")
                                    .get(idx)
                                    .unwrap()
                                    .clone()
                            })
                            .collect();

                        Value::Array(values)
                    }
                };
                parameters.insert(parameter_name, value);
            }
        }

        Ok(parameters)
    }

    pub fn scaffold(&self) -> Result<()> {
        let excludes = self.template.exclude.clone().unwrap_or_default();

        let mut parameters: BTreeMap<String, Value> = self.fetch_parameters_value()?;
        let name: String = match &self.project_name {
            Some(project_name) => project_name.clone(),
            None => Input::new()
                .with_prompt("What is the name of your generated project ?")
                .interact()?,
        };
        let dir_path = self.create_dir(&name)?;
        parameters.insert(
            "target_dir".to_string(),
            Value::String(dir_path.to_str().unwrap_or_default().to_string()),
        );

        parameters.insert("name".to_string(), Value::String(name));
        // List entries inside directory
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

        let mut template_engine = Handlebars::new();
        handlebars_misc_helpers::setup_handlebars(&mut template_engine);
        for entry in entries {
            let entry = entry.map_err(|e| anyhow!("cannot read entry : {}", e))?;
            let entry_path = entry.path().strip_prefix(&self.template_path)?;
            if entry_path == PathBuf::from("") {
                continue;
            }
            // TODO: check to ignore .git dir
            if entry.file_type().is_dir() {
                if entry.path().to_str() == Some(".") {
                    continue;
                }
                fs::create_dir(dir_path.join(entry_path))
                    .map_err(|e| anyhow!("cannot create dir : {}", e))?;
                continue;
            }

            let filename = entry.path();
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
            let rendered_path = template_engine
                .render_template(
                    dir_path
                        .join(entry_path)
                        .to_str()
                        .expect("path is not utf8 valid"),
                    &parameters,
                )
                .map_err(|e| anyhow!("cannot render template for path : {}", e))?;
            fs::write(rendered_path, rendered_content)
                .map_err(|e| anyhow!("cannot create file : {}", e))?;
        }

        if let Some(notes) = &self.template.notes {
            let rendered_notes = template_engine
                .render_template(notes, &parameters)
                .map_err(|e| anyhow!("cannot render template for path : {}", e))?;
            println!("------------------------\n\n{}", rendered_notes);
        }

        Ok(())
    }
}
