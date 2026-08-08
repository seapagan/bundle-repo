use super::*;

#[test]
fn test_xml10_character_boundaries() {
    for character in [
        '\0', '\u{0001}', '\u{0008}', '\u{000b}', '\u{000c}', '\u{000e}',
        '\u{001f}', '\u{fffe}', '\u{ffff}',
    ] {
        let value = format!("before{character}after");
        let invalid = first_invalid_xml10_char(&value).unwrap();
        assert_eq!(invalid.character, character);
        assert_eq!(invalid.byte_index, "before".len());
    }

    for character in [
        '\t',
        '\n',
        '\r',
        '\u{0020}',
        '\u{d7ff}',
        '\u{e000}',
        '\u{fffd}',
        '\u{10000}',
        '\u{10ffff}',
    ] {
        assert_eq!(first_invalid_xml10_char(&character.to_string()), None);
    }
}

#[test]
fn test_metadata_validation_rejects_each_entry_point_before_file_access() {
    let cases = [
        ("repository file path", 0, "missing\\u{b}file", 7),
        ("repository structure file path", 1, "bad\\u{b}basename", 3),
        ("repository structure folder name", 2, "bad\\u{b}folder", 3),
        (
            "repository structure folder name",
            3,
            "nested\\u{b}folder",
            6,
        ),
    ];
    for (expected_role, location, escaped_value, byte_index) in cases {
        let mut tree = FileTree::default();
        match location {
            0 => tree.file_paths.push("missing\u{000b}file".to_string()),
            1 => tree
                .folder_node
                .files
                .push("bad\u{000b}basename".to_string()),
            2 => {
                tree.folder_node.subfolders.insert(
                    "bad\u{000b}folder".to_string(),
                    FolderNode::default(),
                );
            }
            3 => {
                let mut parent = FolderNode::default();
                parent.subfolders.insert(
                    "nested\u{000b}folder".to_string(),
                    FolderNode::default(),
                );
                tree.folder_node
                    .subfolders
                    .insert("parent".to_string(), parent);
            }
            _ => unreachable!(),
        }

        let error = validate_file_tree_xml_metadata(&tree).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            error.to_string(),
            format!(
                "{expected_role} \"{escaped_value}\" contains U+000B at byte index {byte_index}, which cannot be represented in XML 1.0"
            )
        );
    }
}

#[test]
fn test_tab_metadata_rejection_records_writer_normalization_contract() {
    let mut output = Vec::new();
    {
        let mut writer = EmitterConfig::new()
            .perform_indent(false)
            .create_writer(&mut output);
        writer
            .write(XmlEvent::start_element("root").attr("path", "a\tb"))
            .unwrap();
        writer.write(XmlEvent::end_element()).unwrap();
    }
    let parsed = parse_document(&output);
    let value = parsed
        .iter()
        .find_map(|event| match event {
            ReaderXmlEvent::StartElement { attributes, .. } => attributes
                .iter()
                .find(|attribute| attribute.name.local_name == "path")
                .map(|attribute| attribute.value.as_str()),
            _ => None,
        })
        .unwrap();
    assert_eq!(value, "a b");

    let mut tree = FileTree::default();
    tree.file_paths.push("a\tb".to_string());
    let error = validate_file_tree_xml_metadata(&tree).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(
        error.to_string(),
        "repository file path \"a\\tb\" contains U+0009 at byte index 1, which cannot round-trip through XML attributes with the resolved writer"
    );
}

