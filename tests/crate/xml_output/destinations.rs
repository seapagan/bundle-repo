use super::*;

#[test]
fn test_quiet_reporter_keeps_plain_and_gzip_stdout_bytes_clean() {
    let xml = b"<repository>legacy text</repository>\n";
    for gzip in [false, true] {
        let mut output = Vec::new();
        let mut timings = ProcessingTimings::default();
        let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
        reporter.phase("Hidden phase").unwrap();
        reporter
            .conversion(
                "legacy.txt",
                &crate::text_processing::ConversionReport {
                    source_encoding: "windows-1252",
                    had_replacements: true,
                },
            )
            .unwrap();
        reporter
            .malformed_utf8_replacement("malformed.txt")
            .unwrap();

        write_stdout(&mut output, xml, gzip, 6, &mut timings).unwrap();
        let (normal, diagnostic) = reporter.into_parts();
        assert!(normal.is_empty());
        assert!(diagnostic.is_empty());
        if gzip {
            assert_eq!(&output[..2], &[0x1f, 0x8b]);
            let mut decoded = Vec::new();
            GzDecoder::new(output.as_slice())
                .read_to_end(&mut decoded)
                .unwrap();
            assert_eq!(decoded, xml);
        } else {
            assert_eq!(output, xml);
        }
    }
}

#[test]
fn test_destination_phase_messages_cover_all_destinations() {
    let cases = [
        (
            Params {
                output_file: Some("result.xml".to_string()),
                ..Params::default()
            },
            "-> Writing result to 'result.xml'\n",
        ),
        (
            Params {
                output_file: Some("result.xml".to_string()),
                gzip: true,
                ..Params::default()
            },
            "-> Compressing and writing result to 'result.xml.gz'\n",
        ),
        (
            Params {
                clipboard: true,
                ..Params::default()
            },
            "-> Copying result to clipboard\n",
        ),
    ];

    for (params, expected) in cases {
        let mut reporter =
            ProgressReporter::new(Vec::new(), Vec::new(), false);
        report_destination(&params, &mut reporter).unwrap();

        let (normal, diagnostic) = reporter.into_parts();
        assert_eq!(String::from_utf8(normal).unwrap(), expected);
        assert!(diagnostic.is_empty());
    }
}

#[test]
fn test_gzip_file_round_trip_and_metrics() {
    let temp_dir = tempdir().unwrap();
    let secret = crate::secret_scanning::synthetic_github_pat();
    fs::write(
        temp_dir.path().join("test.txt"),
        format!("token = {secret}"),
    )
    .unwrap();
    let tokenizer = Model::GPT4.to_tokenizer().unwrap();
    let scanner = SecretScanner::from_bundled().unwrap();

    let file_tree = || {
        let mut tree = FileTree::default();
        tree.file_paths.push("test.txt".to_string());
        tree
    };

    let plain_path = temp_dir.path().join("plain.xml");
    let plain = Params {
        output_file: Some(plain_path.to_string_lossy().into_owned()),
        ..Params::default()
    };
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = ProcessingTimings::default();
    let (_, plain_size, plain_tokens) =
        output_repo_as_xml_with_scanner_and_timings(
            &plain,
            file_tree(),
            temp_dir.path(),
            &tokenizer,
            "GPT-4",
            Some(&scanner),
            &mut reporter,
            &mut timings,
        )
        .unwrap();
    let expected_xml = fs::read(&plain_path).unwrap();
    assert_eq!(plain_size, expected_xml.len() as u64);
    assert!(
        !expected_xml
            .windows(secret.len())
            .any(|window| window == secret.as_bytes())
    );
    assert!(
        expected_xml
            .windows(b"[Secret removed: GitHub Personal Access Token]".len())
            .any(|window| {
                window == b"[Secret removed: GitHub Personal Access Token]"
            })
    );

    for level in [1, 9] {
        let requested_path =
            temp_dir.path().join(format!("level-{level}.xml"));
        let compressed = Params {
            output_file: Some(requested_path.to_string_lossy().into_owned()),
            gzip: true,
            gzip_level: level,
            ..Params::default()
        };

        let (_, compressed_size, compressed_tokens) =
            output_repo_as_xml_with_scanner_and_timings(
                &compressed,
                file_tree(),
                temp_dir.path(),
                &tokenizer,
                "GPT-4",
                Some(&scanner),
                &mut reporter,
                &mut timings,
            )
            .unwrap();
        let effective_path = format!("{}.gz", requested_path.display());
        let gzip_bytes = fs::read(&effective_path).unwrap();
        assert_eq!(&gzip_bytes[..2], &[0x1f, 0x8b]);
        assert_eq!(compressed_size, gzip_bytes.len() as u64);
        assert_eq!(compressed_tokens, plain_tokens);

        let mut decoded = Vec::new();
        GzDecoder::new(gzip_bytes.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, expected_xml);
    }
}

