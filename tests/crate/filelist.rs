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

fn list_files(
    repo_path: &Path,
    extend_exclude: Option<&[String]>,
    exclude: Option<&[String]>,
    include: Option<&[String]>,
    legacy_excludes: bool,
) -> Result<Vec<String>, String> {
    let mut reporter =
        crate::progress::ProgressReporter::new(Vec::new(), Vec::new(), true);
    let options = FileSelectionOptions {
        extend_exclude,
        exclude,
        include,
        legacy_excludes,
    };
    let mut files = list_files_in_repo(repo_path, &options, &mut reporter)?;
    files.sort();
    Ok(files)
}

#[test]
fn test_list_files_basic() {
    let temp_dir = TempDir::new().unwrap();
    let test_files = ["file1.txt", "src/file2.rs", "src/nested/file3.rs"];
    create_test_files(&temp_dir, &test_files);

    let files = list_files(temp_dir.path(), None, None, None, false).unwrap();

    assert_eq!(files.len(), 3);
    assert!(files.contains(&"file1.txt".to_string()));
    assert!(files.contains(&"src/file2.rs".to_string()));
    assert!(files.contains(&"src/nested/file3.rs".to_string()));
}

#[test]
fn test_modern_default_includes_useful_repository_context() {
    let temp_dir = TempDir::new().unwrap();
    let test_files = [
        ".gitignore",
        ".github/workflows/test.yml",
        "Cargo.lock",
        "requirements.txt",
        "LICENSE.txt",
        "renovate.json",
        ".vscode/settings.json",
        ".tool-config",
        "contains.git.txt",
        ".git/config",
        "nested/.git/config",
    ];
    create_test_files(&temp_dir, &test_files);

    let files = list_files(temp_dir.path(), None, None, None, false).unwrap();

    for included in &test_files[..9] {
        assert!(files.contains(&included.to_string()), "missing {included}");
    }
    assert!(!files.iter().any(|path| path.starts_with(".git/")));
    assert!(!files.iter().any(|path| path.contains("/.git/")));
}

#[test]
fn test_git_metadata_file_is_structurally_excluded() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(&temp_dir, &["visible.txt"]);
    File::create(temp_dir.path().join(".git")).unwrap();

    let files = list_files(temp_dir.path(), None, None, None, false).unwrap();

    assert_eq!(files, vec!["visible.txt"]);
}

#[test]
fn test_exclusion_glob_contract() {
    let patterns = vec![
        "*.md".to_string(),
        "docs/*".to_string(),
        "file?.txt".to_string(),
        "src/[ab].rs".to_string(),
        "target".to_string(),
        r"windows\path\*.toml".to_string(),
    ];
    let matcher = ExclusionMatcher::new(false, None, Some(&patterns)).unwrap();

    for path in [
        "README.md",
        "nested/Guide.MD",
        "docs/direct.txt",
        "file1.txt",
        "src/a.rs",
        "target",
        "nested/target",
        "windows/path/config.toml",
    ] {
        assert!(matcher.matches_file(path), "expected match for {path}");
    }
    for path in [
        "docs/nested/deep.txt",
        "file10.txt",
        "src/c.rs",
        "targeted/file.txt",
    ] {
        assert!(!matcher.matches_file(path), "unexpected match for {path}");
    }

    let recursive =
        ExclusionMatcher::new(false, None, Some(&["docs/**".to_string()]))
            .unwrap();
    assert!(recursive.matches_file("docs/nested/deep.txt"));

    let subtree =
        ExclusionMatcher::new(false, None, Some(&["generated/".to_string()]))
            .unwrap();
    assert!(subtree.matches_file("generated"));
    assert!(subtree.matches_file("generated/nested/file.txt"));
}

#[test]
fn test_root_anchored_basename_excludes_only_repository_root() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(
        &temp_dir,
        &["target/root.txt", "nested/target/deep.txt", "visible.txt"],
    );

    let unanchored = vec!["target".to_string()];
    let files =
        list_files(temp_dir.path(), None, Some(&unanchored), None, false)
            .unwrap();
    assert_eq!(files, vec!["visible.txt"]);

    let anchored = vec!["/target".to_string()];
    let files =
        list_files(temp_dir.path(), None, Some(&anchored), None, false)
            .unwrap();
    assert_eq!(files, vec!["nested/target/deep.txt", "visible.txt"]);
}

