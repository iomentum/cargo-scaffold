use anyhow::{anyhow, Result};
use console::{Emoji, Style};
use std::path::Path;

pub(crate) fn clone(
    repository: &str,
    reference_opt: Option<&str>,
    target_dir: &Path,
    private_key_path: Option<&Path>,
) -> Result<()> {
    let cyan = Style::new().cyan();
    println!(
        "{} {}",
        Emoji("🔄", ""),
        cyan.apply_to("Cloning repository…"),
    );

    let mut auth = auth_git2::GitAuthenticator::default();
    if let Some(private_key_path) = private_key_path {
        auth = auth.add_ssh_key_from_file(private_key_path, None)
    }

    let git_config = git2::Config::open_default()
        .map_err(|e| anyhow!(e).context("Opening git configuration"))?;

    let mut fetch_options = git2::FetchOptions::new();

    // Add credentials callback.
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(auth.credentials(&git_config));
    fetch_options.remote_callbacks(callbacks);

    if reference_opt.is_some() {
        fetch_options.download_tags(git2::AutotagOption::All);
    }

    // Prepare builder.
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_options);

    // Clone the project.
    let repo = builder.clone(repository, target_dir)?;

    // Either a git tag, commit
    if let Some(git_reference) = reference_opt {
        match repo.revparse_ext(git_reference) {
            Ok((obj, reference)) => {
                repo.checkout_tree(&obj, None)?;
                match reference {
                    // tagref is an actual reference like branches or tags
                    Some(reporef) => repo.set_head(reporef.name().expect("tag has a name; qed")),
                    // this is a commit, not a reference
                    None => repo.set_head_detached(obj.id()),
                }?;
            }
            Err(_) => {
                // It might be a branch
                std::fs::remove_dir_all(target_dir)?;
                builder.branch(git_reference);
                let _repo = builder.clone(repository, target_dir)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn clone_http() {
        let template_path = "https://github.com/http-rs/surf.git";
        let tmp_dir = tempdir().unwrap();
        clone(template_path, None, tmp_dir.path(), None).unwrap();
    }

    #[test]
    fn clone_http_commit() {
        let commit = Some("8f0039488b3877ca59592900bc7ad645a83e2886");
        let template_path = "https://github.com/http-rs/surf.git";
        let tmp_dir = tempdir().unwrap();
        clone(template_path, commit, tmp_dir.path(), None).unwrap();
    }

    #[test]
    fn clone_http_branch() {
        let branch = Some("main");
        let template_path = "https://github.com/apollographql/router.git";
        let tmp_dir = tempdir().unwrap();
        clone(template_path, branch, tmp_dir.path(), None).unwrap();
    }

    #[test]
    // warn: your ssh key must be in pem format
    fn clone_ssh() {
        let template_path = "git@github.com:http-rs/surf.git";
        let tmp_dir = tempdir().unwrap();
        clone(template_path, None, tmp_dir.path(), None).unwrap();
    }

    #[test]
    // warn: your ssh key must be in pem format
    fn clone_ssh_commit() {
        let commit = Some("8f0039488b3877ca59592900bc7ad645a83e2886");
        let template_path = "git@github.com:http-rs/surf.git";
        let tmp_dir = tempdir().unwrap();
        clone(template_path, commit, tmp_dir.path(), None).unwrap();
    }
}