#[test]
fn test_file_creation_error_reports_effective_output_path() {
    let home = tempdir().unwrap();
    let params = Params {
        output_file: Some("~/missing/bundle.xml".to_string()),
        gzip: true,
        ..Params::default()
    };
    let output_path =
        effective_output_file_with_home(&params, Some(home.path()));
    let source_error = File::create(&output_path).unwrap_err();

    let error = create_output_file(&output_path).unwrap_err();

    assert_eq!(error.kind(), source_error.kind());
    assert_eq!(
        error.to_string(),
        format!(
            "failed to create output file '{}': {source_error}",
            home.path().join("missing/bundle.xml.gz").display()
        )
    );
}

#[test]
fn test_gzip_effective_filename_keeps_existing_suffix() {
    let params = Params {
        output_file: Some("bundle.XML.GZ".to_string()),
        gzip: true,
        ..Params::default()
    };
    assert_eq!(effective_output_file(&params), Path::new("bundle.XML.GZ"));

    let default_name = Params {
        output_file: None,
        gzip: true,
        ..Params::default()
    };
    assert_eq!(
        effective_output_file(&default_name),
        Path::new(&format!("{DEFAULT_OUTPUT_FILE}.gz"))
    );
}

#[test]
fn test_config_home_relative_output_path_is_expanded() {
    let home = tempdir().unwrap();
    let config = config::Config::builder()
        .set_override("output_file", "~/Documents/packed-repo.xml")
        .unwrap()
        .build()
        .unwrap();
    let params = Params::from(config);

    assert_eq!(
        effective_output_file_with_home(&params, Some(home.path())),
        home.path().join("Documents/packed-repo.xml")
    );
}

#[test]
fn test_cli_home_relative_output_path_is_expanded() {
    let home = tempdir().unwrap();
    let params = Params {
        output_file: Some("~/quoted-output.xml".to_string()),
        ..Params::default()
    };

    assert_eq!(
        effective_output_file_with_home(&params, Some(home.path())),
        home.path().join("quoted-output.xml")
    );
}

#[test]
fn test_non_home_relative_output_paths_are_unchanged() {
    let home = tempdir().unwrap();
    let absolute_path = home.path().join("absolute.xml");

    for output_file in [
        PathBuf::from("relative/output.xml"),
        absolute_path,
        PathBuf::from("~other/output.xml"),
    ] {
        let params = Params {
            output_file: Some(output_file.to_string_lossy().into_owned()),
            ..Params::default()
        };
        assert_eq!(
            effective_output_file_with_home(&params, Some(home.path())),
            output_file
        );
    }
}

#[test]
fn test_gzip_suffix_is_applied_after_home_expansion() {
    let home = tempdir().unwrap();
    let params = Params {
        output_file: Some("~/packed-repo.xml".to_string()),
        gzip: true,
        ..Params::default()
    };
    assert_eq!(
        effective_output_file_with_home(&params, Some(home.path())),
        home.path().join("packed-repo.xml.gz")
    );

    let existing_suffix = Params {
        output_file: Some("~/packed-repo.XML.GZ".to_string()),
        gzip: true,
        ..Params::default()
    };
    assert_eq!(
        effective_output_file_with_home(&existing_suffix, Some(home.path())),
        home.path().join("packed-repo.XML.GZ")
    );
}

#[test]
fn test_home_relative_output_path_is_unchanged_without_home() {
    let params = Params {
        output_file: Some("~/packed-repo.xml".to_string()),
        ..Params::default()
    };

    assert_eq!(
        effective_output_file_with_home(&params, None),
        Path::new("~/packed-repo.xml")
    );
}

#[test]
fn test_bare_home_component_is_not_expanded() {
    let home = tempdir().unwrap();

    for output_file in ["~", "~/"] {
        let params = Params {
            output_file: Some(output_file.to_string()),
            ..Params::default()
        };
        assert_eq!(
            effective_output_file_with_home(&params, Some(home.path()))
                .as_os_str(),
            std::ffi::OsStr::new(output_file)
        );
    }
}

#[test]
fn test_gzip_bare_home_component_stays_relative() {
    let home = tempdir().unwrap();

    for (output_file, expected) in [("~", "~.gz"), ("~/", "~/.gz")] {
        let params = Params {
            output_file: Some(output_file.to_string()),
            gzip: true,
            ..Params::default()
        };
        assert_eq!(
            effective_output_file_with_home(&params, Some(home.path())),
            Path::new(expected)
        );
    }
}

#[cfg(windows)]
#[test]
fn test_windows_home_relative_output_path_is_expanded() {
    let home = tempdir().unwrap();
    let params = Params {
        output_file: Some(r"~\Documents\packed-repo.xml".to_string()),
        ..Params::default()
    };

    assert_eq!(
        effective_output_file_with_home(&params, Some(home.path())),
        home.path().join(r"Documents\packed-repo.xml")
    );
}

