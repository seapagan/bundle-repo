use super::*;

#[test]
fn test_metadata_section_order_with_optional_skipped_section() {
    use crate::secret_scanning::SkippedItemKind;
    let flags = metadata_params(&[("arbitrary", "value")]);
    let skipped = [SkippedRepositoryItem {
        kind: SkippedItemKind::File,
        safe_path: "[REDACTED]".to_string(),
        reason: SkipReason::SecretInPath { secret_type: None },
    }];
    let xml = serialize_metadata_fixture(&flags, &[]);
    assert_eq!(
        parse_top_level_sections(&xml),
        [
            "file_summary",
            "repository_metadata",
            "repository_structure",
            "repository_files"
        ]
    );
    let xml = serialize_metadata_fixture(&flags, &skipped);
    assert_eq!(
        parse_top_level_sections(&xml),
        [
            "file_summary",
            "repository_metadata",
            "repository_structure",
            "repository_skipped",
            "repository_files"
        ]
    );
}

#[test]
fn test_metadata_summary_numbers_scanned_repository_sections() {
    let flags = metadata_params(&[("arbitrary", "value")]);
    let xml = serialize_metadata_fixture(&flags, &[]);
    assert_eq!(
        parse_element_text(&xml, "file_format"),
        "The content is organized as follows:\n1. This summary section\n2. Repository metadata: User-provided metadata from BundleRepo configuration.\n3. Repository structure: A hierarchical listing of safely emitted folders and files.\n4. Repository skipped (optional): Safe diagnostics for files or subtrees omitted because a secret was detected in their path.\n5. Repository files: Each emitted file is listed with:\n  - File path as an attribute\n  - Full contents of the file, excluding binary files, text classified as likely secret-bearing by its path/type, and text that XML 1.0 cannot represent."
    );
    assert_eq!(
        parse_element_text(&xml, "notes"),
        "- Repository ignore rules and configured exclusion patterns may omit files\n  unless a path was explicitly included.\n- Files and subtrees with detected secrets in their paths are omitted from both\n  canonical repository sections and reported safely under Repository Skipped.\n- Decoded text classified as likely secret-bearing by its path/type retains its\n  canonical file entry with a safe unavailable-content diagnostic.\n- Binary files and text that XML 1.0 cannot represent retain a file entry with\n  an unavailable-content diagnostic.\n- Configured metadata was mandatorily validated for secrets and XML compatibility."
    );
}

#[test]
fn test_metadata_summary_qualifies_disabled_repository_scanning() {
    let mut flags = metadata_params(&[("arbitrary", "value")]);
    flags.secret_scan = false;
    let xml = serialize_metadata_fixture(&flags, &[]);
    assert_eq!(
        parse_element_text(&xml, "file_format"),
        "The content is organized as follows:\n1. This summary section\n2. Repository metadata: User-provided metadata from BundleRepo configuration.\n3. Repository structure: A hierarchical listing of folders and files.\n4. Repository files: Each file is listed with:\n  - File path as an attribute\n  - Full contents of the file, excluding binary files and text that XML 1.0 cannot represent."
    );
    assert_eq!(
        parse_element_text(&xml, "notes"),
        "- Repository ignore rules and configured exclusion patterns may omit files\n  unless a path was explicitly included.\n- Binary files and text that XML 1.0 cannot represent retain a file entry with\n  an unavailable-content diagnostic.\n- Repository path and content secret scanning was disabled for this bundle.\n- Configured metadata was mandatorily validated for secrets and XML compatibility."
    );
}

#[test]
fn test_no_metadata_document_bytes_match_pre_metadata_baselines() {
    for secret_scan in [false, true] {
        let flags = Params {
            secret_scan,
            ..Params::default()
        };
        let xml = serialize_metadata_fixture(&flags, &[]);
        assert_eq!(parse_metadata(&xml), None);
        assert_eq!(
            parse_top_level_sections(&xml),
            ["file_summary", "repository_structure", "repository_files"]
        );
        let expected = if secret_scan {
            NO_METADATA_SCANNED
        } else {
            NO_METADATA_UNSCANNED
        };
        assert_eq!(xml, expected.as_bytes());
    }
}

const NO_METADATA_UNSCANNED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<repository>
  <file_summary>
    <purpose>This file contains a packed representation of the entire repository's contents.
It is designed to be easily consumable by AI systems for analysis, code review,
or other automated processes.</purpose>
    <file_format>The content is organized as follows:
1. This summary section
2. Repository structure: A hierarchical listing of folders and files.
3. Repository files: Each file is listed with:
  - File path as an attribute
  - Full contents of the file, excluding binary files and text that XML 1.0 cannot represent.</file_format>
    <instructions>- The LLM is instructed to focus solely on the repository's contents, including
  the code, file structure, and purpose of the files.
