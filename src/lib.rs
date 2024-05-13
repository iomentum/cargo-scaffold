#![doc = include_str!("../README.md")]
mod git;
mod helpers;

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
use clap::Parser;
use console::{Emoji, Style};
use dialoguer::{Confirm, Input, MultiSelect, Select};
use fs::OpenOptions;
use globset::{Glob, GlobSetBuilder};
use handlebars::Handlebars;
use helpers::ForRangHelper;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

pub use toml::Value;
pub const SCAFFOLD_FILENAME: &str = ".scaffold.toml";

#[derive(Serialize, Deserialize)]
pub struct ScaffoldDescription {
    template: TemplateDescription,
    #[serde(default)]
    parameters: BTreeMap<String, Parameter>,
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

/// Opts: The options for scaffolding.
///
/// This structure can be generated using the `parse` or `parse_from` method (when used in Cli) or
/// can be generated as `default` and then the required values can be updated.
///
/// Usage: If generated in a 'cli' binary
///
/// ```no_run
/// # use cargo_scaffold::{Opts, ScaffoldDescription};
/// # use clap::Parser;
/// # use anyhow::Result;
///
/// # fn main() -> Result<()> {
///
///     let opts = Opts::parse_from(vec!["scaffold", "/path/to/template"]);
///     ScaffoldDescription::new(opts)?.scaffold()
/// # }
///
/// ```
///
/// Usage: When generated as a library
///
/// ```no_run
/// # use cargo_scaffold::{Opts, ScaffoldDescription};
/// # use clap::Parser;
/// # use anyhow::Result;
///
/// # fn main() -> Result<()> {
///
///     let mut opts = Opts::default()
///         .project_name("testlib")
///         .template_path("https://github.com/Cosmian/mpc_rust_template.git");
///
///     ScaffoldDescription::new(opts)?.scaffold()
/// # }
///
/// ```
#[derive(Parser, Debug, Default)]
#[command(author, version, about, long_about=None)]
pub struct Opts {
    /// Specifiy your template location
    #[arg(name = "template", required = true)]
    template_path: PathBuf,

    /// Specifiy your template location in the repository if it's not located at the root of your repository
    #[arg(name = "repository_template_path", short = 'r', long = "path")]
    repository_template_path: Option<PathBuf>,

    /// Full commit hash, tag or branch from which the template is cloned
    /// (i.e.: "deed14dcbf17ba87f6659ea05755cf94cb1464ab" or "v0.5.0" or "main")
    #[arg(name = "git_ref", short = 't', long = "git_ref")]
    git_ref: Option<String>,

    /// Specify the name of your generated project (and so skip the prompt asking for it)
    #[arg(name = "name", short = 'n', long = "name")]
    project_name: Option<String>,

    /// Specifiy the target directory
    #[arg(name = "target_directory", short = 'd', long = "target_directory")]
    target_dir: Option<PathBuf>,

    /// Override target directory if it exists
    #[arg(short = 'f', long = "force")]
    force: bool,

    /// Append files in the target directory, create directory with the project name if it doesn't already exist but doesn't overwrite existing file (use force for that kind of usage)
    #[arg(short = 'a', long = "append")]
    append: bool,

    /// Ignored, kept for backwards compatibility [DEPRECATED]
    #[arg(short = 'p', long = "passphrase")]
    passphrase_needed: bool,

    /// Specify if your private SSH key is located in another location than $HOME/.ssh/id_rsa
    #[arg(short = 'k', long = "private_key_path")]
    private_key_path: Option<PathBuf>,

    /// Supply parameters via the command line in <name>=<value> format
    #[arg(long = "param")]
    parameters: Vec<String>,
}

impl Opts {
    /// Builder function for the `Opts` structure
    pub fn builder<T: Into<PathBuf>>(template_path: T) -> Self {
        Self::default().template_path(template_path)
    }

    /// Set the template path for the structure.
    pub fn template_path<T: Into<PathBuf>>(mut self, path: T) -> Self {
        let _ = std::mem::replace(&mut self.template_path, path.into());
        self
    }

    /// Set the template path inside the repository
    pub fn repository_template_path<T: Into<PathBuf>>(mut self, path: T) -> Self {
        let _ = self.repository_template_path.replace(path.into());
        self
    }

