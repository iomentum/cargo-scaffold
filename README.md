# Cargo-scaffold

`cargo-scaffold` is flexible and easy developer tool to let you scaffold a project. It's fully configurable without writing any line of codes and generate any kind of projects with a developer friendly CLI.

// Gif shell

## Features

+ Scaffold a project in a second
+ User interactions automatically generated
+ Do not write any lines of code, only description of your template is necessary
+ Not only for Rust crate/project. It's completely language agnostic

## Installation

```
cargo install cargo-scaffold
```

## Usage

You can scaffold your project from any `cargo-template` scaffold located locally in a directory or in a git repository

```
# Locally
cargo scaffold your_template_dir

# From git repository
cargo scaffold git@github.com:username/yourtemplate.git
```

Here is the available options for `cargo scaffold`:

```
USAGE:
    cargo-scaffold scaffold [FLAGS] [OPTIONS] <template>

FLAGS:
    -f, --force         Override target directory if it exists
    -h, --help          Prints help information
    -p, --passphrase    Specify if your ssh key is protected by a passphrase
    -V, --version       Prints version information

OPTIONS:
    -t, --target-directory <target-directory>    Specifiy the target directory

ARGS:
    <template>    Specifiy your template location
```


## Write your own template

To let you scaffold and generate different projects the only mandatory part is to have a `.scaffold.toml` file at the root of the template directory. This file is useful to document and add user interactions for your template. In your template's directory each files and directory will be copy/pasted to your generated project but updated using [Handlebars templating](https://handlebarsjs.com/).

### Template description

Here is an example of `.scaffold.toml` file:

```toml
# Exclude paths to not copy/paste to the generated project
exclude = [
    "./target"
]

# Basic template informations
[template]
name = "test"
author = "Benjamin Coenen <5719034+bnjjj@users.noreply.github.com>"
version = "0.1.0"

# Parameters are basically all the variables needed to generate your template using templating. It will be displayed as prompt to interact with user (due to the message subfield).
# All the parameters will be available in your templates as variables (example: `{{description}}`).
[parameters]
    # [parameters.name] is already reserved
    [parameters.feature]
    type = "string"
    message = "What is the name of your feature ?"
    required = true

    [parameters.gender]
    type = "select"
    message = "Which kind of API do you want to scaffold ?"
    values = ["REST", "graphql"]

    [parameters.dependencies]
    type = "multiselect"
    message = "Which dependencies do you want to use ?"
    values = ["serde", "anyhow", "regex", "rand", "tokio"]

    [parameters.description]
    type = "string"
    message = "What is the description of your feature ?"
    default = "Here is my default description"

    [parameters.show_description]
    type = "boolean"
    message = "Do you want to display the description ?"

    [parameters.limit]
    type = "integer"
    message = "What is the limit ?"
```

Here is the list of different types you can use for your parameter: `string`, `integer`, `float`, `boolean`, `select`, `multiselect`.

### Templating

In any files inside your template's directory you can use [Handlebars templating](https://handlebarsjs.com/guide/). Please refer to this documentation for all the syntax about templating. Here is a basic example if you want to display the parameter named `description` and if the boolean parameter `show_description` is set to `true` as described in the previous section.

```
{{#if show_description}} {{description}} {{/if}}
```