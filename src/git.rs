use anyhow::Result;
use console::{Emoji, Style};
use dialoguer::Password;
use git2::{Cred, Oid, RemoteCallbacks, Repository};
use std::path::Path;

pub(crate) fn clone(
    repository: &str,
    commit_opt: &Option<String>,
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
    let repo = if repository.contains("http") {
        Repository::clone(repository, &target_dir)?
    } else {
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
        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(callbacks);

        // Prepare builder.
        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fo);

        // Clone the project.
        builder.clone(repository, target_dir)?
    };

    // move cloned repo to specified commit
    if let Some(commit) = commit_opt {
        let oid = Oid::from_str(commit)?;
        let commit_obj = repo.find_commit(oid)?;
        let _branch = repo.branch(commit, &commit_obj, false)?;
        let obj = repo.revparse_single(&("refs/heads/".to_owned() + commit))?;
        repo.checkout_tree(&obj, None)?;
        repo.set_head(&("refs/heads/".to_owned() + commit))?;
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