    /// Set the git reference
    pub fn git_ref<T: Into<String>>(mut self, gitref: T) -> Self {
        let _ = self.git_ref.replace(gitref.into());
        self
    }

    /// Set the project name
    pub fn project_name<T: Into<String>>(mut self, name: T) -> Self {
        let _ = self.project_name.replace(name.into());
        self
    }

    /// Set the target directory
    pub fn target_dir<T: Into<PathBuf>>(mut self, target_dir: T) -> Self {
        let _ = self.target_dir.replace(target_dir.into());
        self
    }

    /// Force generating to the target directory if exists
    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Append generating to the target directory if exists
    pub fn append(mut self, append: bool) -> Self {
        self.append = append;
        self
    }

    /// Is Passphrase needed (prompt user)
    pub fn passphrase_needed(mut self, needed: bool) -> Self {
        self.passphrase_needed = needed;
        self
    }

    /// Set the private key path
    pub fn private_key_path<T: Into<PathBuf>>(mut self, private_key_path: T) -> Self {
        let _ = self.private_key_path.replace(private_key_path.into());
        self
    }

    /// Set the parameters (supplied as `vec!["key1=value1", "key2=value2"]`).
    pub fn parameters<T: Into<String>>(mut self, params: Vec<T>) -> Self {
        let _ = std::mem::replace(
            &mut self.parameters,
            params
                .into_iter()
                .map(|x| x.into())
                .collect::<Vec<String>>(),
        );
        self
    }
}

impl ScaffoldDescription {
    pub fn new(opts: Opts) -> Result<Self> {
        let mut default_parameters = BTreeMap::new();
        for param in opts.parameters {
            let split = param.splitn(2, '=').collect::<Vec<_>>();
            if split.len() != 2 {
                return Err(anyhow!("invalid argument: {}", param));
            }
            default_parameters.insert(split[0].to_string(), Value::String(split[1].to_string()));
        }
        if let Some(ref name) = opts.project_name {
            default_parameters.insert("name".to_string(), Value::String(name.to_string()));
        }

        let mut template_path = opts.template_path.to_string_lossy().to_string();
        let mut scaffold_desc: ScaffoldDescription = {
            if template_path.ends_with(".git") {
                let tmp_dir = env::temp_dir().join(format!("{:x}", md5::compute(&template_path)));
                if tmp_dir.exists() {
                    fs::remove_dir_all(&tmp_dir)?;
                }
                fs::create_dir_all(&tmp_dir)?;
                git::clone(
                    &template_path,
                    opts.git_ref.as_deref(),
                    &tmp_dir,
                    opts.private_key_path.as_deref(),
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
        use std::collections::btree_map::Entry;

        let mut parameters: BTreeMap<String, Value> = self.default_parameters.clone();
        for (parameter_name, parameter) in &self.parameters {
            if let Entry::Vacant(entry) = parameters.entry(parameter_name.clone()) {
                entry.insert(parameter.to_value_interactive()?);
            }
        }

        if let Entry::Vacant(entry) = parameters.entry("name".to_string()) {
            let value = Parameter {
                message: "What is the name of your generated project ?".to_string(),
                required: true,
                r#type: ParameterType::String,
                default: None,
                values: None,
                tags: None,
            }
            .to_value_interactive()?;
            entry.insert(value);
        };

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
            .as_str()
            .expect("project name must be a string")
            .to_string();
        let dir_path = self.create_dir(&name)?;
        parameters.insert(
            "target_dir".to_string(),
            Value::String(dir_path.to_str().unwrap_or_default().to_string()),
        );

        let mut template_engine = Handlebars::new();
        template_engine.set_strict_mode(false);
        #[cfg(feature = "helpers")]
        handlebars_misc_helpers::setup_handlebars(&mut template_engine);
        template_engine.register_helper("forRange", Box::new(ForRangHelper));

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
            }
            let commands = commands
                .iter()
                .map(|c| template_engine.render_template(c, &parameters).ok())
                .map(|v| v.unwrap())
                .collect::<Vec<String>>();

            self.run_hooks(&dir_path, &commands)?;
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

                let entry_path = render_path(&template_engine, entry_path, &parameters)?;

                let dir_path_to_create = dir_path.join(&entry_path);
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
            let mut content = Vec::new();
            {
                let mut file =
                    File::open(filename).map_err(|e| anyhow!("cannot open file : {}", e))?;
                // TODO add the ability to read a non string file
                file.read_to_end(&mut content)
                    .map_err(|e| anyhow!("cannot read file {filename:?} : {}", e))?;
            }
            let (path, content) = if disable_templating.is_match(entry_path) {
                (dir_path.join(entry_path), content)
            } else {
                let content = std::str::from_utf8(&content)
                    .map_err(|_| anyhow!("invalid UTF-8 in {entry_path:?}, consider disabling templating for this file"))?;
                let rendered_content = template_engine
                    .render_template(content, &parameters)
                    .map_err(|e| anyhow!("cannot render template {entry_path:?} : {}", e))?;

                let rendered_path =
                    render_path(&template_engine, &dir_path.join(entry_path), &parameters)?;
                (rendered_path, rendered_content.into_bytes())
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
                .map_err(|e| anyhow!("cannot set permission to file {:?} : {}", path, e))?;
            file.write_all(&content)
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
                let commands = commands
                    .iter()
                    .map(|c| template_engine.render_template(c, &parameters).ok())
                    .map(|v| v.unwrap())
                    .collect::<Vec<String>>();

                self.run_hooks(&dir_path, &commands)?;
            }
        }

        Ok(())
    }

    fn run_hooks(&self, project_path: &Path, commands: &[String]) -> Result<()> {
        let initial_path = std::env::current_dir()?;
        // move to project directory
        std::env::set_current_dir(project_path).map_err(|e| {
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

fn render_path(
    template_engine: &Handlebars,
    path: &Path,
    parameters: &BTreeMap<String, Value>,
) -> Result<PathBuf> {
    // The backslash character used as path separator on windows is an escape character for handlebars.
    // Avoid passing it to the template renderer by expanding each path component individually.
    // This also prevents strange patterns where template placeholders span across single folder/file names.
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(component) => {
                let component = component
                    .to_str()
                    .ok_or_else(|| anyhow!("invalid Unicode path: {path:?}"))?;
                let rendered = template_engine
                    .render_template(component, parameters)
                    .map_err(|e| anyhow!("cannot render template for path {path:?} : {}", e))?;
                output.push(rendered);
            }
            component => output.push(component),
        };
    }
    Ok(output)
}

impl Parameter {
    fn to_value_interactive(&self) -> Result<toml::Value> {
        let value = match self.r#type {
            ParameterType::String => {
                Value::String(Input::new().with_prompt(&self.message).interact()?)
            }
            ParameterType::Float => {
                Value::Float(Input::<f64>::new().with_prompt(&self.message).interact()?)
            }
            ParameterType::Integer => {
                Value::Integer(Input::<i64>::new().with_prompt(&self.message).interact()?)
            }
            ParameterType::Boolean => {
                Value::Boolean(Confirm::new().with_prompt(&self.message).interact()?)
            }
            ParameterType::Select => {
                let idx_selected = Select::new()
                    .items(
                        self.values
                            .as_ref()
                            .expect("cannot make a select parameter with empty values"),
                    )
                    .with_prompt(&self.message)
                    .default(0)
                    .interact()?;
                self.values
                    .as_ref()
                    .expect("cannot make a select parameter with empty values")
                    .get(idx_selected)
                    .unwrap()
                    .clone()
            }
            ParameterType::MultiSelect => {
                let idxs_selected = MultiSelect::new()
                    .items(
                        self.values
                            .as_ref()
                            .expect("cannot make a select parameter with empty values"),
                    )
                    .with_prompt(&self.message)
                    .interact()?;
                let values = idxs_selected
                    .into_iter()
                    .map(|idx| {
                        self.values
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
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::{render_path, BTreeMap, Handlebars};

    use super::{Opts, ScaffoldDescription};
    use std::fs::{remove_file, File};
    use std::io::Write;
    use std::path::Path;
    use std::process::{Command, Stdio};

    #[test]
    #[cfg(windows)]
    fn windows_paths_interpolation_works() {
        // this isn't completely a scaffold test.
        // This is us making sure we don't regress with the interpolation
        let template_engine = Handlebars::new();

        let mut parameters = BTreeMap::new();
        parameters.insert("snake_name".to_string(), "tracing".to_string().into());

        let path = Path::new(
            "\\\\?\\C:\\Users\\Ignition\\AppData\\Local\\Temp\\router_scaffoldXwTZ11\\src\\plugins\\{{snake_name}}.rs"
        );
        let res = render_path(&template_engine, path, &parameters).unwrap();

        assert_eq!(Path::new("\\\\?\\C:\\Users\\Ignition\\AppData\\Local\\Temp\\router_scaffoldXwTZ11\\src\\plugins\\tracing.rs"), res);
    }

    #[test]
    #[cfg(unix)]
    fn unix_paths_interpolation_works() {
        // this isn't completely a scaffold test.
        // This is us making sure we don't regress with the interpolation
        let template_engine = Handlebars::new();

        let mut parameters = BTreeMap::new();
        parameters.insert("snake_name".to_string(), "tracing".to_string().into());

        let path = Path::new("/tmp/router_scaffoldXwTZ11/src/plugins/{{snake_name}}.rs");
        let res = render_path(&template_engine, path, &parameters).unwrap();

        assert_eq!(
            Path::new("/tmp/router_scaffoldXwTZ11/src/plugins/tracing.rs"),
            res
        );
    }

    #[test]
    #[cfg(unix)]
    fn unix_paths_dirs_work() {
        // this isn't completely a scaffold test.
        // This is us making sure we don't regress with the interpolation
        let template_engine = Handlebars::new();

        let mut parameters = BTreeMap::new();
        parameters.insert("snake_name".to_string(), "tracing".to_string().into());
        parameters.insert("directory_name".to_string(), "example".to_string().into());

        let path = Path::new(
            "/tmp/router_scaffoldXwTZ11/src/plugins/{{directory_name}}/{{snake_name}}.rs",
        );
        let res = render_path(&template_engine, path, &parameters).unwrap();

        assert_eq!(
            Path::new("/tmp/router_scaffoldXwTZ11/src/plugins/example/tracing.rs"),
            res
        );
    }

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
                .arg(script_name)
                .output()
                .expect("can't set execute perm on script file");
        }
        let mut command = ScaffoldDescription::setup_cmd(&cmd).unwrap();
        command.stdout(Stdio::null());
        let mut child = command.spawn().expect("cannot execute command");
        child.wait().expect("failed to wait on child process");
        // uncomment to see output of script execution
        // std::io::stdout().write_all(&_output.stdout).unwrap();
        remove_file(script_name).unwrap();
    }

    #[test]
    fn test_build_opts_works() {
        let opts = Opts::builder("/path/to/template");
        assert_eq!(
            opts.template_path,
            std::path::PathBuf::from("/path/to/template")
        );

        // Test projct name can be set
        assert!(opts.project_name.is_none());
        let opts = opts.project_name("project");
        assert_eq!(opts.project_name, Some("project".to_string()));

        // Test template can be set
        assert_eq!(
            opts.template_path,
            std::path::PathBuf::from("/path/to/template")
        );
        let opts = opts.template_path("/path/to/new-template");
        assert_eq!(
            opts.template_path,
            std::path::PathBuf::from("/path/to/new-template")
        );

        // Test repository template path can be set.
        assert!(opts.repository_template_path.is_none());
        let opts = opts.repository_template_path("somepath");
        assert_eq!(
            opts.repository_template_path,
            Some(std::path::PathBuf::from("somepath"))
        );

        // Test git_ref can be set
        assert!(opts.git_ref.is_none());
        let opts = opts.git_ref("main");
        assert_eq!(opts.git_ref, Some("main".to_string()));

        // Test target_dir
        assert!(opts.target_dir.is_none());
        let opts = opts.target_dir("target");
        assert_eq!(opts.target_dir, Some(std::path::PathBuf::from("target")));

        // Test append, force, passphrase_needed
        assert!(!opts.append);
        assert!(!opts.force);
        assert!(!opts.passphrase_needed);
        let opts = opts.append(true).force(true).passphrase_needed(true);
        assert!(opts.append);
        assert!(opts.force);
        assert!(opts.passphrase_needed);

        // Test private_key_path can be set
        assert!(opts.private_key_path.is_none());
        let opts = opts.private_key_path(".ssh/id_rsa");
        assert_eq!(
            opts.private_key_path,
            Some(std::path::PathBuf::from(".ssh/id_rsa"))
        );

        // Test parameters can be set
        assert!(opts.parameters.is_empty());
        let opts = opts.parameters(vec!["key1=value1"]);
        assert_eq!(opts.parameters, vec!["key1=value1".to_string()]);
    }
}