#[test]
fn test_gzip_stdout_bytes_round_trip() {
    let xml = b"<repository />\n";
    let mut output = Vec::new();
    let mut timings = ProcessingTimings::default();
    write_stdout(&mut output, xml, true, 6, &mut timings).unwrap();
    assert_eq!(&output[..2], &[0x1f, 0x8b]);
    assert!(!timings.compression.is_zero());
    assert!(!timings.output_write_or_copy.is_zero());

    let mut decoded = Vec::new();
    GzDecoder::new(output.as_slice())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, xml);
}

#[test]
fn test_all_testable_destinations_use_canonical_serialization_bytes() {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join("test.txt"), "a <tag> & ]]> tail").unwrap();
    let mut expected_tree = FileTree::default();
    expected_tree.file_paths.push("test.txt".to_string());
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), true);
    let expected = serialize_repository_xml(
        &Params::default(),
        &expected_tree,
        temp_dir.path(),
        None,
        &mut reporter,
        &mut ProcessingTimings::default(),
    )
    .unwrap();

    let plain_path = temp_dir.path().join("plain.xml");
    let plain_params = Params {
        output_file: Some(plain_path.to_string_lossy().into_owned()),
        ..Params::default()
    };
    let mut plain_tree = FileTree::default();
    plain_tree.file_paths.push("test.txt".to_string());
    output_repo_as_xml(
        &plain_params,
        plain_tree,
        temp_dir.path(),
        &Model::GPT4.to_tokenizer().unwrap(),
    )
    .unwrap();
    assert_eq!(fs::read(plain_path).unwrap(), expected);

    let mut stdout = Vec::new();
    write_stdout(
        &mut stdout,
        &expected,
        false,
        6,
        &mut ProcessingTimings::default(),
    )
    .unwrap();
    assert_eq!(stdout, expected);

    let mut gzip_stdout = Vec::new();
    write_stdout(
        &mut gzip_stdout,
        &expected,
        true,
        6,
        &mut ProcessingTimings::default(),
    )
    .unwrap();
    let mut decoded = Vec::new();
    GzDecoder::new(gzip_stdout.as_slice())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, expected);

    let clipboard_text = String::from_utf8(expected.clone()).unwrap();
    assert_eq!(clipboard_text.as_bytes(), expected);
}

#[test]
fn test_gzip_clipboard_is_rejected() {
    let temp_dir = tempdir().unwrap();
    let params = Params {
        clipboard: true,
        gzip: true,
        ..Params::default()
    };
    let tokenizer = Model::GPT4.to_tokenizer().unwrap();
    let error = output_repo_as_xml(
        &params,
        FileTree::default(),
        temp_dir.path(),
        &tokenizer,
    )
    .unwrap_err();
    assert!(error.to_string().contains("--no-gzip --clipboard"));
}

#[test]
fn test_gzip_stdout_takes_precedence_over_clipboard() {
    let params = Params {
        stdout: true,
        clipboard: true,
        gzip: true,
        ..Params::default()
    };
    assert!(validate_output_options_for(&params, false).is_ok());
}

#[test]
fn test_gzip_stdout_rejects_terminal_output() {
    let params = Params {
        stdout: true,
        gzip: true,
        ..Params::default()
    };
    let error = validate_output_options_for(&params, true).unwrap_err();
    assert!(error.to_string().contains("redirect stdout"));
}

#[test]
fn test_uncompressed_stdout_preserves_canonical_bytes() {
    let mut output = Vec::new();
    let mut timings = ProcessingTimings::default();
    write_stdout(&mut output, b"xml\n", false, 6, &mut timings).unwrap();
    assert_eq!(output, b"xml\n");
    assert!(timings.compression.is_zero());
    assert!(!timings.output_write_or_copy.is_zero());
}

#[test]
fn test_uncompressed_stdout_rejects_invalid_utf8_without_writing() {
    let mut output = Vec::new();
    let mut timings = ProcessingTimings::default();

    let error =
        write_stdout(&mut output, b"invalid \xff", false, 6, &mut timings)
            .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(output.is_empty());
    assert!(timings.compression.is_zero());
    assert!(timings.output_write_or_copy.is_zero());
}

#[test]
fn test_finish_output_rejects_invalid_utf8_before_counting_or_writing() {
    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("invalid.xml");
    let params = Params {
        output_file: Some(output_path.to_string_lossy().into_owned()),
        ..Params::default()
    };
    let tokenizer = Model::GPT4.to_tokenizer().unwrap();
    let mut reporter = ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = ProcessingTimings::default();

    let error = finish_output(
        &params,
        1,
        b"invalid \xff".to_vec(),
        &tokenizer,
        "GPT-4",
        &mut reporter,
        &mut timings,
    )
    .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!output_path.exists());
    assert!(timings.token_count.is_zero());
    assert!(timings.output_write_or_copy.is_zero());
    let (normal, diagnostic) = reporter.into_parts();
    assert!(normal.is_empty());
    assert!(diagnostic.is_empty());
}
