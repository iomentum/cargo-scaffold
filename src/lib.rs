#![doc = include_str!("../README.md")]
mod git;
mod helpers;

use crate::git::clone;

use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    string::ToString,
};

use anyhow::{anyhow, Context, Result};
use console::{Emoji, Style};
use dialoguer::{Confirm, Input, MultiSelect, Select};
use fs::OpenOptions;
use globset::{Glob, GlobSetBuilder};
use handlebars::Handlebars;
use helpers::ForRangHelper;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use walkdir::WalkDir;

pub use toml::Value;
pub const SCAFFOLD_FILENAME: &str = ".scaffold.toml";

#[derive(Serialize, Deserialize)]
pub struct ScaffoldDescription {
    template: TemplateDescription,
    parameters: Option<BTreeMap<String, Parameter>>,
    hooks: Option<Hooks>,
    #[serde(skip)]
    target_dir: Option<PathBuf>,
    #[serde(skip)]
    template_path: PathBuf,
    #[serde(skip)]
    force: bool,
    #[serde(skip)]
    append: bool,
    #[serde(skip)]
    project_name: Option<String>,
    #[serde(skip)]
    default_parameters: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateDescription {
    exclude: Option<Vec<String>>,
    disable_templating: Option<Vec<String>>,
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
    tags: Option<Vec<String>>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Hooks {
    pre: Option<Vec<String>>,
    post: Option<Vec<String>>,
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

#[derive(StructOpt)]
pub enum Cargo {
    #[structopt(about = "Scaffold a new project from a template")]
    Scaffold(Opts),
}

#[derive(StructOpt)]
pub struct Opts {
    /// Specifiy your template location
    #[structopt(name = "template", required = true)]
    pub template_path: PathBuf,

    /// Specifiy your template location in the repository if it's not located at the root of your repository
    #[structopt(name = "repository_template_path", short = "r", long = "path")]
    pub repository_template_path: Option<PathBuf>,

    /// Full commit hash or tag from which the template is cloned
    /// (i.e.: "deed14dcbf17ba87f6659ea05755cf94cb1464ab" or "v0.5.0")
    #[structopt(name = "git_ref", short = "t", long = "git_ref")]
    pub git_ref: Option<String>,

    /// Specify the name of your generated project (and so skip the prompt asking for it)
    #[structopt(name = "name", short = "n", long = "name")]
    pub project_name: Option<String>,

    /// Specifiy the target directory
    #[structopt(name = "target_directory", short = "d", long = "target_directory")]
    pub target_dir: Option<PathBuf>,

    /// Override target directory if it exists
    #[structopt(short = "f", long = "force")]
    pub force: bool,

    /// Append files in the target directory, create directory with the project name if it doesn't already exist but doesn't overwrite existing file (use force for that kind of usage)
    #[structopt(short = "a", long = "append")]
    pub append: bool,

    /// Specify if your SSH key is protected by a passphrase
    #[structopt(short = "p", long = "passphrase")]
    pub passphrase_needed: bool,

    /// Specify if your private SSH key is located in another location than $HOME/.ssh/id_rsa
    #[structopt(short = "k", long = "private_key_path")]
    pub private_key_path: Option<PathBuf>,

    /// Supply parameters via the command line
    #[structopt(long = "param")]
    pub default_parameters: Vec<String>,
}

#[buildstructor::buildstructor]
impl Opts {
    #[builder]
    #[allow(clippy::too_many_arguments)]
    #[allow(dead_code)]
    pub fn new(
        template_path: PathBuf,
        repository_template_path: Option<PathBuf>,
        git_ref: Option<String>,
        project_name: Option<String>,
        target_dir: Option<PathBuf>,
        force: Option<bool>,
        append: Option<bool>,
        passphrase_needed: Option<bool>,
        private_key_path: Option<PathBuf>,
        default_parameters: Vec<String>,
    ) -> Opts {
        Self {
            template_path,
            repository_template_path,
            git_ref,
            project_name,
            target_dir,
            force: force.unwrap_or_default(),
            append: append.unwrap_or_default(),
            passphrase_needed: passphrase_needed.unwrap_or_default(),
            private_key_path,
            default_parameters,
        }
    }
}

impl ScaffoldDescription {
    pub fn new(opts: Opts) -> Result<Self> {
        let mut default_parameters = BTreeMap::new();
        for param in opts.default_parameters {
            let split = param.splitn(2, '=').collect::<Vec<_>>();
            if split.len() != 2 {
                return Err(anyhow!("invalid argument: {}", param));
            }
            default_parameters.insert(split[0].to_string(), Value::String(split[1].to_string()));
        }
        let mut template_path = opts.template_path.to_string_lossy().to_string();
        let mut scaffold_desc: ScaffoldDescription = {
            if template_path.ends_with(".git") {
                let tmp_dir = env::temp_dir().join(format!("{:x}", md5::compute(&template_path)));
                if tmp_dir.exists() {
                    fs::remove_dir_all(&tmp_dir)?;
                }
                fs::create_dir_all(&tmp_dir)?;
                clone(
                    &template_path,
                    &opts.git_ref,
                    &tmp_dir,
                    &opts.private_key_path.unwrap_or_else(|| {
                        PathBuf::from(&format!(
                            "{}/.ssh/id_rsa",
                            env::var("HOME").expect("cannot fetch $HOME")
                        ))
                    }),
                    opts.passphrase_needed,
                )?;
                template_path = match opts.repository_template_path {
                    Some(sub_path) => tmp_dir.join(sub_path).to_string_lossy().to_string(),
                    None => tmp_dir.to_string_lossy().to_string(),
                };
            }
            let mut scaffold_file =
                File::open(PathBuf::from(&template_path).join(SCAFFOLD_FILENAME))
                    .with_context(|| format!("cannot open .scaffold.toml in {}", template_path))?;
            let mut scaffold_desc_str = String::new();
            scaffold_file.read_to_string(&mut scaffold_desc_str)?;
            toml::from_str(&scaffold_desc_str)?
        };

        scaffold_desc.target_dir = opts.target_dir;
        scaffold_desc.force = opts.force;
        scaffold_desc.template_path = PathBuf::from(template_path);
        scaffold_desc.project_name = opts.project_name;
        scaffold_desc.append = opts.append;
        scaffold_desc.default_parameters = default_parameters;

        Ok(scaffold_desc)
    }

    pub fn name(&self) -> Option<String> {
        self.project_name.clone()
    }

    pub fn parameters(&self) -> Option<&BTreeMap<String, Parameter>> {
        self.parameters.as_ref()
    }

    fn create_dir(&self, name: &str) -> Result<PathBuf> {
        let mut dir_path = self
            .target_dir
            .clone()
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| ".".into()));

        let cyan = Style::new().cyan();
        if self.target_dir.is_none() {
            dir_path = dir_path.join(name);
        }
        if dir_path.exists() {
            if !self.force && !self.append {
                return Err(anyhow!(
                    "cannot create {} because it already exists",
                    dir_path.to_string_lossy()
                ));
            } else if self.force {
                println!(
                    "{} {}",
                    Emoji("🔄", ""),
                    cyan.apply_to("Override directory…"),
                );
                fs::remove_dir_all(&dir_path).with_context(|| "Cannot remove directory")?;
            } else if self.append {
                println!(
                    "{} {}",
                    Emoji("🔄", ""),
                    cyan.apply_to(format!(
                        "Append to directory {}…",
                        dir_path.to_string_lossy()
                    )),
                );
            }
        } else {
            println!(
                "{} {}",
                Emoji("🔄", ""),
                cyan.apply_to(format!(
                    "Creating directory {}…",
                    dir_path.to_string_lossy()
                )),
            );
        }
        fs::create_dir_all(&dir_path).with_context(|| "Cannot create directory")?;
        let path = fs::canonicalize(dir_path).with_context(|| "Cannot canonicalize path")?;

        Ok(path)
    }

    /// Launch prompt to the user to ask for different parameters
    pub fn fetch_parameters_value(&self) -> Result<BTreeMap<String, Value>> {
        let mut parameters: BTreeMap<String, Value> = self.default_parameters.clone();
        let mut parameter_list = self
            .parameters
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        if let None = self.parameters.clone().unwrap_or_default().get("name") {
            parameter_list.insert(
                0,
                (
                    "name".to_string(),
                    Parameter {
                        message: "What is the name of your generated project ?".to_string(),
                        required: true,
                        r#type: ParameterType::String,
                        default: None,
                        values: None,
                        tags: None,
                    },
                ),
            );
        }
        for (parameter_name, parameter) in parameter_list {
            if parameters.contains_key(&parameter_name) {
                continue;
            }

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

        Ok(parameters)
    }

    /// Scaffold the project with the template
    pub fn scaffold(&self) -> Result<()> {
        let mut parameters = self.default_parameters.clone();
        parameters.append(&mut self.fetch_parameters_value()?);
        self.internal_scaffold(parameters)
    }

    /// Scaffold the project with the given parameters defined in the .scaffold.toml without prompting any inputs
    /// It's a non-interactive mode
    pub fn scaffold_with_parameters(&self, mut parameters: BTreeMap<String, Value>) -> Result<()> {
        let mut default_parameters = self.default_parameters.clone();
        if let Some(name) = &self.project_name {
            parameters.insert("name".to_string(), Value::String(name.clone()));
        } else {
            return Err(anyhow!("project_name must be set"));
        }

        default_parameters.append(&mut parameters);
        self.internal_scaffold(default_parameters)
    }

    fn internal_scaffold(&self, mut parameters: BTreeMap<String, Value>) -> Result<()> {
        let excludes = match &self.template.exclude {
            Some(exclude) => {
                let mut builder = GlobSetBuilder::new();
                for ex in exclude {
                    builder.add(Glob::new(ex.trim_start_matches("./"))?);
                }

                builder.build()?
            }
            None => GlobSetBuilder::new().build()?,
        };
        let disable_templating = match &self.template.disable_templating {
            Some(exclude) => {
                let mut builder = GlobSetBuilder::new();
                for ex in exclude {
                    builder.add(Glob::new(ex.trim_start_matches("./"))?);
                }

                builder.build()?
            }
            None => GlobSetBuilder::new().build()?,
        };

        let name = parameters
            .get("name")
            .expect("project name must have been set. qed")
            .to_string();
        let dir_path = self.create_dir(&name)?;
        parameters.insert(
            "target_dir".to_string(),
            Value::String(dir_path.to_str().unwrap_or_default().to_string()),
        );

        // pre-hooks
        if let Some(Hooks {
            pre: Some(commands),
            ..
        }) = &self.hooks
        {
            if !commands.is_empty() {
                let cyan = Style::new().cyan();
                println!(
                    "{} {}",
                    Emoji("🤖", ""),
                    cyan.apply_to("Triggering pre-hooks…"),
                );
                self.run_hooks(&dir_path, commands)?;
            }
        }

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

                if entry.depth() == 1 && entry.file_name() == SCAFFOLD_FILENAME {
                    return false;
                }

                !excludes.is_match(
                    entry
                        .path()
                        .strip_prefix(&self.template_path)
                        .unwrap_or_else(|_| entry.path()),
                )
            });

        let mut template_engine = Handlebars::new();
        template_engine.set_strict_mode(false);
        #[cfg(feature = "helpers")]
        handlebars_misc_helpers::setup_handlebars(&mut template_engine);
        template_engine.register_helper("forRange", Box::new(ForRangHelper));

        let cyan = Style::new().cyan();
        println!("{} {}", Emoji("🔄", ""), cyan.apply_to("Templating files…"),);
        for entry in entries {
            let entry = entry.map_err(|e| anyhow!("cannot read entry : {}", e))?;
            let entry_path = entry.path().strip_prefix(&self.template_path)?;

            if entry_path == PathBuf::from("") {
                continue;
            }
            if entry.file_type().is_dir() {
                if entry.path().to_str() == Some(".") {
                    continue;
                }
                let dir_path_to_create = dir_path.join(entry_path);
                if dir_path_to_create.exists() && self.force {
                    fs::remove_dir_all(&dir_path_to_create)
                        .with_context(|| "Cannot remove directory")?;
                }
                if dir_path_to_create.exists() && self.append {
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
                // TODO add the ability to read a non string file
                file.read_to_string(&mut content)
                    .map_err(|e| anyhow!("cannot read file {filename:?} : {}", e))?;
            }
            let (path, content) = if disable_templating.is_match(entry_path) {
                (
                    dir_path.join(entry_path).to_string_lossy().to_string(),
                    content,
                )
            } else {
                let rendered_content = template_engine
                    .render_template(&content, &parameters)
                    .map_err(|e| anyhow!("cannot render template {entry_path:?} : {}", e))?;
                let rendered_path = template_engine
                    .render_template(
                        dir_path
                            .join(entry_path)
                            .to_str()
                            .expect("path is not utf8 valid"),
                        &parameters,
                    )
                    .map_err(|e| {
                        anyhow!("cannot render template for path {entry_path:?} : {}", e)
                    })?;

                (rendered_path, rendered_content)
            };

            let filename_path = PathBuf::from(&path);
            // We skip the file if the file already exist and if we are in an append mode
            if filename_path.exists() && !self.force && self.append {
                continue;
            }

            let permissions = entry
                .metadata()
                .map_err(|e| anyhow!("cannot get metadata for path : {}", e))?
                .permissions();

            let mut file = OpenOptions::new().write(true).create(true).open(&path)?;
            file.set_permissions(permissions)
                .map_err(|e| anyhow!("cannot set permission to file '{}' : {}", path, e))?;
            file.write_all(content.as_bytes())
                .map_err(|e| anyhow!("cannot create file : {}", e))?;
        }

        let green = Style::new().green();
        println!(
            "{} Your project {} has been generated successfuly {}",
            Emoji("✅", ""),
            green.apply_to(name),
            Emoji("🚀", "")
        );

        let yellow = Style::new().yellow();
        println!(
            "\n{}\n",
            yellow.apply_to("-----------------------------------------------------"),
        );

        if let Some(notes) = &self.template.notes {
            let rendered_notes = template_engine
                .render_template(notes, &parameters)
                .map_err(|e| anyhow!("cannot render template for path : {}", e))?;
            println!("{}", rendered_notes);
            println!(
                "\n{}\n",
                yellow.apply_to("-----------------------------------------------------"),
            );
        }

        // post-hooks
        if let Some(Hooks {
            post: Some(commands),
            ..
        }) = &self.hooks
        {
            if !commands.is_empty() {
                let cyan = Style::new().cyan();
                println!(
                    "{} {}",
                    Emoji("🤖", ""),
                    cyan.apply_to("Triggering post-hooks…"),
                );
                self.run_hooks(&dir_path, commands)?;
            }
        }

        Ok(())
    }

    fn run_hooks(&self, project_path: &Path, commands: &[String]) -> Result<()> {
        let initial_path = std::env::current_dir()?;
        // move to project directory
        std::env::set_current_dir(&project_path).map_err(|e| {
            anyhow!(
                "cannot change directory to project path {:?}: {}",
                &project_path,
                e
            )
        })?;
        // run commands
        let magenta = Style::new().magenta();
        for cmd in commands {
            println!("{} {}", Emoji("✨", ""), magenta.apply_to(cmd));
            ScaffoldDescription::run_cmd(cmd)?;
        }
        // move back to initial path
        std::env::set_current_dir(&initial_path).map_err(|e| {
            anyhow!(
                "cannot move back to original path {:?}: {}",
                &initial_path,
                e
            )
        })?;
        Ok(())
    }

    pub fn run_cmd(cmd: &str) -> Result<()> {
        let mut command = ScaffoldDescription::setup_cmd(cmd)?;
        let mut child = command.spawn().expect("cannot execute command");
        child.wait().expect("failed to wait on child process");
        Ok(())
    }

    pub fn setup_cmd(cmd: &str) -> Result<Command> {
        let splitted_cmd =
            shell_words::split(cmd).map_err(|e| anyhow!("cannot split command line : {}", e))?;
        if splitted_cmd.is_empty() {
            anyhow::bail!("command argument is invalid: empty after splitting");
        }
        let mut command = Command::new(&splitted_cmd[0]);
        if splitted_cmd.len() > 1 {
            command.args(&splitted_cmd[1..]);
        }
        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use super::ScaffoldDescription;
    use std::fs::{remove_file, File};
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn split_and_run_cmd() {
        let mut command = ScaffoldDescription::setup_cmd("ls -alh .").unwrap();
        command.stdout(Stdio::null());
        let mut child = command.spawn().expect("cannot execute command");
        child.wait().expect("failed to wait on child process");

        let mut command = ScaffoldDescription::setup_cmd("/bin/bash -c ls").unwrap();
        command.stdout(Stdio::null());
        let mut child = command.spawn().expect("cannot execute command");
        child.wait().expect("failed to wait on child process");
    }

    #[test]
    fn split_and_run_script() {
        let script_name = "./test.sh";
        let cmd = format!("/bin/bash -c {}", script_name);
        {
            let mut file = File::create(script_name).unwrap();
            file.write_all(b"#!/bin/bash\nls .\nfree").unwrap();
            Command::new("chmod")
                .arg("+x")
                .arg(&script_name)
                .output()
                .expect("can't set execute perm on script file");
        }
        let mut command = ScaffoldDescription::setup_cmd(&cmd).unwrap();
        command.stdout(Stdio::null());
        let mut child = command.spawn().expect("cannot execute command");
        child.wait().expect("failed to wait on child process");
        // uncomment to see output of script execution
        // std::io::stdout().write_all(&_output.stdout).unwrap();
        remove_file(&script_name).unwrap();
    }
}
