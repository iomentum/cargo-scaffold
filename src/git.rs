use anyhow::Result;
use console::{Emoji, Style};
use dialoguer::Password;
use git2::{Cred, RemoteCallbacks};
use std::path::Path;

pub(crate) fn clone(
    repository: &str,
    reference_opt: &Option<String>,
    target_dir: &Path,
    private_key_path: &Path,
    passphrase_needed: bool,
) -> Result<()> {
    let cyan = Style::new().cyan();
    println!(
        "{} {}",
        Emoji("🔄", ""),
        cyan.apply_to("Cloning repository…"),
    );
    let mut fetch_options = git2::FetchOptions::new();
    if !repository.starts_with("http") {
        let mut callbacks = RemoteCallbacks::new();
        let passphrase = if passphrase_needed {
            Password::new()
                .with_prompt(format!("Enter passphrase for {:?}", private_key_path))
                .interact()?
                .into()
        } else {
            None
        };
        callbacks.credentials(move |_url, username_from_url, _allowed_types| {
            Cred::ssh_key(
                username_from_url.unwrap(),
                // Some(&private_key_path.with_extension("pub")),
                None,
                private_key_path,
                passphrase.as_deref(),
            )
        });

        // Prepare fetch options.
        fetch_options.remote_callbacks(callbacks);
    }

    if reference_opt.is_some() {
        fetch_options.download_tags(git2::AutotagOption::All);
    }

    // Prepare builder.
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_options);

    // Clone the project.
    let repo = builder.clone(repository, target_dir)?;
    println!("target_dir -- {target_dir:?}");

    // Either a git tag, commit
    if let Some(git_reference) = reference_opt {
        let (obj, reference) = repo.revparse_ext(git_reference)?;
        repo.checkout_tree(&obj, None)?;
        match reference {
            // tagref is an actual reference like branches or tags
            Some(reporef) => repo.set_head(reporef.name().expect("tag has a name; qed")),
            // this is a commit, not a reference
            None => repo.set_head_detached(obj.id()),
        }?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, path::PathBuf};

    #[test]
    fn clone_http() {
        let private_key_path = PathBuf::from(&format!(
            "{}/.ssh/id_rsa",
            env::var("HOME").expect("cannot fetch $HOME")
        ));
        let template_path = "https://github.com/http-rs/surf.git";
        let tmp_dir = env::temp_dir().join(format!("{:x}1", md5::compute(&template_path)));
        if tmp_dir.exists() {
            fs::remove_dir_all(&tmp_dir).unwrap();
        }
        fs::create_dir_all(&tmp_dir).unwrap();
        clone(template_path, &None, &tmp_dir, &private_key_path, false).unwrap();
        fs::remove_dir_all(&tmp_dir).unwrap();
    }

    #[test]
    fn clone_http_commit() {
        let private_key_path = PathBuf::from(&format!(
            "{}/.ssh/id_rsa",
            env::var("HOME").expect("cannot fetch $HOME")
        ));
        let commit = Some("8f0039488b3877ca59592900bc7ad645a83e2886".to_owned());
        let template_path = "https://github.com/http-rs/surf.git";
        let tmp_dir =
            env::temp_dir().join(format!("{:x}2", md5::compute(&commit.clone().unwrap())));
        if tmp_dir.exists() {
            fs::remove_dir_all(&tmp_dir).unwrap();
        }
        fs::create_dir_all(&tmp_dir).unwrap();
        clone(template_path, &commit, &tmp_dir, &private_key_path, false).unwrap();
        fs::remove_dir_all(&tmp_dir).unwrap();
    }

    #[test]
    // warn: your ssh key must be in pem format
    fn clone_ssh() {
        let private_key_path = PathBuf::from(&format!(
            "{}/.ssh/id_rsa",
            env::var("HOME").expect("cannot fetch $HOME")
        ));
        let template_path = "git@github.com:http-rs/surf.git";
        let tmp_dir = env::temp_dir().join(format!("{:x}3", md5::compute(&template_path)));
        if tmp_dir.exists() {
            fs::remove_dir_all(&tmp_dir).unwrap();
        }
        fs::create_dir_all(&tmp_dir).unwrap();
        clone(template_path, &None, &tmp_dir, &private_key_path, false).unwrap();
        fs::remove_dir_all(&tmp_dir).unwrap();
    }

    #[test]
    // warn: your ssh key must be in pem format
    fn clone_ssh_commit() {
        let private_key_path = PathBuf::from(&format!(
            "{}/.ssh/id_rsa",
            env::var("HOME").expect("cannot fetch $HOME")
        ));
        let commit = Some("8f0039488b3877ca59592900bc7ad645a83e2886".to_owned());
        let template_path = "git@github.com:http-rs/surf.git";
        let tmp_dir = env::temp_dir().join(format!("{:x}4", md5::compute(&commit.unwrap())));
        if tmp_dir.exists() {
            fs::remove_dir_all(&tmp_dir).unwrap();
        }
        fs::create_dir_all(&tmp_dir).unwrap();
        clone(template_path, &None, &tmp_dir, &private_key_path, false).unwrap();
        fs::remove_dir_all(&tmp_dir).unwrap();
    }
}
