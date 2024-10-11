use git2::{
    Cred, ErrorClass, ErrorCode, FetchOptions, RemoteCallbacks, Repository,
};
use regex::Regex;
use std::path::{Path, PathBuf};
use url::Url;

use crate::cli::Flags;

pub fn clone_repo(
    flags: &Flags,
    repo_input: &str,
    token: Option<&str>,
    temp_dir_path: &Path,
) -> Result<PathBuf, git2::Error> {
    if !flags.stdout {
        println!("-> Cloning repository...");
    }

    let repo_url = if is_valid_url(repo_input) {
        repo_input.to_string()
    } else if is_valid_shorthand(repo_input) {
        format!("https://github.com/{}.git", repo_input)
    } else {
        eprintln!("Invalid repository shorthand.");
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
        builder.branch(&branch_name);
        if !flags.stdout {
            println!("-> Checking out branch: {}", branch_name);
        }
    }

    match builder.clone(&repo_url, &repo_folder) {
        Ok(_) => {
            if !flags.stdout {
                println!(
                    "-> Successfully cloned repository '{}'{}",
                    &repo_url.trim_end_matches(".git"),
                    flags.branch.as_ref().map_or(String::new(), |b| format!(
                        " (branch: {})",
                        b
                    ))
                );
            }
            Ok(repo_folder)
        }
        Err(e) => {
            let error_message = match (e.class(), e.code()) {
            (ErrorClass::Reference, ErrorCode::NotFound) => {
                if flags.branch.is_some() {
                    format!("The specified branch '{}' does not exist in the repository.", flags.branch.as_ref().unwrap())
                } else {
                    format!("Failed to clone: {}", e)
                }
            },
            (ErrorClass::Net, _) => format!("Network error: The repository '{}' might not exist or you may not have permission to access it.", repo_input),
            (ErrorClass::Http, _) if e.message().contains("too many redirects or authentication replays") => {
                format!("The repository '{}' does not exist or requires authentication.\nIf it's a private repository, please provide a valid token using the --token option.", repo_input)
            },
            _ => format!("Failed to clone: {}", e),
        };
            Err(git2::Error::from_str(&error_message))
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

pub fn check_current_directory(flags: &Flags) -> Result<(), git2::Error> {
    match Repository::discover(".") {
        Ok(repo) => {
            if !flags.stdout {
                let repo_path = repo.path().parent().unwrap().display();
                let branch_name = get_current_branch_name(&repo)?;
                println!(
                    "-> Found a git repository in the current directory: '{}' (branch: {})",
                    repo_path, branch_name
                );
            }
            Ok(())
        }
        Err(_) => {
            eprintln!("X  No git repository found in the current directory.");
            Err(git2::Error::from_str("Not a git repository"))
        }
    }
}

fn get_current_branch_name(repo: &Repository) -> Result<String, git2::Error> {
    let head = repo.head()?;
    if let Some(name) = head.shorthand() {
        Ok(name.to_string())
    } else {
        Ok("detached HEAD".to_string())
    }
}