#[test]
fn test_root_anchored_directory_globs_share_exclusion_semantics() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(
        &temp_dir,
        &[
            "docs/root.txt",
            "docs/nested/deep.txt",
            "nested/docs/visible.txt",
        ],
    );

    for pattern in ["/docs/**", "/docs/"] {
        let patterns = vec![pattern.to_string()];
        let excluded =
            list_files(temp_dir.path(), None, Some(&patterns), None, false)
                .unwrap();
        let extended =
            list_files(temp_dir.path(), Some(&patterns), None, None, false)
                .unwrap();

        assert_eq!(excluded, vec!["nested/docs/visible.txt"]);
        assert_eq!(extended, excluded);
    }
}

#[test]
fn test_invalid_root_anchor_exclusion_globs_are_errors() {
    let temp_dir = TempDir::new().unwrap();

    for pattern in ["/", "//target", "///docs/**"] {
        let patterns = vec![pattern.to_string()];
        let error =
            list_files(temp_dir.path(), None, Some(&patterns), None, false)
                .unwrap_err();

        assert!(error.contains(pattern));
        assert!(error.contains("invalid exclusion glob"));
    }
}

#[test]
fn test_exclusion_globs_are_unicode_case_insensitive() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(&temp_dir, &["nested/ÜBER.txt"]);
    let exclude = vec!["über.txt".to_string()];

    let files = list_files(temp_dir.path(), None, Some(&exclude), None, false)
        .unwrap();

    assert!(files.is_empty());
}

#[test]
fn test_exclusion_question_mark_matches_one_unicode_character() {
    let exclude = vec!["?.txt".to_string()];

    let unicode_dir = TempDir::new().unwrap();
    create_test_files(&unicode_dir, &["ß.txt"]);
    let files =
        list_files(unicode_dir.path(), None, Some(&exclude), None, false)
            .unwrap();
    assert!(files.is_empty());

    let ascii_dir = TempDir::new().unwrap();
    create_test_files(&ascii_dir, &["ss.txt"]);
    let files =
        list_files(ascii_dir.path(), None, Some(&exclude), None, false)
            .unwrap();
    assert_eq!(files, vec!["ss.txt"]);
}

#[test]
fn test_exclusion_character_class_matches_one_unicode_character() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(&temp_dir, &["ß.txt", "s.txt"]);
    let exclude = vec!["[ẞ].txt".to_string()];

    let files = list_files(temp_dir.path(), None, Some(&exclude), None, false)
        .unwrap();

    assert_eq!(files, vec!["s.txt"]);
}

#[test]
fn test_exclusion_ascii_glob_width_is_unchanged() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(&temp_dir, &["a.txt", "ab.txt", "b.txt"]);
    let exclude = vec!["[a]?.txt".to_string()];

    let files = list_files(temp_dir.path(), None, Some(&exclude), None, false)
        .unwrap();

    assert_eq!(files, vec!["a.txt", "b.txt"]);
}

#[test]
fn test_custom_exclude_replaces_defaults_through_file_listing() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(
        &temp_dir,
        &["ordinary.txt", "custom.tmp", "Cargo.lock"],
    );
    let exclude = vec!["custom.tmp".to_string()];

    let files =
        list_files(temp_dir.path(), None, Some(&exclude), None, true).unwrap();

    assert_eq!(files.len(), 2);
    assert!(files.contains(&"ordinary.txt".to_string()));
    assert!(files.contains(&"Cargo.lock".to_string()));
    assert!(!files.contains(&"custom.tmp".to_string()));
}

#[test]
fn test_extend_exclude_augments_selected_profile_through_file_listing() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(
        &temp_dir,
        &["ordinary.txt", "custom.tmp", "Cargo.lock"],
    );
    let extend_exclude = vec!["*.tmp".to_string()];

    let modern =
        list_files(temp_dir.path(), Some(&extend_exclude), None, None, false)
            .unwrap();
    let legacy =
        list_files(temp_dir.path(), Some(&extend_exclude), None, None, true)
            .unwrap();

    assert_eq!(modern, vec!["Cargo.lock", "ordinary.txt"]);
    assert_eq!(legacy, vec!["ordinary.txt"]);
}

