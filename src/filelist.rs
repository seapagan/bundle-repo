use casefold::simple_fold;
use glob::{MatchOptions, Pattern};
use ignore::WalkBuilder;
use regex::Regex;
use same_file::Handle;
use std::collections::{BTreeSet, HashMap};
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::progress::ProgressReporter;

const LEGACY_EXCLUDE_PATTERNS: [&str; 7] = [
    r"(?i)\.gitignore",
    r"(?i)renovate\.json",
    r"(?i)requirement.*\.txt",
    r"(?i)\.lock$",
    r"(?i)licen[cs]e(\..*)?",
    r"(?i)\.github",
    r"(?i)\.vscode",
];

pub struct FileSelectionOptions<'a> {
    pub extend_exclude: Option<&'a [String]>,
    pub exclude: Option<&'a [String]>,
    pub include: Option<&'a [String]>,
    pub legacy_excludes: bool,
}

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

fn repository_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

struct ExclusionMatcher {
    legacy_patterns: Vec<Regex>,
    custom_patterns: Vec<Pattern>,
}

impl ExclusionMatcher {
    fn new(
        legacy_excludes: bool,
        extend_exclude: Option<&[String]>,
        exclude: Option<&[String]>,
    ) -> Result<Self, String> {
        let patterns = if let Some(patterns) = exclude {
            patterns
        } else {
            extend_exclude.unwrap_or_default()
        };
        let mut custom_patterns = Vec::new();
        for pattern in patterns {
            for normalized in exclusion_glob_variants(pattern) {
                let folded = simple_fold(normalized);
                let glob = Pattern::new(&folded).map_err(|error| {
                    format!("invalid exclusion glob '{pattern}': {error}")
                })?;
                custom_patterns.push(glob);
            }
        }

        Ok(Self {
            legacy_patterns: if legacy_excludes && exclude.is_none() {
                LEGACY_EXCLUDE_PATTERNS
                    .into_iter()
                    .map(|pattern| Regex::new(pattern).unwrap())
                    .collect()
            } else {
                Vec::new()
            },
            custom_patterns,
        })
    }

    fn matches_file(&self, repository_path: &str) -> bool {
        self.legacy_patterns
            .iter()
            .any(|pattern| pattern.is_match(repository_path))
            || self.matches_custom(repository_path)
    }

    fn matches_directory(&self, repository_path: &str) -> bool {
        self.matches_custom(repository_path)
    }

    fn matches_custom(&self, repository_path: &str) -> bool {
        let folded = simple_fold(repository_path.to_string());
        let options = MatchOptions {
            case_sensitive: true,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        };
        self.custom_patterns
            .iter()
            .any(|pattern| pattern.matches_with(&folded, options))
    }
}

fn exclusion_glob_variants(pattern: &str) -> Vec<String> {
    let normalized = pattern.replace('\\', "/");
    if normalized.ends_with('/') {
        let directory = normalized.trim_end_matches('/');
        return vec![directory.to_string(), format!("{directory}/**")];
    }
    if normalized.contains('/') {
        vec![normalized]
    } else {
        vec![format!("**/{normalized}")]
    }
}

#[cfg(windows)]
fn is_git_component(component: &OsStr) -> bool {
    component.to_string_lossy().eq_ignore_ascii_case(".git")
}

#[cfg(not(windows))]
fn is_git_component(component: &OsStr) -> bool {
    component == ".git"
}

fn has_git_component(path: &Path) -> bool {
    path.components()
        .any(|component| is_git_component(component.as_os_str()))
}

#[derive(Clone)]
struct IncludeSelector {
    path: String,
    directory: bool,
}