- Do not comment on the XML format, structure, or encoding of THIS FILE. Focus
  your analysis on the functionality, structure, and organization of the
  repository contents.
- Each &lt;file&gt; should be interpreted based on its file extension. For example:
  - ".py" for Python
  - ".md" for Markdown
  - ".rs" for Rust
  - ".cpp" for C++</instructions>
    <usage_guidelines>- This file should be treated as read-only. Any changes should be made to the
  original repository files, not this packed version.
- When processing this file, use the file path to distinguish
  between different files in the repository.
- Be aware that this file may contain sensitive information. Handle it with
  the same level of security as you would the original repository.</usage_guidelines>
    <notes>- Repository ignore rules and configured exclusion patterns may omit files
  unless a path was explicitly included.
- Binary files and text that XML 1.0 cannot represent retain a file entry with
  an unavailable-content diagnostic.
- Secret scanning was disabled for this bundle.</notes>
    <additional_info>For more information about bundlerepo, visit: https://github.com/seapagan/bundle-repo</additional_info>
  </file_summary>
  <repository_structure>
    <summary>This node contains the hierarchical structure of the repository's files and folders.</summary>
    <file path="test.txt" />
  </repository_structure>
  <repository_files>
    <summary>This node contains a list of files with their full paths and contents serialized as CDATA.</summary>
    <file path="test.txt" size="13" lines="1"><![CDATA[ordinary text]]></file>
  </repository_files>
</repository>
"#;

const NO_METADATA_SCANNED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<repository>
  <file_summary>
    <purpose>This file contains a packed representation of the entire repository's contents.
It is designed to be easily consumable by AI systems for analysis, code review,
or other automated processes.</purpose>
    <file_format>The content is organized as follows:
1. This summary section
2. Repository structure: A hierarchical listing of safely emitted folders and files.
3. Repository skipped (optional): Safe diagnostics for files or subtrees omitted because a secret was detected in their path.
4. Repository files: Each emitted file is listed with:
  - File path as an attribute
  - Full contents of the file, excluding binary files, text classified as likely secret-bearing by its path/type, and text that XML 1.0 cannot represent.</file_format>
    <instructions>- The LLM is instructed to focus solely on the repository's contents, including
  the code, file structure, and purpose of the files.
- Do not comment on the XML format, structure, or encoding of THIS FILE. Focus
  your analysis on the functionality, structure, and organization of the
  repository contents.
- Each &lt;file&gt; should be interpreted based on its file extension. For example:
  - ".py" for Python
  - ".md" for Markdown
  - ".rs" for Rust
  - ".cpp" for C++</instructions>
    <usage_guidelines>- This file should be treated as read-only. Any changes should be made to the
  original repository files, not this packed version.
- When processing this file, use the file path to distinguish
  between different files in the repository.
- Be aware that this file may contain sensitive information. Handle it with
  the same level of security as you would the original repository.</usage_guidelines>
    <notes>- Repository ignore rules and configured exclusion patterns may omit files
  unless a path was explicitly included.
- Files and subtrees with detected secrets in their paths are omitted from both
  canonical repository sections and reported safely under Repository Skipped.
- Decoded text classified as likely secret-bearing by its path/type retains its
  canonical file entry with a safe unavailable-content diagnostic.
- Binary files and text that XML 1.0 cannot represent retain a file entry with
  an unavailable-content diagnostic.</notes>
    <additional_info>For more information about bundlerepo, visit: https://github.com/seapagan/bundle-repo</additional_info>
  </file_summary>
  <repository_structure>
    <summary>This node contains the hierarchical structure of the repository's files and folders.</summary>
    <file path="test.txt" />
  </repository_structure>
  <repository_files>
    <summary>This node contains a list of files with their full paths and contents serialized as CDATA.</summary>
    <file path="test.txt" size="13" lines="1"><![CDATA[ordinary text]]></file>
  </repository_files>
</repository>
"#;

#[test]
fn test_add_line_numbers() {
    let content = "First line\nSecond line\nThird line";
    let numbered = add_line_numbers(content);
    assert!(numbered.contains("1  First line"));
    assert!(numbered.contains("2  Second line"));
    assert!(numbered.contains("3  Third line"));
    assert!(numbered.ends_with('\n'));
}

