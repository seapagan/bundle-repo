use ignore::WalkBuilder;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Component, PathBuf};

#[derive(Default)]
pub struct FolderNode {
    pub files: Vec<String>,
    pub subfolders: HashMap<String, FolderNode>,
}

#[derive(Default)]
pub struct FileTree {
    pub folder_node: FolderNode,
    pub file_paths: Vec<String>, // Add a list to track file paths for <repository_files>
}

pub fn list_files_in_repo(
    repo_path: &PathBuf,
    extend_exclude: Option<&[String]>,
    exclude: Option<&[String]>,
) -> Vec<String> {
    let mut file_list = Vec::new();

    // Initialize ignore patterns based on whether exclude is set
    let ignore_patterns: Vec<String> = if let Some(patterns) = exclude {
        // If exclude is set, use only those patterns
        patterns
            .iter()
            .map(|p| {
                let escaped = regex::escape(p);
                format!(r"(?i){}", escaped)
            })
            .collect()
    } else {
        // Otherwise use default patterns
        let mut patterns: Vec<String> = vec![
            r"(?i)\.gitignore",
            r"(?i)renovate\.json",
            r"(?i)requirement.*\.txt",
            r"(?i)\.lock$",
            r"(?i)license(\..*)?",
            r"(?i)\.github",
            r"(?i)\.git",
            r"(?i)\.vscode",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        // Add additional patterns if provided
        if let Some(extend_patterns) = extend_exclude {
            patterns.extend(extend_patterns.iter().map(|p| {
                let escaped = regex::escape(p);
                format!(r"(?i){}", escaped)
            }));
        }
        patterns
    };

    let regex_list: Vec<Regex> = ignore_patterns
        .iter()
        .map(|pattern| {
            Regex::new(pattern).unwrap_or_else(|e| {
                eprintln!(
                    "Warning: Invalid regex pattern '{}': {}",
                    pattern, e
                );
                Regex::new(r"^$").unwrap()
            })
        })
        .collect();

    let walker = WalkBuilder::new(repo_path)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                    continue;
                }

                let path = entry.path();
                let relative_path = match path.strip_prefix(repo_path) {
                    Ok(p) => p.to_string_lossy().to_string(),
                    Err(_) => continue,
                };

                // Skip if the file matches any of our patterns
                if regex_list.iter().any(|re| re.is_match(&relative_path)) {
                    continue;
                }

                file_list.push(relative_path);
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }

    file_list
}

pub fn group_files_by_directory(file_list: Vec<String>) -> FileTree {
    let mut root = FolderNode::default();
    let mut file_paths = Vec::new(); // To store the relative paths of each file

    for file_path in file_list {
        let path = PathBuf::from(&file_path);
        let path_components: Vec<Component> = path.components().collect();

        let mut current_node = &mut root;
        for component in path_components.iter().take(path_components.len() - 1)
        {
            let folder_name =
                component.as_os_str().to_string_lossy().to_string();
            current_node =
                current_node.subfolders.entry(folder_name).or_default();
        }

        if let Some(file_name) = path_components.last() {
            current_node
                .files
                .push(file_name.as_os_str().to_string_lossy().to_string());
            file_paths.push(file_path); // Store the full relative path
        }
    }

    FileTree {
        folder_node: root,
        file_paths,
    }
}

#[cfg(test)]
mod tests {
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

        let files =
            list_files_in_repo(&temp_dir.path().to_path_buf(), None, None);

        assert_eq!(files.len(), 3);
        assert!(files.contains(&"file1.txt".to_string()));
        assert!(files.contains(&"src/file2.rs".to_string()));
        assert!(files.contains(&"src/nested/file3.rs".to_string()));
    }

    #[test]
    fn test_list_files_with_exclude() {
        let temp_dir = TempDir::new().unwrap();
        let test_files =
            ["file1.txt", "src/file2.rs", "test.lock", ".gitignore"];
        create_test_files(&temp_dir, &test_files);

        let files =
            list_files_in_repo(&temp_dir.path().to_path_buf(), None, None);

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
            ".github/workflows/test.yml",
            ".vscode/settings.json",
        ];
        create_test_files(&temp_dir, &test_files);

        let files =
            list_files_in_repo(&temp_dir.path().to_path_buf(), None, None);

        // Only file1.txt should remain, all others should be excluded by default patterns
        assert_eq!(files.len(), 1);
        assert!(files.contains(&"file1.txt".to_string()));
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
        assert!(file_tree
            .file_paths
            .contains(&"src/nested/file3.rs".to_string()));
        assert!(file_tree
            .file_paths
            .contains(&"src/nested/deep/file4.rs".to_string()));
    }
}