fn normalize_include(selector: &str) -> Result<String, String> {
    let normalized = selector.replace('\\', "/");
    let windows_absolute = normalized.as_bytes().get(1) == Some(&b':')
        && normalized.as_bytes().get(2) == Some(&b'/')
        && normalized
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic);
    if normalized.starts_with('/') || windows_absolute {
        return Err(format!(
            "invalid include path '{selector}': path must be repository-relative"
        ));
    }

    let mut components = Vec::new();
    for component in normalized.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err(format!(
                    "invalid include path '{selector}': parent traversal is not allowed"
                ));
            }
            component if is_git_component(OsStr::new(component)) => {
                return Err(format!(
                    "invalid include path '{selector}': .git metadata cannot be included"
                ));
            }
            component => components.push(component),
        }
    }
    if components.is_empty() {
        return Err(format!(
            "invalid include path '{selector}': repository root cannot be included"
        ));
    }
    Ok(components.join("/"))
}

fn include_metadata(
    path: &Path,
    selector: &str,
) -> Result<Option<std::fs::Metadata>, String> {
    match path.symlink_metadata() {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(format!("invalid include path '{selector}': {error}"))
        }
    }
}

fn include_directory_entries(
    parent: &Path,
    selector: &str,
) -> Result<Vec<std::fs::DirEntry>, String> {
    std::fs::read_dir(parent)
        .map_err(|error| {
            format!(
                "invalid include path '{selector}': cannot read '{}': {error}",
                parent.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            format!(
                "invalid include path '{selector}': cannot read '{}': {error}",
                parent.display()
            )
        })
}

fn actual_include_path(
    entries: Vec<std::fs::DirEntry>,
    requested: &Path,
    component: &str,
    selector: &str,
) -> Result<PathBuf, String> {
    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.file_name() == OsStr::new(component))
    {
        return Ok(entry.path());
    }

    let identify = |path: &Path, error| {
        format!(
            "invalid include path '{selector}': cannot identify '{}': {error}",
            path.display()
        )
    };
    let requested_handle = Handle::from_path(requested)
        .map_err(|error| identify(requested, error))?;
    let mut matches = Vec::new();
    for entry in entries {
        let path = entry.path();
        let file_type =
            entry.file_type().map_err(|error| identify(&path, error))?;
        if file_type.is_symlink() {
            continue;
        }
        let handle = Handle::from_path(&path)
            .map_err(|error| identify(&path, error))?;
        if handle == requested_handle {
            matches.push(path);
        }
    }

    match matches.len() {
        0 => Err(format!(
            "invalid include path '{selector}': resolved component '{component}' has no matching directory entry"
        )),
        1 => Ok(matches.pop().unwrap()),
        _ => Err(format!(
            "invalid include path '{selector}': component '{component}' is ambiguous"
        )),
    }
}

fn resolve_include_component(
    parent: &Path,
    component: &str,
    selector: &str,
) -> Result<Option<(PathBuf, std::fs::Metadata)>, String> {
    let requested = parent.join(component);
    let Some(metadata) = include_metadata(&requested, selector)? else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() {
        return Ok(None);
    }
    let entries = include_directory_entries(parent, selector)?;
    let actual =
        actual_include_path(entries, &requested, component, selector)?;
    Ok(Some((actual, metadata)))
}

fn resolve_include(
    repo_path: &Path,
    selector: &str,
) -> Result<Option<IncludeSelector>, String> {
    let path = normalize_include(selector)?;
    let mut current = repo_path.to_path_buf();
    let component_count = path.split('/').count();
    for (index, component) in path.split('/').enumerate() {
        let Some((actual, metadata)) =
            resolve_include_component(&current, component, selector)?
        else {
            return Ok(None);
        };
        current = actual;
        if index + 1 < component_count && !metadata.is_dir() {
            return Ok(None);
        }
        if index + 1 == component_count {
            if !metadata.is_file() && !metadata.is_dir() {
                return Ok(None);
            }
            return Ok(Some(IncludeSelector {
                path: repository_path(
                    current.strip_prefix(repo_path).map_err(|error| {
                        format!("invalid include path '{selector}': {error}")
                    })?,
                ),
                directory: metadata.is_dir(),
            }));
        }
    }
    Ok(None)
}

