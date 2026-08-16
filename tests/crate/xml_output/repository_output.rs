use super::*;

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
fn test_parallel_scanner_finishes_before_large_file_serialization() {
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