#[test]
fn test_read_classify_and_decode_distinguishes_text_and_binary() {
    let temp_dir = tempdir().unwrap();
    let mut timings = ProcessingTimings::default();

    // Create a text file
    let text_path = temp_dir.path().join("test.txt");
    fs::write(&text_path, "Hello, World!").unwrap();
    assert!(matches!(
        read_classify_and_decode(&text_path, false, &mut timings).unwrap(),
        ProcessedFile::Text(_)
    ));

    // Create a binary file
    let binary_path = temp_dir.path().join("test.bin");
    fs::write(&binary_path, [0u8, 159u8, 146u8, 150u8]).unwrap();
    assert!(matches!(
        read_classify_and_decode(&binary_path, false, &mut timings).unwrap(),
        ProcessedFile::Binary(_)
    ));
}

#[test]
fn test_output_repo_as_xml() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");

    // Create the test file
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "Test content").unwrap();

    let params = Params {
        output_file: Some(output_file.to_str().unwrap().to_string()),
        ..Params::default()
    };

    let mut file_tree = FileTree::default();
    file_tree.file_paths.push("test.txt".to_string());

    let tokenizer = Model::GPT4.to_tokenizer().unwrap();

    let result =
        output_repo_as_xml(&params, file_tree, temp_dir.path(), &tokenizer);
    assert!(result.is_ok());

    let xml_content = fs::read_to_string(output_file).unwrap();
    assert!(
        xml_content.contains("<?xml version=\"1.0\" encoding=\"utf-8\"?>")
    );
    assert!(xml_content.contains("<repository>"));
    assert!(xml_content.contains("<repository_structure>"));
    assert!(xml_content.contains("<repository_files>"));
    assert!(xml_content.contains("<file path=\"test.txt\""));
    assert!(xml_content.contains("Test content"));
    assert!(xml_content.contains(
        "Repository ignore rules and configured exclusion patterns may omit"
    ));
    assert!(xml_content.contains("unless a path was explicitly included"));
}

#[test]
fn test_scanner_redacts_content_before_line_numbers_and_serialization() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");
    let secret = crate::secret_scanning::synthetic_github_pat();
    fs::write(
        temp_dir.path().join("secret.txt"),
        format!("let token = \"{secret}\";\nlet safe = true;"),
    )
    .unwrap();
    let params = Params {
        output_file: Some(output_file.to_string_lossy().into_owned()),
        line_numbers: true,
        ..Params::default()
    };
    let mut tree = FileTree::default();
    tree.file_paths.push("secret.txt".to_string());
    let scanner = SecretScanner::from_bundled().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = ProcessingTimings::default();

    output_repo_as_xml_with_scanner_and_timings(
        &params,
        tree,
        temp_dir.path(),
        &Model::GPT4.to_tokenizer().unwrap(),
        "GPT-4",
        Some(&scanner),
        &mut reporter,
        &mut timings,
    )
    .unwrap();

    let xml = fs::read(output_file).unwrap();
    let text = String::from_utf8(xml.clone()).unwrap();
    assert!(!text.contains(&secret));
    assert!(text.contains(
        "1  let token = \"[Secret removed: GitHub Personal Access Token]\";"
    ));
    assert!(text.contains("2  let safe = true;"));
    assert_eq!(timings.text_files_scanned, 1);
    assert!(timings.findings_redacted >= 1);
    assert!(timings.secret_scanning > Duration::ZERO);
    parse_document(&xml);
    let (normal, diagnostic) = reporter.into_parts();
    assert!(!String::from_utf8(normal).unwrap().contains(&secret));
    assert!(!String::from_utf8(diagnostic).unwrap().contains(&secret));
}

#[test]
fn test_large_file_secret_is_redacted_before_serialization() {
    let temp_dir = tempdir().unwrap();
    let secret = crate::secret_scanning::synthetic_github_pat();
    let mut content = "x".repeat(1024 * 1024);
    content.push('\n');
    content.push_str(&secret);
    fs::write(temp_dir.path().join("large-secret.txt"), content).unwrap();
    let mut tree = FileTree::default();
    tree.file_paths.push("large-secret.txt".to_string());
    let scanner = SecretScanner::from_bundled_for_workers(2).unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let mut timings = ProcessingTimings::default();

    let xml = serialize_repository_xml(
        &Params::default(),
        &tree,
        &[],
        temp_dir.path(),
        Some(&scanner),
        &mut reporter,
        &mut timings,
    )
    .unwrap();

    let xml = String::from_utf8(xml).unwrap();
    assert!(!xml.contains(&secret));
    assert!(xml.contains("[Secret removed: GitHub Personal Access Token]"));
    assert_eq!(timings.text_files_scanned, 1);
    assert!(timings.findings_redacted >= 1);
    assert!(timings.secret_scanning > Duration::ZERO);
}

