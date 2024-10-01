use clap::Parser;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use regex::Regex;
use std::path::PathBuf;
use tempfile::tempdir;
use url::Url;

/// Simple program to clone a GitHub repo or check if the current folder is a repo
#[derive(Parser)]
#[command(name = "Repopack Clone Tool")]
#[command(author = "Your Name")]
#[command(version = "1.0")]
#[command(about = "Clone a repo or check if the current folder is a Git repo", long_about = None)]
struct Cli {
    /// GitHub repo URL or shorthand (user/repo)
    repo: Option<String>,

    /// GitHub Personal Access Token for authentication
    #[arg(short, long)]
    token: Option<String>,
}

fn main() {
    let args = Cli::parse();

    if let Some(repo_input) = args.repo {
        // Handle the case where the user specifies a repo to clone
        if let Err(e) = clone_repo(&repo_input, args.token.as_deref()) {
            eprintln!("Error: {}", e);
        }
    } else {
        // Handle the case where no arguments are provided, check if the current directory is a repo
        if let Err(e) = check_current_directory() {
            eprintln!("Error: {}", e);
        }
    }
}

/// Validates if the input is a valid URL or a shorthand and clones the repository accordingly.
fn clone_repo(repo_input: &str, token: Option<&str>) -> Result<(), git2::Error> {
    let repo_url = if is_valid_url(repo_input) {
        repo_input.to_string()
    } else if is_valid_shorthand(repo_input) {
        format!("https://github.com/{}.git", repo_input)
    } else {
        eprintln!("Invalid repository shorthand. Expected format: user/repo");
        return Err(git2::Error::from_str("Invalid repository shorthand"));
    };

    let temp_dir = tempdir().unwrap(); // Create a temp directory.
    let mut repo_folder = PathBuf::from(temp_dir.path());
    repo_folder.push("repo_clone"); // Create subfolder.

    let _persisted_dir = temp_dir.into_path(); // Prevent deletion of the temp directory.

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
        if let Some(token) = token {
            // Use the provided token for HTTPS cloning
            Cred::userpass_plaintext("oauth2", token)
        } else {
            // Provide empty credentials for public repositories if no token is provided
            Cred::userpass_plaintext("", "")
        }
    });

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks).depth(1); // Add the callback to fetch options

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_options);

    match builder.clone(&repo_url, &repo_folder) {
        Ok(repo) => {
            println!("Successfully cloned into {}", repo.path().display());
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to clone: {}", e);
            Err(e)
        }
    }
}

/// Validates if the input is a valid URL.
fn is_valid_url(input: &str) -> bool {
    Url::parse(input).is_ok()
}

/// Validates if the input follows the 'user/repo' shorthand format.
fn is_valid_shorthand(input: &str) -> bool {
    let re = Regex::new(r"^[\w\-]+/[\w\-]+$").unwrap();
    re.is_match(input)
}

/// Checks if the current directory is a Git repository.
fn check_current_directory() -> Result<(), git2::Error> {
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