#[test]
fn test_complete_document_has_one_root_and_literal_file_instruction() {
    let xml = serialize_single_file(b"ordinary text", false);
    let events = parse_document(&xml);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                ReaderXmlEvent::StartDocument { .. }
            ))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, ReaderXmlEvent::EndDocument))
            .count(),
        1
    );
    let element_names = events
        .iter()
        .filter_map(|event| match event {
            ReaderXmlEvent::StartElement { name, .. } => {
                Some(name.local_name.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    for expected in [
        "repository",
        "file_summary",
        "purpose",
        "file_format",
        "instructions",
        "usage_guidelines",
        "notes",
        "additional_info",
        "repository_structure",
        "repository_files",
    ] {
        assert!(element_names.contains(&expected));
    }
    let prose = events
        .iter()
        .filter_map(|event| match event {
            ReaderXmlEvent::Characters(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert!(prose.contains("Each <file> should be interpreted"));
    assert!(!element_names.contains(&".py"));
}

#[test]
fn test_cdata_content_matrix_round_trips_through_complete_documents() {
    let cases = [
        "ordinary text",
        "<tag attr='single' other=\"double\"> & text > tail",
        "literal </file><injected>markup</injected>",
        "]]>",
        "before]]>middle]]>after",
        "]]>at start and at end]]>",
        "日本語 العربية Кириллица café 😀 \u{fffd} \u{10000}",
        "",
        "first\r\nsecond\rthird\nfourth",
    ];

    for content in cases {
        let xml = serialize_single_file(content.as_bytes(), false);
        let file = parse_file(&xml, "test.txt");
        let expected = content.replace("\r\n", "\n").replace('\r', "\n");
        assert_eq!(file.text, expected, "failed content {content:?}");
        assert!(String::from_utf8(xml).unwrap().contains("<![CDATA["));
    }
}

#[test]
fn test_embedded_cdata_end_tokens_use_adjacent_sections() {
    let content = "a]]>b]]>c";
    let xml = serialize_single_file(content.as_bytes(), false);
    let serialized = String::from_utf8(xml.clone()).unwrap();
    assert!(serialized.matches("<![CDATA[").count() >= 3);
    assert_eq!(parse_file(&xml, "test.txt").text, content);
}

#[test]
fn test_line_numbered_content_uses_xml_logical_lines() {
    for content in [
        b"alpha\nbeta\ngamma\n".as_slice(),
        b"alpha\r\nbeta\r\ngamma\r\n".as_slice(),
        b"alpha\rbeta\rgamma\n".as_slice(),
    ] {
        let xml = serialize_single_file(content, true);
        let file = parse_file(&xml, "test.txt");
        assert_eq!(file.text, "1  alpha\n2  beta\n3  gamma\n");
        assert_eq!(attribute(&file, "lines"), "3");
    }
}

#[test]
fn test_line_metadata_uses_xml_logical_lines() {
    for content in [
        b"alpha\nbeta\ngamma\n".as_slice(),
        b"alpha\r\nbeta\r\ngamma\r\n".as_slice(),
        b"alpha\rbeta\rgamma\n".as_slice(),
    ] {
        let xml = serialize_single_file(content, false);
        let file = parse_file(&xml, "test.txt");
        assert_eq!(file.text, "alpha\nbeta\ngamma\n");
        assert_eq!(attribute(&file, "lines"), "3");
    }
}

#[test]
fn test_empty_file_remains_empty_with_and_without_line_numbers() {
    for line_numbers in [false, true] {
        let xml = serialize_single_file(b"", line_numbers);
        let file = parse_file(&xml, "test.txt");
        assert_eq!(attribute(&file, "lines"), "0");
        assert!(file.text.is_empty());
    }
}

#[test]
fn test_xml_forbidden_text_is_omitted_without_reclassification() {
    for character in ['\u{000b}', '\u{001f}', '\u{fffe}', '\u{ffff}'] {
        let content =
            format!("a sufficiently long text prefix {character} and suffix");
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("test.txt");
        fs::write(&path, content.as_bytes()).unwrap();
        assert!(matches!(
            read_classify_and_decode(
                &path,
                false,
                &mut ProcessingTimings::default()
            )
            .unwrap(),
            ProcessedFile::Text(_)
        ));

        let xml = serialize_single_file(content.as_bytes(), false);
        let serialized = String::from_utf8(xml.clone()).unwrap();
        let file = parse_file(&xml, "test.txt");
        assert_eq!(attribute(&file, "size"), content.len().to_string());
        assert_eq!(attribute(&file, "lines"), "0");
        assert!(file.text.is_empty());
        assert!(!serialized.contains("<![CDATA["));
        assert_eq!(file.comments.len(), 1);
        assert!(file.comments[0].contains("content omitted"));
        assert!(file.comments[0].contains(&format_code_point(character)));
    }
}

#[test]
fn test_xml_forbidden_text_warning_respects_quiet_reporter() {
    let content = "a sufficiently long text prefix \u{000b} and suffix";
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join("test.txt"), content).unwrap();
    let mut tree = FileTree::default();
    tree.file_paths.push("test.txt".to_string());

    for quiet in [false, true] {
        let mut reporter =
            ProgressReporter::new(Vec::new(), Vec::new(), quiet);
        serialize_repository_xml(
            &Params::default(),
            &tree,
            temp_dir.path(),
            &mut reporter,
            &mut ProcessingTimings::default(),
        )
        .unwrap();
        let (normal, diagnostic) = reporter.into_parts();
        assert!(normal.is_empty());
        if quiet {
            assert!(diagnostic.is_empty());
        } else {
            assert_eq!(
                String::from_utf8(diagnostic).unwrap(),
                "warning: 'test.txt' content was omitted because XML 1.0 cannot represent character U+000B\n"
            );
        }
    }
}

#[test]
fn test_sparse_del_remains_text_and_round_trips_as_xml10() {
    let content = "before\u{007f}after";
    assert!(is_xml10_char('\u{007f}'));
    let temp_dir = tempdir().unwrap();
    let path = temp_dir.path().join("test.txt");
    fs::write(&path, content).unwrap();
    assert!(matches!(
        read_classify_and_decode(
            &path,
            false,
            &mut ProcessingTimings::default()
        )
        .unwrap(),
        ProcessedFile::Text(_)
    ));
    let xml = serialize_single_file(content.as_bytes(), false);
    assert_eq!(parse_file(&xml, "test.txt").text, content);
}

#[test]
fn test_xml_sensitive_metadata_round_trips_in_structure_and_file_entries() {
    let file_name = "file<&\"'.txt";
    let folder_name = "folder<&\"'";
    let mut tree = FileTree::default();
    tree.folder_node.files.push(file_name.to_string());
    tree.folder_node
        .subfolders
        .insert(folder_name.to_string(), FolderNode::default());
    let temp_dir = tempdir().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let xml = serialize_repository_xml(
        &Params::default(),
        &tree,
        temp_dir.path(),
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();
    let events = parse_document(&xml);
    assert!(events.iter().any(|event| matches!(
        event,
        ReaderXmlEvent::StartElement { name, attributes, .. }
            if name.local_name == "file"
                && attributes.iter().any(|attribute|
                    attribute.name.local_name == "path"
                        && attribute.value == file_name)
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ReaderXmlEvent::StartElement { name, attributes, .. }
            if name.local_name == "folder"
                && attributes.iter().any(|attribute|
                    attribute.name.local_name == "name"
                        && attribute.value == folder_name)
    )));

    let entry_xml = serialize_text_entry(file_name, "content");
    assert_eq!(parse_file(&entry_xml, file_name).text, "content");
}

#[test]
fn test_parse_file_selects_repository_content_entry_on_path_collision() {
    let path = "collision.txt";
    let content = "repository content";
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join(path), content).unwrap();
    let mut tree = FileTree::default();
    tree.folder_node.files.push(path.to_string());
    tree.file_paths.push(path.to_string());
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);

    let xml = serialize_repository_xml(
        &Params::default(),
        &tree,
        temp_dir.path(),
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();
    let file = parse_file(&xml, path);

    assert_eq!(attribute(&file, "size"), content.len().to_string());
    assert_eq!(attribute(&file, "lines"), "1");
    assert_eq!(file.text, content);
}

#[test]
fn test_nested_repository_structure_round_trips_with_hierarchy() {
    let mut deepest = FolderNode::default();
    deepest.files.push("deep.txt".to_string());
    let mut middle = FolderNode::default();
    middle.files.push("middle.txt".to_string());
    middle.subfolders.insert("deep".to_string(), deepest);
    let mut tree = FileTree::default();
    tree.folder_node.files.push("root.txt".to_string());
    tree.folder_node
        .subfolders
        .insert("middle".to_string(), middle);
    let temp_dir = tempdir().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);

    let xml = serialize_repository_xml(
        &Params::default(),
        &tree,
        temp_dir.path(),
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();
    assert_eq!(
        parse_structure_files(&xml),
        [
            (Vec::<String>::new(), "root.txt".to_string()),
            (vec!["middle".to_string()], "middle.txt".to_string()),
            (
                vec!["middle".to_string(), "deep".to_string()],
                "deep.txt".to_string(),
            ),
        ]
    );
}

#[test]
fn test_lf_and_cr_metadata_round_trip_exactly() {
    let path = "line\nfeed.txt";
    let entry_xml = serialize_text_entry(path, "content");
    assert!(String::from_utf8_lossy(&entry_xml).contains("&#xA;"));
    validate_xml_attribute(path, "test path").unwrap();
    assert_eq!(attribute(&parse_file(&entry_xml, path), "path"), path);

    let folder_name = "carriage\rreturn";
    let mut tree = FileTree::default();
    tree.folder_node
        .subfolders
        .insert(folder_name.to_string(), FolderNode::default());
    validate_file_tree_xml_metadata(&tree).unwrap();
    let temp_dir = tempdir().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let xml = serialize_repository_xml(
        &Params::default(),
        &tree,
        temp_dir.path(),
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&xml).contains("&#xD;"));
    assert!(parse_document(&xml).iter().any(|event| matches!(
        event,
        ReaderXmlEvent::StartElement { name, attributes, .. }
            if name.local_name == "folder"
                && attributes.iter().any(|attribute|
                    attribute.name.local_name == "name"
                        && attribute.value == folder_name)
    )));
}

#[test]
fn test_read_error_comment_is_xml_safe_and_diagnostic() {
    let diagnostic = "bad -- <tag> & \"quote\" trailing-\u{000b}";
    let xml = serialize_read_error_entry("test.txt", diagnostic);
    let file = parse_file(&xml, "test.txt");
    assert_eq!(attribute(&file, "size"), "0");
    assert_eq!(attribute(&file, "lines"), "0");
    assert!(file.text.is_empty());
    assert_eq!(file.comments.len(), 1);
    assert_eq!(
        file.comments[0],
        " Failed to read file: bad -  <tag> & \"quote\" trailing-[unrepresentable U+000B] "
    );
}

#[test]
fn test_invalid_metadata_creates_no_destination_file() {
    let temp_dir = tempdir().unwrap();
    let output = temp_dir.path().join("must-not-exist.xml");
    let params = Params {
        output_file: Some(output.to_string_lossy().into_owned()),
        ..Params::default()
    };
    let mut tree = FileTree::default();
    tree.file_paths.push("missing\u{000b}file".to_string());
    let error = output_repo_as_xml(
        &params,
        tree,
        temp_dir.path(),
        &Model::GPT4.to_tokenizer().unwrap(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!output.exists());
}