#[test]
fn test_path_only_finding_omits_text_with_safe_fixed_placeholder() {
    let temp_dir = tempdir().unwrap();
    let path = "certificate.p12";
    fs::write(temp_dir.path().join(path), "private decoded text").unwrap();
    let mut tree = FileTree::default();
    tree.file_paths.push(path.to_string());
    let rules = r#"
[[rules]]
id = 'private-path-classification'
description = 'private rule description'
path = '\.p12$'
"#;
    let scanner = SecretScanner::from_rules_for_workers(rules, 1).unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let mut timings = ProcessingTimings::default();

    let xml = serialize_repository_xml(
        &Params::default(),
        &tree,
        &[],
        temp_dir.path(),
        Some(&scanner),
        &mut reporter,
        &mut timings,
    )
    .unwrap();
    let file = parse_file(&xml, path);
    let serialized = String::from_utf8(xml).unwrap();

    assert!(file.text.is_empty());
    assert_eq!(
        file.comments,
        [" Text content omitted because the file type may contain secrets "]
    );
    assert!(!serialized.contains("private decoded text"));
    assert!(!serialized.contains("private-path-classification"));
    assert!(!serialized.contains("private rule description"));
    assert!(!serialized.contains("File path matches pattern"));
    assert_eq!(timings.findings_redacted, 0);
}

#[test]
fn test_path_only_finding_wins_over_ordinary_spans() {
    let temp_dir = tempdir().unwrap();
    let path = "certificate.p12";
    fs::write(temp_dir.path().join(path), "token=SECRETAA").unwrap();
    let mut tree = FileTree::default();
    tree.file_paths.push(path.to_string());
    let rules = r#"
[[rules]]
id = 'path-only'
path = '\.p12$'

[[rules]]
id = 'ordinary'
regex = 'token=([A-Z]{8})'
keywords = ['token=']
secretGroup = 1
"#;
    let scanner = SecretScanner::from_rules_for_workers(rules, 2).unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let mut timings = ProcessingTimings::default();

    let xml = serialize_repository_xml(
        &Params::default(),
        &tree,
        &[],
        temp_dir.path(),
        Some(&scanner),
        &mut reporter,
        &mut timings,
    )
    .unwrap();
    let file = parse_file(&xml, path);

    assert!(file.text.is_empty());
    assert_eq!(
        file.comments,
        [" Text content omitted because the file type may contain secrets "]
    );
    assert_eq!(timings.findings_redacted, 1);
}

#[test]
fn test_disabled_secret_scan_summary_does_not_claim_protection() {
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let params = Params {
        secret_scan: false,
        ..Params::default()
    };
    let xml = serialize_repository_xml(
        &params,
        &FileTree::default(),
        &[],
        tempdir().unwrap().path(),
        None,
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();
    let text = String::from_utf8(xml).unwrap();

    assert!(text.contains("Secret scanning was disabled for this bundle."));
    assert!(!text.contains("detected secrets in their paths"));
    assert!(!text.contains("Repository skipped"));
    assert!(!text.contains("likely secret-bearing"));
}

#[test]
fn test_utf16_conversion_precedes_secret_scanning() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");
    let secret = crate::secret_scanning::synthetic_github_pat();
    let source = format!("token = {secret}\n");
    let mut bytes = vec![0xff, 0xfe];
    for unit in source.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(temp_dir.path().join("secret.txt"), bytes).unwrap();
    let params = Params {
        output_file: Some(output_file.to_string_lossy().into_owned()),
        utf8: true,
        ..Params::default()
    };
    let mut tree = FileTree::default();
    tree.file_paths.push("secret.txt".to_string());
    let scanner = SecretScanner::from_bundled().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = ProcessingTimings::default();

    output_repo_as_xml_with_scanner_and_timings(
        &params,
        tree,
        temp_dir.path(),
        &Model::GPT4.to_tokenizer().unwrap(),
        "GPT-4",
        Some(&scanner),
        &mut reporter,
        &mut timings,
    )
    .unwrap();

    let xml = fs::read_to_string(output_file).unwrap();
    assert!(!xml.contains(&secret));
    assert!(
        xml.contains("token = [Secret removed: GitHub Personal Access Token]")
    );
    assert_eq!(timings.transcoded_files, 1);
    assert_eq!(timings.text_files_scanned, 1);
}