#[test]
fn test_exclude_accepts_windows_separator_input() {
    let temp_dir = TempDir::new().unwrap();
    let test_files = ["file1.txt", "src/file2.rs"];
    create_test_files(&temp_dir, &test_files);

    let exclude = vec![r"src\file2.rs".to_string()];
    let files = list_files(temp_dir.path(), None, Some(&exclude), None, false)
        .unwrap();

    assert_eq!(files, vec!["file1.txt"]);
}

#[test]
fn test_legacy_exclude_profile_and_replacement_precedence() {
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

    let files = list_files(temp_dir.path(), None, None, None, true).unwrap();

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
fn test_legacy_file_pattern_does_not_prune_matching_directory() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(&temp_dir, &["cache.lock/keep.txt"]);

    let files = list_files(temp_dir.path(), None, None, None, true).unwrap();

    assert_eq!(files, vec!["cache.lock/keep.txt"]);
}

#[test]
fn test_include_overrides_bundle_exclusions_without_duplicates() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(
        &temp_dir,
        &["keep.txt", "other.txt", "generated/a.txt", "generated/b.md"],
    );
    let exclude = vec!["*.txt".to_string(), "generated/**".to_string()];
    let include = vec![
        "keep.txt".to_string(),
        "generated/".to_string(),
        "generated/a.txt".to_string(),
    ];

    let files = list_files(
        temp_dir.path(),
        None,
        Some(&exclude),
        Some(&include),
        false,
    )
    .unwrap();

    assert_eq!(files, vec!["generated/a.txt", "generated/b.md", "keep.txt"]);
}

#[test]
fn test_include_overrides_legacy_and_extend_excludes() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(&temp_dir, &["Cargo.lock", ".github/workflow.yml"]);
    let extend = vec!["*.yml".to_string()];
    let include = vec!["Cargo.lock".to_string(), ".github/".to_string()];

    let files =
        list_files(temp_dir.path(), Some(&extend), None, Some(&include), true)
            .unwrap();

    assert_eq!(files, vec![".github/workflow.yml", "Cargo.lock"]);
}

#[test]
fn test_include_narrowly_overrides_repository_ignore_rules() {
    let temp_dir = TempDir::new().unwrap();
    git2::Repository::init(temp_dir.path()).unwrap();
    fs::write(
        temp_dir.path().join(".gitignore"),
        "ignored.txt\ngenerated/\n",
    )
    .unwrap();
    fs::write(temp_dir.path().join(".ignore"), "private.txt\n").unwrap();
    create_test_files(
        &temp_dir,
        &[
            "ignored.txt",
            "ignored-sibling.txt",
            "private.txt",
            "generated/schema.json",
            "generated/nested/hidden.txt",
            "visible.txt",
        ],
    );
    fs::write(temp_dir.path().join("generated/.ignore"), "nested/\n").unwrap();

    let normal = list_files(temp_dir.path(), None, None, None, false).unwrap();
    assert!(!normal.contains(&"ignored.txt".to_string()));
    assert!(!normal.contains(&"private.txt".to_string()));
    assert!(!normal.iter().any(|path| path.starts_with("generated/")));

    let include = vec![
        "ignored.txt".to_string(),
        "private.txt".to_string(),
        "generated/".to_string(),
    ];
    let files = list_files(temp_dir.path(), None, None, Some(&include), false)
        .unwrap();

    for path in [
        "ignored.txt",
        "private.txt",
        "generated/schema.json",
        "generated/nested/hidden.txt",
        "visible.txt",
    ] {
        assert!(files.contains(&path.to_string()), "missing {path}");
    }
    assert!(files.contains(&"ignored-sibling.txt".to_string()));
}

#[test]
fn test_include_overrides_git_info_exclude() {
    let temp_dir = TempDir::new().unwrap();
    git2::Repository::init(temp_dir.path()).unwrap();
    fs::write(
        temp_dir.path().join(".git/info/exclude"),
        "generated.txt\nignored-sibling.txt\n",
    )
    .unwrap();
    create_test_files(
        &temp_dir,
        &["generated.txt", "ignored-sibling.txt", "visible.txt"],
    );
    let include = vec!["generated.txt".to_string()];

    let files = list_files(temp_dir.path(), None, None, Some(&include), false)
        .unwrap();

    assert!(files.contains(&"generated.txt".to_string()));
    assert!(!files.contains(&"ignored-sibling.txt".to_string()));
    assert!(files.contains(&"visible.txt".to_string()));
}

