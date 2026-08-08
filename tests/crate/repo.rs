use super::clone_error_message;
use git2::{Error, ErrorClass, ErrorCode};

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
        "Failed to clone: reference not found; class=Reference (4); code=NotFound (-3)"
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
        "Failed to clone: disk full; class=Os (2)"
    );
}