#[test]
fn test_scanner_preserves_no_finding_xml_bytes() {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join("safe.txt"), "ordinary text").unwrap();
    let mut tree = FileTree::default();
    tree.file_paths.push("safe.txt".to_string());
    let params = Params {
        stdout: true,
        ..Params::default()
    };
    let scanner = SecretScanner::from_bundled().unwrap();
    let mut first_reporter =
        ProgressReporter::new(Vec::new(), Vec::new(), true);
    let without_scanner = serialize_repository_xml(
        &params,
        &tree,
        &[],
        temp_dir.path(),
        None,
        &mut first_reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();
    let mut second_reporter =
        ProgressReporter::new(Vec::new(), Vec::new(), true);
    let with_scanner = serialize_repository_xml(
        &params,
        &tree,
        &[],
        temp_dir.path(),
        Some(&scanner),
        &mut second_reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert_eq!(with_scanner, without_scanner);
}

#[test]
fn test_unrelated_xml_invalid_text_stays_omitted_after_redaction() {
    let temp_dir = tempdir().unwrap();
    let secret = crate::secret_scanning::synthetic_github_pat();
    fs::write(
        temp_dir.path().join("invalid.txt"),
        format!("token = {secret}\u{000b}tail"),
    )
    .unwrap();
    let mut tree = FileTree::default();
    tree.file_paths.push("invalid.txt".to_string());
    let scanner = SecretScanner::from_bundled().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);
    let xml = serialize_repository_xml(
        &Params::default(),
        &tree,
        &[],
        temp_dir.path(),
        Some(&scanner),
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    let text = String::from_utf8(xml).unwrap();
    assert!(!text.contains(&secret));
    assert!(text.contains("Text content omitted: XML 1.0 cannot represent"));
    let (normal, diagnostic) = reporter.into_parts();
    assert!(!String::from_utf8(normal).unwrap().contains(&secret));
    assert!(!String::from_utf8(diagnostic).unwrap().contains(&secret));
    assert!(!text.contains("<repository_skipped>"));
}

fn assert_secret_path_diagnostics(xml: &[u8]) {
    let skipped = parse_skipped(xml);
    assert_eq!(skipped.len(), 2);
    assert_eq!(
        skipped[0],
        [
            ("kind".to_string(), "subtree".to_string()),
            ("reason".to_string(), "secret-in-path".to_string()),
            (
                "path".to_string(),
                "fixtures/[Secret removed: GitHub Personal Access Token]"
                    .to_string(),
            ),
            (
                "secret-type".to_string(),
                "GitHub Personal Access Token".to_string(),
            ),
        ]
    );
    assert_eq!(
        skipped[1],
        [
            ("kind".to_string(), "file".to_string()),
            ("reason".to_string(), "secret-in-path".to_string()),
            (
                "path".to_string(),
                "root-[Secret removed: GitHub Personal Access Token].env"
                    .to_string(),
            ),
            (
                "secret-type".to_string(),
                "GitHub Personal Access Token".to_string(),
            ),
        ]
    );
}

fn assert_secret_path_section_order(xml: &[u8]) {
    let root_sections = parse_document(xml)
        .into_iter()
        .filter_map(|event| match event {
            ReaderXmlEvent::StartElement { name, .. }
                if matches!(
                    name.local_name.as_str(),
                    "file_summary"
                        | "repository_structure"
                        | "repository_skipped"
                        | "repository_files"
                ) =>
            {
                Some(name.local_name)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        root_sections,
        [
            "file_summary",
            "repository_structure",
            "repository_skipped",
            "repository_files",
        ]
    );
}

#[test]
fn test_secret_path_omissions_are_safe_consistent_and_parser_backed() {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join("included.txt"), "safe content").unwrap();
    let secret = crate::secret_scanning::synthetic_github_pat();
    let scanner = SecretScanner::from_bundled().unwrap();
    let path_scan = scanner
        .scan_repository_paths(vec![
            format!("root-{secret}.env"),
            format!("fixtures/{secret}/one.txt"),
            format!("fixtures/{secret}/nested/two.txt"),
            "included.txt".to_string(),
        ])
        .unwrap();
    let tree = crate::filelist::group_files_by_directory(path_scan.included);
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let xml = serialize_repository_xml(
        &Params::default(),
        &tree,
        &path_scan.skipped,
        temp_dir.path(),
        Some(&scanner),
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert!(!String::from_utf8_lossy(&xml).contains(&secret));
    assert_eq!(
        parse_structure_files(&xml),
        [(vec![], "included.txt".to_string())]
    );
    assert_eq!(parse_file(&xml, "included.txt").text, "safe content");
    assert_secret_path_diagnostics(&xml);
    assert_secret_path_section_order(&xml);
}

#[test]
fn test_invalid_metadata_after_path_redaction_cannot_echo_secret() {
    let secret = crate::secret_scanning::synthetic_github_pat();
    let scanner = SecretScanner::from_bundled().unwrap();
    let path_scan = scanner
        .scan_repository_paths(vec![format!("{secret}\u{000b}.txt")])
        .unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);

    let error = serialize_repository_xml(
        &Params::default(),
        &FileTree::default(),
        &path_scan.skipped,
        tempdir().unwrap().path(),
        Some(&scanner),
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!error.to_string().contains(&secret));
    assert!(error.to_string().contains("U+000B"));
}

#[test]
fn test_skipped_secret_type_attribute_is_optional() {
    let skipped = [SkippedRepositoryItem {
        kind: crate::secret_scanning::SkippedItemKind::File,
        safe_path: "[Secret removed].env".to_string(),
        reason: SkipReason::SecretInPath { secret_type: None },
    }];
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let xml = serialize_repository_xml(
        &Params::default(),
        &FileTree::default(),
        &skipped,
        tempdir().unwrap().path(),
        None,
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert_eq!(
        parse_skipped(&xml),
        [vec![
            ("kind".to_string(), "file".to_string()),
            ("reason".to_string(), "secret-in-path".to_string()),
            ("path".to_string(), "[Secret removed].env".to_string()),
        ]]
    );
}

#[test]
fn test_skipped_attributes_round_trip_xml_sensitive_characters() {
    let safe_path = "safe & < \" path";
    let secret_type = "Type & < \" marker";
    let skipped = [SkippedRepositoryItem {
        kind: crate::secret_scanning::SkippedItemKind::Subtree,
        safe_path: safe_path.to_string(),
        reason: SkipReason::SecretInPath {
            secret_type: Some(secret_type.to_string()),
        },
    }];
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let xml = serialize_repository_xml(
        &Params::default(),
        &FileTree::default(),
        &skipped,
        tempdir().unwrap().path(),
        None,
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    assert_eq!(
        parse_skipped(&xml),
        [vec![
            ("kind".to_string(), "subtree".to_string()),
            ("reason".to_string(), "secret-in-path".to_string()),
            ("path".to_string(), safe_path.to_string()),
            ("secret-type".to_string(), secret_type.to_string()),
        ]]
    );
}

#[test]
fn test_reused_timings_subtract_only_per_call_file_phase_deltas() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");
    let params = Params {
        output_file: Some(output_file.to_string_lossy().into_owned()),
        ..Params::default()
    };
    let tokenizer = Model::GPT4.to_tokenizer().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = ProcessingTimings {
        file_classification_and_read: Duration::from_secs(1),
        utf8_validation_or_transcode: Duration::from_secs(1),
        ..ProcessingTimings::default()
    };

    output_repo_as_xml_with_timings(
        &params,
        FileTree::default(),
        temp_dir.path(),
        &tokenizer,
        "GPT-4",
        &mut reporter,
        &mut timings,
    )
    .unwrap();
    let after_first_call = timings.xml_generation;

    output_repo_as_xml_with_timings(
        &params,
        FileTree::default(),
        temp_dir.path(),
        &tokenizer,
        "GPT-4",
        &mut reporter,
        &mut timings,
    )
    .unwrap();

    assert!(after_first_call > Duration::ZERO);
    assert!(timings.xml_generation > after_first_call);
}

#[test]
fn test_output_repo_with_line_numbers() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");

    // Create the test file with multiple lines
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "Line 1\nLine 2\nLine 3").unwrap();

    let params = Params {
        output_file: Some(output_file.to_str().unwrap().to_string()),
        line_numbers: true,
        ..Params::default()
    };

    let mut file_tree = FileTree::default();
    file_tree.file_paths.push("test.txt".to_string());

    let tokenizer = Model::GPT4.to_tokenizer().unwrap();

    let result =
        output_repo_as_xml(&params, file_tree, temp_dir.path(), &tokenizer);
    assert!(result.is_ok());

    let xml_content = fs::read_to_string(output_file).unwrap();
    assert!(xml_content.contains("1  Line 1"));
    assert!(xml_content.contains("2  Line 2"));
    assert!(xml_content.contains("3  Line 3"));
}

#[test]
fn test_binary_file_handling() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");

    // Create a binary file
    let test_file = temp_dir.path().join("test.bin");
    fs::write(&test_file, [0u8, 159u8, 146u8, 150u8]).unwrap();

    let params = Params {
        output_file: Some(output_file.to_str().unwrap().to_string()),
        ..Params::default()
    };

    let mut file_tree = FileTree::default();
    file_tree.file_paths.push("test.bin".to_string());

    let tokenizer = Model::GPT4.to_tokenizer().unwrap();

    let result =
        output_repo_as_xml(&params, file_tree, temp_dir.path(), &tokenizer);
    assert!(result.is_ok());

    let xml_content = fs::read(output_file).unwrap();
    let file = parse_file(&xml_content, "test.bin");
    assert_eq!(attribute(&file, "size"), "4");
    assert_eq!(attribute(&file, "lines"), "0");
    assert!(file.text.is_empty());
    assert!(file.comments[0].contains("binary file and not included"));
}

