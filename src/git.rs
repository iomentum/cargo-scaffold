use anyhow::Result;
use dialoguer::Password;
use git2::{Cred, RemoteCallbacks, Repository};
use std::env;
use std::path::{Path, PathBuf};

pub(crate) fn clone(repository: &str, target_dir: &PathBuf, passphrase_needed: bool) -> Result<()> {
    if repository.contains("http") {
        Repository::clone(repository, &target_dir)?;
    } else {
        let mut callbacks = RemoteCallbacks::new();
        let passphrase = if passphrase_needed {
            Password::new()
                .with_prompt("Enter passphrase for .ssh/id_rsa")
                .interact()?
                .into()
        } else {
            None
        };
        callbacks.credentials(move |_url, username_from_url, _allowed_types| {
            Cred::ssh_key(
                username_from_url.unwrap(),
                None,
                std::path::Path::new(&format!(
                    "{}/.ssh/id_rsa", // TODO: add flag to specify
                    env::var("HOME").expect("cannot fetch $HOME")
                )),
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
        builder.clone(repository, &target_dir)?;
    }

    Ok(())
}
