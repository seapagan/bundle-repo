use clap::Parser;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use ignore::WalkBuilder;
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
        match clone_repo(&repo_input, args.token.as_deref()) {
            Ok(repo_folder) => {
                // List files in the cloned repository
                let files = list_files_in_repo(&repo_folder);
                for file in files {
                    println!("{}", file);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    } else {
        // Handle the case where no arguments are provided, check if the current directory is a repo
        if let Err(e) = check_current_directory() {
            eprintln!("Error: {}", e);
        } else {
            // If successful, list files in the current directory
            let repo_path = PathBuf::from("."); // Current directory
            let files = list_files_in_repo(&repo_path);
            for file in files {
                println!("{}", file);
            }
        }
    }
}

/// Validates if the input is a valid URL or a shorthand and clones the repository accordingly.
fn clone_repo(repo_input: &str, token: Option<&str>) -> Result<PathBuf, git2::Error> {
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
        Ok(_) => {
            println!("Successfully cloned into {}", repo_folder.display());
            Ok(repo_folder) // Return the repo folder
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

/// Function to list all files in the repository while ignoring `.git`, `.github`, and respecting `.gitignore`,
/// as well as ignoring custom file patterns.
fn list_files_in_repo(repo_path: &PathBuf) -> Vec<String> {
    let mut file_list = Vec::new();

    // Define the static list of custom files and folders to ignore (case-insensitive)
    let ignore_patterns = vec![
        r"(?i)\.gitignore",        // .gitignore (case-insensitive)
        r"(?i)renovate\.json",     // renovate.json (case-insensitive)
        r"(?i)requirement.*\.txt", // requirement*.txt (case-insensitive)
        r"(?i)\.lock$",            // *.lock (case-insensitive)
        r"(?i)license(\..*)?",     // license*.* (or without extension, case-insensitive)
        r"(?i)todo\..*",           // todo.* (case-insensitive)
        r"(?i)\.github",           // .github folder (case-insensitive)
        r"(?i)\.git",              // .git folder (case-insensitive)
    ];
    let regex_list: Vec<Regex> = ignore_patterns
        .into_iter()
        .map(|pattern| Regex::new(pattern).unwrap())
        .collect();

    // Create a walker that respects .gitignore
    let walker = WalkBuilder::new(repo_path)
        .hidden(false) // Include hidden files unless excluded by .gitignore
        .git_ignore(true) // Enable .gitignore
        .git_exclude(true) // Respect global gitignore
        .git_global(true) // Respect user-level .gitignore
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let path = entry.path();

                // Check if the path is within a folder we want to exclude (e.g., `.git` or `.github`)
                if let Ok(relative_path) = path.strip_prefix(repo_path) {
                    let relative_path_str = relative_path.to_string_lossy().to_string();

                    // Check if the path matches any folder or file ignore patterns
                    if regex_list.iter().any(|re| re.is_match(&relative_path_str)) {
                        continue; // Skip if it matches the ignore pattern
                    }
                }

                // Only store files
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    file_list.push(
                        path.strip_prefix(repo_path)
                            .unwrap()
                            .to_string_lossy()
                            .to_string(),
                    );
                }
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }

    file_list
}