#[test]
fn test_stdout_output() {
    let temp_dir = tempdir().unwrap();

    // Create the test file
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "Test content").unwrap();

    let params = Params {
        stdout: true,
        ..Params::default()
    };

    let mut file_tree = FileTree::default();
    file_tree.file_paths.push("test.txt".to_string());

    let tokenizer = Model::GPT4.to_tokenizer().unwrap();

    let result =
        output_repo_as_xml(&params, file_tree, temp_dir.path(), &tokenizer);
    assert!(result.is_ok());
    let (num_files, size, _) = result.unwrap();
    assert_eq!(num_files, 1);
    assert_eq!(size, 0); // Size is 0 for stdout output
}

#[test]
fn test_utf8_encoding() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");

    // Create a test file with non-UTF8 content
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, b"Hello \xFF World").unwrap(); // Invalid UTF-8 sequence

    let params = Params {
        output_file: Some(output_file.to_str().unwrap().to_string()),
        utf8: true,
        ..Params::default()
    };

    let mut file_tree = FileTree::default();
    file_tree.file_paths.push("test.txt".to_string());

    let tokenizer = Model::GPT4.to_tokenizer().unwrap();

    let result =
        output_repo_as_xml(&params, file_tree, temp_dir.path(), &tokenizer);
    assert!(result.is_ok());

    let xml_content = fs::read_to_string(output_file).unwrap();
    assert!(xml_content.contains("<file path=\"test.txt\""));
    // The content should be readable as UTF-8
    assert!(String::from_utf8(xml_content.as_bytes().to_vec()).is_ok());
}