#[test]
fn test_include_file_below_ignored_directory_is_narrow() {
    let temp_dir = TempDir::new().unwrap();
    git2::Repository::init(temp_dir.path()).unwrap();
    fs::write(temp_dir.path().join(".gitignore"), "generated/\n").unwrap();
    create_test_files(&temp_dir, &["generated/one.txt", "generated/two.txt"]);
    let include = vec!["generated/one.txt".to_string()];

    let files = list_files(temp_dir.path(), None, None, Some(&include), false)
        .unwrap();

    assert!(files.contains(&"generated/one.txt".to_string()));
    assert!(!files.contains(&"generated/two.txt".to_string()));
}

#[test]
fn test_include_relationships_respect_component_boundaries() {
    let selectors = [
        IncludeSelector {
            path: "foo".to_string(),
            directory: true,
        },
        IncludeSelector {
            path: "selected/file.txt".to_string(),
            directory: false,
        },
    ];

    assert!(include_entry(&selectors, "selected"));
    assert!(include_file(&selectors, "foo/child.txt"));
    assert!(include_file(&selectors, "selected/file.txt"));
    assert!(!include_file(&selectors, "foobar/child.txt"));
    assert!(!include_file(&selectors, "selected/file.txt/child"));
}

#[test]
fn test_alternate_case_include_file_uses_filesystem_spelling() {
    let temp_dir = TempDir::new().unwrap();
    git2::Repository::init(temp_dir.path()).unwrap();
    fs::write(temp_dir.path().join(".gitignore"), "MixedCase.txt\n").unwrap();
    create_test_files(&temp_dir, &["MixedCase.txt"]);
    let alternate = "mixedcase.TXT";
    let alternate_resolves =
        temp_dir.path().join(alternate).symlink_metadata().is_ok();
    let include = vec![alternate.to_string()];

    let files = list_files(temp_dir.path(), None, None, Some(&include), false)
        .unwrap();

    assert_eq!(
        files.contains(&"MixedCase.txt".to_string()),
        alternate_resolves
    );
    assert!(!files.contains(&alternate.to_string()));
}

#[test]
fn test_alternate_case_include_directory_uses_filesystem_spelling() {
    let temp_dir = TempDir::new().unwrap();
    git2::Repository::init(temp_dir.path()).unwrap();
    fs::write(temp_dir.path().join(".gitignore"), "Generated/\n").unwrap();
    create_test_files(&temp_dir, &["Generated/Nested/Schema.json"]);
    let alternate = "generated";
    let alternate_resolves =
        temp_dir.path().join(alternate).symlink_metadata().is_ok();
    let include = vec![format!("{alternate}/")];

    let files = list_files(temp_dir.path(), None, None, Some(&include), false)
        .unwrap();

    assert_eq!(
        files.contains(&"Generated/Nested/Schema.json".to_string()),
        alternate_resolves
    );
    assert!(!files.contains(&"generated/Nested/Schema.json".to_string()));
}

#[test]
fn test_ambiguous_alternate_case_include_is_an_error() {
    let temp_dir = TempDir::new().unwrap();
    git2::Repository::init(temp_dir.path()).unwrap();
    fs::write(
        temp_dir.path().join(".gitignore"),
        "Original.txt\nAlias.txt\n",
    )
    .unwrap();
    create_test_files(&temp_dir, &["Original.txt"]);
    fs::hard_link(
        temp_dir.path().join("Original.txt"),
        temp_dir.path().join("Alias.txt"),
    )
    .unwrap();
    let alternate = "ORIGINAL.TXT";
    let include = vec![alternate.to_string()];

    let result =
        list_files(temp_dir.path(), None, None, Some(&include), false);

    if temp_dir.path().join(alternate).symlink_metadata().is_ok() {
        let error = result.unwrap_err();
        assert!(error.contains("invalid include path 'ORIGINAL.TXT'"));
        assert!(error.contains("ambiguous"));
    } else {
        assert!(!result.unwrap().iter().any(|path| path.ends_with(".txt")));
    }
}