fn include_entry(selectors: &[IncludeSelector], path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    selectors.iter().any(|selector| {
        path == selector.path
            || selector.path.starts_with(&format!("{path}/"))
            || (selector.directory
                && path.starts_with(&format!("{}/", selector.path)))
    })
}

fn include_file(selectors: &[IncludeSelector], path: &str) -> bool {
    selectors.iter().any(|selector| {
        path == selector.path
            || (selector.directory
                && path.starts_with(&format!("{}/", selector.path)))
    })
}

pub fn list_files_in_repo<N: Write, D: Write>(
    repo_path: &Path,
    options: &FileSelectionOptions<'_>,
    reporter: &mut ProgressReporter<N, D>,
) -> Result<Vec<String>, String> {
    let mut file_list = BTreeSet::new();
    let exclusions = Arc::new(ExclusionMatcher::new(
        options.legacy_excludes,
        options.extend_exclude,
        options.exclude,
    )?);
    walk_normal(repo_path, &exclusions, reporter, &mut file_list);

    let selectors = options
        .include
        .unwrap_or_default()
        .iter()
        .map(|selector| resolve_include(repo_path, selector))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if !selectors.is_empty() {
        walk_includes(repo_path, &selectors, reporter, &mut file_list);
    }

    Ok(file_list.into_iter().collect())
}

fn walk_normal<N: Write, D: Write>(
    repo_path: &Path,
    exclusions: &Arc<ExclusionMatcher>,
    reporter: &mut ProgressReporter<N, D>,
    file_list: &mut BTreeSet<String>,
) {
    let filter_root = repo_path.to_path_buf();
    let filter_exclusions = Arc::clone(exclusions);

    let mut builder = WalkBuilder::new(repo_path);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .filter_entry(move |entry| {
            let Ok(relative) = entry.path().strip_prefix(&filter_root) else {
                return false;
            };
            if has_git_component(relative) {
                return false;
            }
            !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !filter_exclusions
                    .matches_directory(&repository_path(relative))
        });

    for result in builder.build() {
        match result {
            Ok(entry) => {
                if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                    continue;
                }

                let path = entry.path();
                let relative_path = match path.strip_prefix(repo_path) {
                    Ok(path) => repository_path(path),
                    Err(_) => continue,
                };

                if exclusions.matches_file(&relative_path) {
                    continue;
                }

                file_list.insert(relative_path);
            }
            Err(err) => reporter
                .always_visible_diagnostic(&format!("Error: {err}"))
                .unwrap(),
        }
    }
}

fn walk_includes<N: Write, D: Write>(
    repo_path: &Path,
    selectors: &[IncludeSelector],
    reporter: &mut ProgressReporter<N, D>,
    file_list: &mut BTreeSet<String>,
) {
    let filter_root = repo_path.to_path_buf();
    let filter_selectors = selectors.to_vec();
    let mut builder = WalkBuilder::new(repo_path);
    builder
        .standard_filters(false)
        .follow_links(false)
        .filter_entry(move |entry| {
            let Ok(relative) = entry.path().strip_prefix(&filter_root) else {
                return false;
            };
            !has_git_component(relative)
                && include_entry(&filter_selectors, &repository_path(relative))
        });

    for result in builder.build() {
        match result {
            Ok(entry)
                if entry.file_type().is_some_and(|kind| kind.is_file()) =>
            {
                let Ok(relative) = entry.path().strip_prefix(repo_path) else {
                    continue;
                };
                let relative = repository_path(relative);
                if include_file(selectors, &relative) {
                    file_list.insert(relative);
                }
            }
            Ok(_) => {}
            Err(error) => reporter
                .always_visible_diagnostic(&format!("Error: {error}"))
                .unwrap(),
        }
    }
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
#[path = "../tests/crate/filelist.rs"]
mod tests;