#[test]
fn test_encoding_fixture_matrix_is_included_as_valid_utf8_xml() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");
    let mut file_tree = FileTree::default();
    for fixture in ENCODING_FIXTURES {
        fs::write(temp_dir.path().join(fixture.name), fixture.bytes).unwrap();
        file_tree.file_paths.push(fixture.name.to_string());
    }
    let params = Params {
        output_file: Some(output_file.to_string_lossy().into_owned()),
        utf8: true,
        ..Params::default()
    };

    output_repo_as_xml(
        &params,
        file_tree,
        temp_dir.path(),
        &Model::GPT4.to_tokenizer().unwrap(),
    )
    .unwrap();

    let xml = fs::read_to_string(output_file).unwrap();
    for fixture in ENCODING_FIXTURES {
        let start = format!("<file path=\"{}\"", fixture.name);
        let file_xml = xml.split(&start).nth(1).unwrap();
        assert!(
            file_xml
                .split("</file>")
                .next()
                .unwrap()
                .contains(fixture.expected)
        );
    }
}

#[test]
fn test_utf16_fixtures_are_excluded_when_conversion_is_disabled() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");
    let mut file_tree = FileTree::default();
    for (name, bytes) in [
        ("utf-16le.txt", UTF16LE_BYTES),
        ("utf-16be.txt", UTF16BE_BYTES),
    ] {
        fs::write(temp_dir.path().join(name), bytes).unwrap();
        file_tree.file_paths.push(name.to_string());
    }
    let params = Params {
        output_file: Some(output_file.to_string_lossy().into_owned()),
        utf8: false,
        ..Params::default()
    };

    output_repo_as_xml(
        &params,
        file_tree,
        temp_dir.path(),
        &Model::GPT4.to_tokenizer().unwrap(),
    )
    .unwrap();

    let xml = fs::read_to_string(output_file).unwrap();
    assert_eq!(xml.matches("binary file and not included").count(), 2);
    assert!(!xml.contains("UTF-16 text with"));
}

#[test]
fn test_progress_phases_and_conversion_follow_execution_order() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");
    fs::write(temp_dir.path().join("legacy.txt"), WINDOWS_1252_BYTES).unwrap();
    let params = Params {
        output_file: Some(output_file.to_string_lossy().into_owned()),
        utf8: true,
        ..Params::default()
    };
    let mut file_tree = FileTree::default();
    file_tree.file_paths.push("legacy.txt".to_string());
    let tokenizer = Model::GPT4.to_tokenizer().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = ProcessingTimings::default();

    reporter.phase("Loading tokenizer for GPT-4").unwrap();
    reporter.phase("Reading files and generating XML").unwrap();
    output_repo_as_xml_with_timings(
        &params,
        file_tree,
        temp_dir.path(),
        &tokenizer,
        "GPT-4",
        &mut reporter,
        &mut timings,
    )
    .unwrap();
    reporter.normal_line("-> Successfully wrote XML").unwrap();

    let (normal, diagnostic) = reporter.into_parts();
    assert_eq!(
        String::from_utf8(normal).unwrap(),
        format!(
            "-> Loading tokenizer for GPT-4\n\
             -> Reading files and generating XML\n\
             -> Converted 'legacy.txt' from windows-1252 to UTF-8\n\
             -> Counting tokens with GPT-4\n\
             -> Writing result to '{}'\n\
             -> Successfully wrote XML\n",
            output_file.display()
        )
    );
    assert!(diagnostic.is_empty());
}

