use super::*;
use std::fs::{self, File};
use tempfile::TempDir;

fn create_test_files(temp_dir: &TempDir, files: &[&str]) {
    for file in files {
        let path = temp_dir.path().join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        File::create(path).unwrap();
    }
}

#[test]
fn test_list_files_basic() {
    let temp_dir = TempDir::new().unwrap();
    let test_files = ["file1.txt", "src/file2.rs", "src/nested/file3.rs"];
    create_test_files(&temp_dir, &test_files);

    let files = list_files_in_repo(&temp_dir.path().to_path_buf(), None, None);

    assert_eq!(files.len(), 3);
    assert!(files.contains(&"file1.txt".to_string()));
    assert!(files.contains(&"src/file2.rs".to_string()));
    assert!(files.contains(&"src/nested/file3.rs".to_string()));
}

#[test]
fn test_list_files_with_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let test_files = ["file1.txt", "src/file2.rs", "test.lock", ".gitignore"];
    create_test_files(&temp_dir, &test_files);

    let files = list_files_in_repo(&temp_dir.path().to_path_buf(), None, None);

    assert_eq!(files.len(), 2);
    assert!(files.contains(&"file1.txt".to_string()));
    assert!(files.contains(&"src/file2.rs".to_string()));
    assert!(!files.contains(&"test.lock".to_string()));
    assert!(!files.contains(&".gitignore".to_string()));
}

#[test]
fn test_list_files_with_custom_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let test_files = ["file1.txt", "src/file2.rs", "exclude_me.txt"];
    create_test_files(&temp_dir, &test_files);

    let exclude = vec!["exclude_me.txt".to_string()];
    let files = list_files_in_repo(
        &temp_dir.path().to_path_buf(),
        None,
        Some(&exclude),
    );

    assert_eq!(files.len(), 2);
    assert!(files.contains(&"file1.txt".to_string()));
    assert!(files.contains(&"src/file2.rs".to_string()));
    assert!(!files.contains(&"exclude_me.txt".to_string()));
}

#[test]
fn test_list_files_with_extend_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let test_files = [
        "file1.txt",
        "src/file2.rs",
        "custom_exclude.txt",
        ".gitignore",
    ];
    create_test_files(&temp_dir, &test_files);

    let extend_exclude = vec!["custom_exclude.txt".to_string()];
    let files = list_files_in_repo(
        &temp_dir.path().to_path_buf(),
        Some(&extend_exclude),
        None,
    );

    assert_eq!(files.len(), 2);
    assert!(files.contains(&"file1.txt".to_string()));
    assert!(files.contains(&"src/file2.rs".to_string()));
    assert!(!files.contains(&"custom_exclude.txt".to_string()));
    assert!(!files.contains(&".gitignore".to_string()));
}

#[cfg(windows)]
#[test]
fn test_windows_exclude_accepts_backslash_paths() {
    let temp_dir = TempDir::new().unwrap();
    let test_files = ["file1.txt", "src/file2.rs"];
    create_test_files(&temp_dir, &test_files);

    let exclude = vec![r"src\file2.rs".to_string()];
    let files = list_files_in_repo(
        &temp_dir.path().to_path_buf(),
        None,
        Some(&exclude),
    );

    assert_eq!(files, vec!["file1.txt"]);
}

#[test]
fn test_default_exclude_patterns() {
    let temp_dir = TempDir::new().unwrap();
    let test_files = [
        "file1.txt",
        ".git/config",
        ".gitignore",
        "renovate.json",
        "requirements.txt",
        "Cargo.lock",
        "LICENSE",
        "LICENSE.txt",
        "LICENCE",
        "Licence.md",
        "license.rst",
        "licence",
        ".github/workflows/test.yml",
        ".vscode/settings.json",
    ];
    create_test_files(&temp_dir, &test_files);

    let files = list_files_in_repo(&temp_dir.path().to_path_buf(), None, None);

    // Only file1.txt should remain, all others should be excluded by default patterns
    assert_eq!(files.len(), 1);
    assert!(files.contains(&"file1.txt".to_string()));

    // Explicitly verify license/licence variations are excluded
    let license_variations = [
        "LICENSE",
        "LICENSE.txt",
        "LICENCE",
        "Licence.md",
        "license.rst",
        "licence",
    ];
    for variant in license_variations {
        assert!(
            !files.contains(&variant.to_string()),
            "Failed to exclude license variant: {}",
            variant
        );
    }
}

#[test]
fn test_group_files_by_directory() {
    let files = vec![
        "file1.txt".to_string(),
        "src/file2.rs".to_string(),
        "src/nested/file3.rs".to_string(),
        "src/nested/deep/file4.rs".to_string(),
    ];

    let file_tree = group_files_by_directory(files);

    // Test root level
    assert_eq!(file_tree.folder_node.files, vec!["file1.txt"]);

    // Test src directory
    let src_folder = file_tree.folder_node.subfolders.get("src").unwrap();
    assert_eq!(src_folder.files, vec!["file2.rs"]);

    // Test nested directory
    let nested_folder = src_folder.subfolders.get("nested").unwrap();
    assert_eq!(nested_folder.files, vec!["file3.rs"]);

    // Test deep directory
    let deep_folder = nested_folder.subfolders.get("deep").unwrap();
    assert_eq!(deep_folder.files, vec!["file4.rs"]);

    // Test file_paths
    assert_eq!(file_tree.file_paths.len(), 4);
    assert!(file_tree.file_paths.contains(&"file1.txt".to_string()));
    assert!(file_tree.file_paths.contains(&"src/file2.rs".to_string()));
    assert!(
        file_tree
            .file_paths
            .contains(&"src/nested/file3.rs".to_string())
    );
    assert!(
        file_tree
            .file_paths
            .contains(&"src/nested/deep/file4.rs".to_string())
    );
}
