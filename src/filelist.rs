use ignore::WalkBuilder;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Component, PathBuf};

#[derive(Default)]
pub struct FolderNode {
    pub files: Vec<String>,
    pub subfolders: HashMap<String, FolderNode>,
}

pub struct FileTree {
    pub folder_node: FolderNode,
    pub file_paths: Vec<String>, // Add a list to track file paths for <repository_files>
}

pub fn list_files_in_repo(repo_path: &PathBuf) -> Vec<String> {
    let mut file_list = Vec::new();

    let ignore_patterns = vec![
        r"(?i)\.gitignore",
        r"(?i)renovate\.json",
        r"(?i)requirement.*\.txt",
        r"(?i)\.lock$",
        r"(?i)license(\..*)?",
        // r"(?i)todo\..*",
        r"(?i)\.github",
        r"(?i)\.git",
        r"(?i)\.vscode",
    ];
    let regex_list: Vec<Regex> = ignore_patterns
        .into_iter()
        .map(|pattern| Regex::new(pattern).unwrap())
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
                let path = entry.path();
                if let Ok(relative_path) = path.strip_prefix(repo_path) {
                    let relative_path_str =
                        relative_path.to_string_lossy().to_string();
                    if regex_list
                        .iter()
                        .any(|re| re.is_match(&relative_path_str))
                    {
                        continue;
                    }
                }

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
