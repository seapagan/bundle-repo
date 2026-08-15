use git2::{
    Cred, ErrorClass, ErrorCode, FetchOptions, RemoteCallbacks, Repository,
};
use regex::Regex;
use std::io::Write;
use std::path::{Path, PathBuf};
use url::Url;

use crate::{progress::ProgressReporter, structs::Params};

pub fn clone_repo<N: Write, D: Write>(
    flags: &Params,
    repo_input: &str,
    token: Option<&str>,
    temp_dir_path: &Path,
    reporter: &mut ProgressReporter<N, D>,
) -> Result<PathBuf, git2::Error> {
    reporter.phase("Cloning repository...").unwrap();

    let repo_url = if is_valid_url(repo_input) {
        repo_input.to_string()
    } else if is_valid_shorthand(repo_input) {
        format!("https://github.com/{}.git", repo_input)
    } else {
        return Err(git2::Error::from_str("Invalid repository shorthand"));
    };

    let repo_folder = temp_dir_path.join("repo_clone");

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
        if let Some(token) = token {
            Cred::userpass_plaintext("oauth2", token)
        } else {
            Cred::userpass_plaintext("", "")
        }
    });

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks).depth(1);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_options);

    if let Some(branch_name) = &flags.branch {
        builder.branch(branch_name);
        reporter
            .phase_with_accent("Checking out branch: ", branch_name, "")
            .unwrap();
    }

    match builder.clone(&repo_url, &repo_folder) {
        Ok(_) => {
            reporter
                .clone_success(
                    repo_url.trim_end_matches(".git"),
                    flags.branch.as_deref(),
                )
                .unwrap();
            Ok(repo_folder)
        }
        Err(error) => Err(git2::Error::from_str(&clone_error_message(
            repo_input,
            flags.branch.as_deref(),
            &error,
        ))),
    }
}

fn clone_error_message(
    repo_input: &str,
    branch_name: Option<&str>,
    error: &git2::Error,
) -> String {
    match (error.class(), error.code()) {
        (ErrorClass::Reference, ErrorCode::NotFound) => {
            if let Some(branch_name) = branch_name {
                format!(
                    "The specified branch '{branch_name}' does not exist in the repository."
                )
            } else {
                format!("Failed to clone: {error}")
            }
        }
        (ErrorClass::Net, _) => format!(
            "Network error: The repository '{repo_input}' might not exist or you may not have permission to access it."
        ),
        (ErrorClass::Http, _)
            if error
                .message()
                .contains("too many redirects or authentication replays") =>
        {
            format!(
                "The repository '{repo_input}' does not exist or requires authentication.\nIf it's a private repository, please provide a valid token using the --token option."
            )
        }
        _ => format!("Failed to clone: {error}"),
    }
}

pub fn is_valid_url(input: &str) -> bool {
    Url::parse(input).is_ok()
}

pub fn is_valid_shorthand(input: &str) -> bool {
    let re = Regex::new(r"^[\w\-]+/[\w\-]+$").unwrap();
    re.is_match(input)
}

pub(crate) fn check_repository_at<N: Write, D: Write>(
    path: &Path,
    reporter: &mut ProgressReporter<N, D>,
) -> Result<(), git2::Error> {
    match Repository::discover(path) {
        Ok(repo) => {
            let repo_path =
                repo.path().parent().unwrap().display().to_string();
            reporter.repository_found(&repo_path, || {
                get_current_branch_name(&repo)
            })?;
            Ok(())
        }
        Err(_) => {
            reporter
                .always_visible_diagnostic(
                    "X  No git repository found in the current directory.",
                )
                .unwrap();
            Err(git2::Error::from_str("Not a git repository"))
        }
    }
}

fn get_current_branch_name(repo: &Repository) -> Result<String, git2::Error> {
    if repo.head_detached()? {
        return Ok("detached HEAD".to_string());
    }

    let head = repo.head()?;
    Ok(head.shorthand()?.to_string())
}

#[cfg(test)]
#[path = "../tests/crate/repo.rs"]
mod tests;
