use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use regex::Regex;
use std::path::PathBuf;
use tempfile::tempdir;
use url::Url;

pub fn clone_repo(
    repo_input: &str,
    token: Option<&str>,
) -> Result<PathBuf, git2::Error> {
    let repo_url = if is_valid_url(repo_input) {
        repo_input.to_string()
    } else if is_valid_shorthand(repo_input) {
        format!("https://github.com/{}.git", repo_input)
    } else {
        eprintln!("Invalid repository shorthand.");
        return Err(git2::Error::from_str("Invalid repository shorthand"));
    };

    let temp_dir = tempdir().unwrap();
    let mut repo_folder = PathBuf::from(temp_dir.path());
    repo_folder.push("repo_clone");

    let _persisted_dir = temp_dir.into_path();

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

    match builder.clone(&repo_url, &repo_folder) {
        Ok(_) => {
            println!("Successfully cloned into {}", repo_folder.display());
            Ok(repo_folder)
        }
        Err(e) => {
            eprintln!("Failed to clone: {}", e);
            Err(e)
        }
    }
}

pub fn is_valid_url(input: &str) -> bool {
    Url::parse(input).is_ok()
}

pub fn is_valid_shorthand(input: &str) -> bool {
    let re = Regex::new(r"^[\w\-]+/[\w\-]+$").unwrap();
    re.is_match(input)
}

pub fn check_current_directory() -> Result<(), git2::Error> {
    match Repository::discover(".") {
        Ok(repo) => {
            println!(
                "Found a git repository in the current directory: {}",
                repo.path().display()
            );
            Ok(())
        }
        Err(_) => {
            eprintln!("No git repository found in the current directory.");
            Err(git2::Error::from_str("Not a git repository"))
        }
    }
}