#[test]
fn test_utf16_replacement_warning_is_emitted_once() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");
    let mut malformed = UTF16LE_BYTES.to_vec();
    malformed.pop();
    fs::write(temp_dir.path().join("malformed.txt"), malformed).unwrap();
    let params = Params {
        output_file: Some(output_file.to_string_lossy().into_owned()),
        utf8: true,
        ..Params::default()
    };
    let mut file_tree = FileTree::default();
    file_tree.file_paths.push("malformed.txt".to_string());
    let tokenizer = Model::GPT4.to_tokenizer().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);

    output_repo_as_xml_with_timings(
        &params,
        file_tree,
        temp_dir.path(),
        &tokenizer,
        "GPT-4",
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    let (normal, diagnostic) = reporter.into_parts();
    let normal = String::from_utf8(normal).unwrap();
    assert_eq!(normal.matches("-> Converted 'malformed.txt'").count(), 1);
    assert_eq!(
        String::from_utf8(diagnostic).unwrap(),
        "warning: 'malformed.txt' decoded as UTF-16LE with replacement characters; information was lost\n"
    );
}

#[test]
fn test_malformed_utf8_bom_emits_one_replacement_warning() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");
    fs::write(
        temp_dir.path().join("malformed.txt"),
        b"\xef\xbb\xbfmalformed \xff text",
    )
    .unwrap();
    let params = Params {
        output_file: Some(output_file.to_string_lossy().into_owned()),
        utf8: true,
        ..Params::default()
    };
    let mut file_tree = FileTree::default();
    file_tree.file_paths.push("malformed.txt".to_string());
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);

    output_repo_as_xml_with_timings(
        &params,
        file_tree,
        temp_dir.path(),
        &Model::GPT4.to_tokenizer().unwrap(),
        "GPT-4",
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    let (normal, diagnostic) = reporter.into_parts();
    assert!(!String::from_utf8(normal).unwrap().contains("Converted"));
    assert_eq!(
        String::from_utf8(diagnostic).unwrap(),
        "warning: 'malformed.txt' contained malformed UTF-8 and was decoded with replacement characters; information was lost\n"
    );
    let xml = fs::read_to_string(output_file).unwrap();
    assert!(xml.contains("\u{feff}malformed \u{fffd} text"));
}

#[test]
fn test_clean_utf8_bom_emits_no_conversion_or_replacement_message() {
    let temp_dir = tempdir().unwrap();
    let output_file = temp_dir.path().join("output.xml");
    fs::write(temp_dir.path().join("clean.txt"), b"\xef\xbb\xbfclean")
        .unwrap();
    let params = Params {
        output_file: Some(output_file.to_string_lossy().into_owned()),
        utf8: true,
        ..Params::default()
    };
    let mut file_tree = FileTree::default();
    file_tree.file_paths.push("clean.txt".to_string());
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);

    output_repo_as_xml_with_timings(
        &params,
        file_tree,
        temp_dir.path(),
        &Model::GPT4.to_tokenizer().unwrap(),
        "GPT-4",
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    let (normal, diagnostic) = reporter.into_parts();
    assert!(!String::from_utf8(normal).unwrap().contains("Converted"));
    assert!(diagnostic.is_empty());
}

#[test]
fn test_malformed_utf8_bom_is_silent_with_quiet_reporter() {
    let temp_dir = tempdir().unwrap();
    fs::write(
        temp_dir.path().join("malformed.txt"),
        b"\xef\xbb\xbfmalformed \xff text",
    )
    .unwrap();
    let paths = vec!["malformed.txt".to_string()];
    let params = Params {
        stdout: true,
        utf8: true,
        ..Params::default()
    };
    let mut writer = EmitterConfig::new()
        .perform_indent(true)
        .write_document_declaration(false)
        .create_writer(Vec::new());
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);

    write_repository_files_to_xml(
        &mut writer,
        &paths,
        temp_dir.path(),
        &params,
        None,
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    let xml = writer.into_inner();
    assert!(
        String::from_utf8(xml)
            .unwrap()
            .contains("\u{feff}malformed \u{fffd} text")
    );
    let (normal, diagnostic) = reporter.into_parts();
    assert!(normal.is_empty());
    assert!(diagnostic.is_empty());
}