#[cfg(unix)]
#[test]
fn test_include_accepts_relative_colon_filename() {
    let temp_dir = TempDir::new().unwrap();
    git2::Repository::init(temp_dir.path()).unwrap();
    fs::write(temp_dir.path().join(".gitignore"), "a:b\n").unwrap();
    create_test_files(&temp_dir, &["a:b"]);
    let include = vec!["a:b".to_string()];

    let files = list_files(temp_dir.path(), None, None, Some(&include), false)
        .unwrap();

    assert!(files.contains(&"a:b".to_string()));
}

#[test]
fn test_invalid_glob_is_an_error() {
    let temp_dir = TempDir::new().unwrap();
    let invalid = vec!["[unterminated".to_string()];

    let error = list_files(temp_dir.path(), None, Some(&invalid), None, false)
        .unwrap_err();

    assert!(error.contains("[unterminated"));
    assert!(error.contains("invalid exclusion glob"));
}

#[test]
fn test_invalid_include_selectors_are_errors() {
    let temp_dir = TempDir::new().unwrap();
    let absolute = temp_dir.path().join("file.txt").display().to_string();
    let cases = [
        absolute.as_str(),
        r"C:\temp\file.txt",
        "C:/temp/file.txt",
        "../file.txt",
        "foo/../file.txt",
        "",
        ".",
        "./",
        "././",
        ".git",
        ".git/config",
        "nested/.git/config",
    ];

    for selector in cases {
        let include = vec![selector.to_string()];
        let error =
            list_files(temp_dir.path(), None, None, Some(&include), false)
                .unwrap_err();
        assert!(error.contains(selector));
        assert!(error.contains("invalid include path"));
    }
}

#[test]
fn test_git_boundary_follows_filesystem_case_semantics() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(&temp_dir, &[".GIT/config"]);
    let git_alias_resolves = temp_dir.path().join(".git").exists();

    let files = list_files(temp_dir.path(), None, None, None, false).unwrap();

    assert_eq!(
        files.contains(&".GIT/config".to_string()),
        !git_alias_resolves
    );
}

#[test]
fn test_include_git_boundary_follows_filesystem_case_semantics() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(&temp_dir, &[".GIT/config"]);
    let git_alias_resolves = temp_dir.path().join(".git").exists();
    let include = vec![".GIT/config".to_string()];

    let result =
        list_files(temp_dir.path(), None, None, Some(&include), false);

    if git_alias_resolves {
        let error = result.unwrap_err();
        assert!(error.contains(".git metadata cannot be included"));
    } else {
        assert_eq!(result.unwrap(), vec![".GIT/config"]);
    }
}

#[test]
fn test_invalid_include_is_validated_before_normal_walk_diagnostics() {
    let temp_dir = TempDir::new().unwrap();
    let missing_repo = temp_dir.path().join("missing");
    let include = vec!["../outside.txt".to_string()];
    let options = FileSelectionOptions {
        extend_exclude: None,
        exclude: None,
        include: Some(&include),
        legacy_excludes: false,
    };
    let mut reporter =
        crate::progress::ProgressReporter::new(Vec::new(), Vec::new(), true);

    let error = list_files_in_repo(&missing_repo, &options, &mut reporter)
        .unwrap_err();

    assert!(error.contains("invalid include path"));
    let (_, diagnostic) = reporter.into_parts();
    assert!(diagnostic.is_empty());
}

#[test]
fn test_missing_include_is_a_no_op() {
    let temp_dir = TempDir::new().unwrap();
    create_test_files(&temp_dir, &["visible.txt"]);
    let include = vec!["missing/file.txt".to_string()];

    let files = list_files(temp_dir.path(), None, None, Some(&include), false)
        .unwrap();

    assert_eq!(files, vec!["visible.txt"]);
}

#[cfg(unix)]
#[test]
fn test_include_does_not_follow_symlinks_outside_repository() {
    use std::os::unix::fs::symlink;

    let temp_dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("secret.txt"), "outside").unwrap();
    symlink(outside.path(), temp_dir.path().join("linked")).unwrap();
    let include = vec!["linked/".to_string(), "linked/secret.txt".to_string()];

    let files = list_files(temp_dir.path(), None, None, Some(&include), false)
        .unwrap();

    assert!(files.is_empty());
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
