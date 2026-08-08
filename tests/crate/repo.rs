use super::*;
use git2::{Error, ErrorClass, ErrorCode, Oid, Signature};
use std::fs;
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

#[test]
fn test_repository_check_discovers_from_explicit_nested_path() {
    let temp_dir = tempdir().unwrap();
    let repo = Repository::init(temp_dir.path()).unwrap();
    repo.set_head("refs/heads/test-branch").unwrap();
    create_commit(&repo);
    let nested_path = temp_dir.path().join("nested/directory");
    fs::create_dir_all(&nested_path).unwrap();
    let params = Params {
        stdout: true,
        ..Params::default()
    };

    assert!(check_repository_at(&nested_path, &params).is_ok());
}

#[test]
fn test_repository_check_rejects_explicit_non_repository_path() {
    let temp_dir = tempdir().unwrap();
    let params = Params {
        stdout: true,
        ..Params::default()
    };

    let error = check_repository_at(temp_dir.path(), &params).unwrap_err();

    assert_eq!(error.message(), "Not a git repository");
}

#[test]
fn test_clone_repo_rejects_invalid_repository_input() {
    let destination_dir = tempdir().unwrap();
    let params = Params {
        stdout: true,
        ..Params::default()
    };

    let error =
        clone_repo(&params, "not a repository", None, destination_dir.path())
            .unwrap_err();

    assert_eq!(error.message(), "Invalid repository shorthand");
}

#[test]
fn test_clone_error_reports_missing_requested_branch() {
    let missing_reference = Error::new(
        ErrorCode::NotFound,
        ErrorClass::Reference,
        "reference not found",
    );
    assert_eq!(
        clone_error_message("owner/repo", Some("missing"), &missing_reference),
        "The specified branch 'missing' does not exist in the repository."
    );
    assert_eq!(
        clone_error_message("owner/repo", None, &missing_reference),
        format!("Failed to clone: {missing_reference}")
    );
}

#[test]
fn test_clone_error_reports_network_context() {
    let network =
        Error::new(ErrorCode::GenericError, ErrorClass::Net, "offline");
    assert_eq!(
        clone_error_message("owner/repo", None, &network),
        "Network error: The repository 'owner/repo' might not exist or you may not have permission to access it."
    );
}

#[test]
fn test_clone_error_reports_authentication_guidance() {
    let authentication = Error::new(
        ErrorCode::Auth,
        ErrorClass::Http,
        "too many redirects or authentication replays",
    );
    assert_eq!(
        clone_error_message("owner/private", None, &authentication),
        "The repository 'owner/private' does not exist or requires authentication.\nIf it's a private repository, please provide a valid token using the --token option."
    );
}

#[test]
fn test_clone_error_preserves_unexpected_details() {
    let unexpected =
        Error::new(ErrorCode::GenericError, ErrorClass::Os, "disk full");
    assert_eq!(
        clone_error_message("owner/repo", None, &unexpected),
        format!("Failed to clone: {unexpected}")
    );
}
