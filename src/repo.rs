use git2::{
    Cred, ErrorClass, ErrorCode, FetchOptions, RemoteCallbacks, Repository,
};
use regex::Regex;
use std::path::{Path, PathBuf};
use url::Url;

use crate::structs::Params;

pub fn clone_repo(
    flags: &Params,
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

pub fn check_current_directory(flags: &Params) -> Result<(), git2::Error> {
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
    if repo.head_detached()? {
        return Ok("detached HEAD".to_string());
    }

    let head = repo.head()?;
    Ok(head.shorthand()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Oid, Signature};
    use tempfile::tempdir;

    fn create_commit(repo: &Repository) -> Oid {
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = Signature::now("Test", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "test", &tree, &[])
            .unwrap()
    }

    #[test]
    fn test_current_branch_name() {
        let temp_dir = tempdir().unwrap();
        let repo = Repository::init(temp_dir.path()).unwrap();
        repo.set_head("refs/heads/test-branch").unwrap();
        create_commit(&repo);

        assert_eq!(get_current_branch_name(&repo).unwrap(), "test-branch");
    }

    #[test]
    fn test_detached_head_name() {
        let temp_dir = tempdir().unwrap();
        let repo = Repository::init(temp_dir.path()).unwrap();
        let commit_id = create_commit(&repo);
        repo.set_head_detached(commit_id).unwrap();

        assert_eq!(get_current_branch_name(&repo).unwrap(), "detached HEAD");
    }
}
