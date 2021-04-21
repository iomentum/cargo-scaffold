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
use clap::{App, Arg, ArgMatches};
use console::{Emoji, Style};
use dialoguer::{Confirm, Input, MultiSelect, Select};
use fs::OpenOptions;
use globset::{Glob, GlobSetBuilder};
use handlebars::Handlebars;
use helpers::ForRangHelper;
use serde::{Deserialize, Serialize};
use toml::Value;
use walkdir::WalkDir;

const SCAFFOLD_FILENAME: &str = ".scaffold.toml";

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
                    Arg::with_name("append")
                        .short("a")
                        .long("append")
                        .help("Append files in the existing directory, do not create directory with the project name")
                        .takes_value(false),
                    Arg::with_name("target-directory")
                        .short("d")
                        .long("target-directory")
                        .help("Specifiy the target directory")
                        .takes_value(true),
                    Arg::with_name("passphrase")
                        .short("p")
                        .long("passphrase")
                        .help("Specify if your SSH key is protected by a passphrase")
                        .takes_value(false),
                ]),
        )
        .get_matches();

    match matches.subcommand() {
        ("scaffold", Some(subcmd)) => ScaffoldDescription::from_cli(subcmd)?.scaffold(),
        _ => Err(anyhow!("cannot find corresponding command")),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

// TODO: switch to structopt with clap
pub struct Opts {
    /// Specifiy your template location
    pub template_path: PathBuf,
    /// Specify the name of your generated project (and so skip the prompt asking for it)
    pub project_name: Option<String>,
    /// Specifiy the target directory
    pub target_dir: Option<PathBuf>,
    /// Override target directory if it exists
    pub force: bool,
    /// Append files in the existing directory, do not create directory with the project name
    pub append: bool,
    /// Specify if your SSH key is protected by a passphrase
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
        scaffold_desc.append = matches.is_present("append");

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
                File::open(PathBuf::from(&template_path).join(SCAFFOLD_FILENAME))
                    .with_context(|| format!("cannot open .scaffold.toml in {}", template_path))?;
            let mut scaffold_desc_str = String::new();
            scaffold_file.read_to_string(&mut scaffold_desc_str)?;
            toml::from_str(&scaffold_desc_str)?
        };

        scaffold_desc.target_dir = opts.target_dir;
        scaffold_desc.force = opts.force;
        scaffold_desc.template_path = opts.template_path;
        scaffold_desc.project_name = opts.project_name;
        scaffold_desc.append = opts.append;

        Ok(scaffold_desc)
    }

    fn create_dir(&self, name: &str) -> Result<PathBuf> {
        let mut dir_path = self
            .target_dir
            .clone()
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| ".".into()));

        let cyan = Style::new().cyan();
        if !self.append {
            dir_path = dir_path.join(name);
            if dir_path.exists() {
                if !self.force {
                    return Err(anyhow!(
                        "cannot create {} because it already exists",
                        dir_path.to_string_lossy()
                    ));
                } else {
                    println!(
                        "{} {}",
                        Emoji("🔄", ""),
                        cyan.apply_to("Override directory…"),
                    );
                    fs::remove_dir_all(&dir_path).with_context(|| "Cannot remove directory")?;
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
        } else {
            println!(
                "{} {}",
                Emoji("🔄", ""),
                cyan.apply_to(format!(
                    "Append to directory {}…",
                    dir_path.to_string_lossy()
                )),
            );
        }
        let path = fs::canonicalize(dir_path).with_context(|| "Cannot canonicalize path")?;

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
        let excludes = match &self.template.exclude {
            Some(exclude) => {
                let mut builder = GlobSetBuilder::new();
                for ex in exclude {
                    builder.add(Glob::new(ex)?);
                }

                builder.build()?
            }
            None => GlobSetBuilder::new().build()?,
        };

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
                self.run_hooks(&dir_path, &commands)?;
            }
        }

        parameters.insert("name".to_string(), Value::String(name.clone()));
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

                if entry.file_name() == SCAFFOLD_FILENAME {
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
            let permissions = entry
                .metadata()
                .map_err(|e| anyhow!("cannot get metadata for path : {}", e))?
                .permissions();

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .open(&rendered_path)?;
            file.set_permissions(permissions).map_err(|e| {
                anyhow!("cannot set permission to file '{}' : {}", rendered_path, e)
            })?;
            file.write_all(rendered_content.as_bytes())
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
                self.run_hooks(&dir_path, &commands)?;
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
            ScaffoldDescription::run_cmd(&cmd)?;
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
        let mut command = ScaffoldDescription::setup_cmd(&cmd)?;
        let mut child = command.spawn().expect("cannot execute command");
        child.wait().expect("failed to wait on child process");
        Ok(())
    }

    pub fn setup_cmd(cmd: &str) -> Result<Command> {
        let splitted_cmd =
            shell_words::split(&cmd).map_err(|e| anyhow!("cannot split command line : {}", e))?;
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
