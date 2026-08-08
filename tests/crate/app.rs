use super::*;
use crate::cli::Flags;
use crate::text_processing::{
    BinaryReason, ProcessedFile, read_classify_and_decode,
};
use clap::Parser;
use std::fs;

fn create_test_config(toml_content: &str) -> Params {
    let config = Config::builder()
        .add_source(config::File::from_str(
            toml_content,
            config::FileFormat::Toml,
        ))
        .build()
        .unwrap();
    config.into()
}

#[test]
fn test_local_config_overrides_global_config() {
    let temp_dir = tempdir().unwrap();
    let global_config = temp_dir.path().join("global.toml");
    let local_config = temp_dir.path().join("local.toml");
    fs::write(
        &global_config,
        "model = \"gpt4\"\nline_numbers = true\ngzip_level = 8\n",
    )
    .unwrap();
    fs::write(
        &local_config,
        "model = \"gpt5\"\ngzip = true\ngzip_level = 3\n",
    )
    .unwrap();

    let params = load_config_from_paths(Some(&global_config), &local_config);

    assert_eq!(params.model.as_deref(), Some("gpt5"));
    assert!(params.line_numbers);
    assert!(params.gzip);
    assert_eq!(params.gzip_level, 3);
}

#[test]
fn test_invalid_config_falls_back_to_defaults() {
    let temp_dir = tempdir().unwrap();
    let local_config = temp_dir.path().join("invalid.toml");
    fs::write(&local_config, "model = [").unwrap();

    let params = load_config_from_paths(None, &local_config);

    assert_eq!(params, Params::default());
}

#[test]
fn test_success_report_names_file_and_metrics() {
    let params = Params {
        output_file: Some("result.xml".to_string()),
        ..Params::default()
    };
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);

    report_success(&params, Model::GPT4o, (3, 2048, 512), &mut reporter)
        .unwrap();

    let (normal, diagnostic) = reporter.into_parts();
    let normal = String::from_utf8(normal).unwrap();
    assert!(normal.starts_with("-> Successfully wrote XML to 'result.xml'\n"));
    assert!(normal.contains("Total Files processed:  3"));
    assert!(normal.contains("Total output size (bytes):  2048"));
    assert!(normal.contains("Token count (GPT-4o):  512"));
    assert!(diagnostic.is_empty());
}

#[test]
fn test_success_report_names_clipboard_destination() {
    let params = Params {
        clipboard: true,
        ..Params::default()
    };
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);

    report_success(&params, Model::GPT5, (1, 2, 3), &mut reporter).unwrap();

    let (normal, diagnostic) = reporter.into_parts();
    assert!(normal.starts_with(b"-> Successfully copied XML to clipboard\n"));
    assert!(diagnostic.is_empty());
}

#[test]
fn test_success_report_is_silent_for_stdout_output() {
    let params = Params {
        stdout: true,
        ..Params::default()
    };
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);

    report_success(&params, Model::GPT5, (1, 2, 3), &mut reporter).unwrap();

    let (normal, diagnostic) = reporter.into_parts();
    assert!(normal.is_empty());
    assert!(diagnostic.is_empty());
}

#[test]
fn test_prepare_tokenizer_reports_invalid_model() {
    let params = Params {
        model: Some("unknown".to_string()),
        ..Params::default()
    };
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = timings::ProcessingTimings::default();

    let error =
        prepare_tokenizer(&params, &mut reporter, &mut timings).unwrap_err();

    assert!(error.starts_with("ERROR: Unsupported model: unknown."));
    let (normal, diagnostic) = reporter.into_parts();
    assert!(normal.is_empty());
    assert!(diagnostic.is_empty());
    assert!(timings.tokenizer_load.is_zero());
}

#[test]
fn test_prepare_tokenizer_announces_selected_model() {
    let params = Params {
        model: Some("gpt4".to_string()),
        ..Params::default()
    };
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = timings::ProcessingTimings::default();

    let (model, _) =
        prepare_tokenizer(&params, &mut reporter, &mut timings).unwrap();

    assert_eq!(model, Model::GPT4);
    let (normal, diagnostic) = reporter.into_parts();
    assert_eq!(normal, b"-> Loading tokenizer for GPT-4\n");
    assert!(diagnostic.is_empty());
}

#[test]
fn test_exclude_takes_precedence_over_extend_exclude() {
    // Setup CLI args with both exclude and extend-exclude
    let args = Flags::parse_from([
        "program",
        "--exclude",
        "*.txt",
        "--extend-exclude",
        "*.md",
    ]);

    // Create config with both exclude and extend-exclude
    let config = create_test_config(
        r#"
            extend_exclude = ["*.rs"]
            exclude = ["*.toml"]
        "#,
    );

    let params = Params::from_args_and_config(&args, config);

    // Verify that extend_exclude is None when exclude is set
    assert!(params.extend_exclude.is_none());
    // Verify that exclude contains only CLI patterns
    assert_eq!(params.exclude, Some(vec!["*.txt".to_string()]));
}

#[test]
fn test_cli_exclude_overrides_config_exclude() {
    let args = Flags::parse_from([
        "program",
        "--exclude",
        "*.txt",
        "--exclude",
        "*.md",
    ]);

    let config = create_test_config(
        r#"
            exclude = ["*.toml", "*.rs"]
        "#,
    );

    let params = Params::from_args_and_config(&args, config);

    assert_eq!(
        params.exclude,
        Some(vec!["*.txt".to_string(), "*.md".to_string()])
    );
}

#[test]
fn test_extend_exclude_combines_cli_and_config() {
    let args = Flags::parse_from([
        "program",
        "--extend-exclude",
        "*.txt",
        "--extend-exclude",
        "*.md",
    ]);

    let config = create_test_config(
        r#"
            extend_exclude = ["*.toml", "*.rs"]
        "#,
    );

    let params = Params::from_args_and_config(&args, config);

    assert_eq!(
        params.extend_exclude,
        Some(vec![
            "*.txt".to_string(),
            "*.md".to_string(),
            "*.toml".to_string(),
            "*.rs".to_string()
        ])
    );
}

#[test]
fn test_config_exclude_disables_extend_exclude() {
    let args = Flags::parse_from(["program", "--extend-exclude", "*.txt"]);

    let config = create_test_config(
        r#"
            exclude = ["*.toml"]
            extend_exclude = ["*.rs"]
        "#,
    );

    let params = Params::from_args_and_config(&args, config);

    assert!(params.extend_exclude.is_none());
    assert_eq!(params.exclude, Some(vec!["*.toml".to_string()]));
}

#[test]
fn test_no_exclude_patterns() {
    let args = Flags::parse_from(["program"]);
    let config = create_test_config("");

    let params = Params::from_args_and_config(&args, config);

    assert!(params.exclude.is_none());
    assert!(params.extend_exclude.is_none());
}

#[test]
fn test_repo_clone_error() {
    let temp_dir = tempdir().unwrap();
    let args = Flags::parse_from(["bundlerepo", "invalid_repo"]);
    let config = Params::default();
    let params = Params::from_args_and_config(&args, config);
    let result =
        repo::clone_repo(&params, "invalid_repo", None, temp_dir.path());
    assert!(result.is_err());
}

#[test]
fn test_current_directory_check() {
    let temp_dir = tempdir().unwrap();
    let params = Params::default();
    std::env::set_current_dir(temp_dir.path()).unwrap();
    let result = repo::check_current_directory(&params);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Not a git repository")
    );
}

#[test]
fn test_xml_output_error() {
    let temp_dir = tempdir().unwrap();
    let params = Params {
        output_file: Some("/nonexistent/directory/output.xml".to_string()),
        ..Params::default()
    };
    let file_tree = filelist::group_files_by_directory(vec![]);
    let model = Model::GPT4o;
    let tokenizer = model.to_tokenizer().unwrap();
    let result = xml_output::output_repo_as_xml(
        &params,
        file_tree,
        temp_dir.path(),
        &tokenizer,
    );
    assert!(result.is_err());
}

#[test]
fn test_utf8_flag_overrides_config_false() {
    let config = create_test_config(
        r#"
            utf8 = false
        "#,
    );
    let args = Flags::parse_from(["program", "--utf8"]);
    let params = Params::from_args_and_config(&args, config);
    assert!(params.utf8);
}

#[test]
fn test_no_utf8_flag_overrides_config_true() {
    let config = create_test_config(
        r#"
            utf8 = true
        "#,
    );
    let args = Flags::parse_from(["program", "--no-utf8"]);
    let params = Params::from_args_and_config(&args, config);
    assert!(!params.utf8);
}

#[test]
fn test_config_utf8_used_when_no_flags() {
    let config = create_test_config(
        r#"
            utf8 = true
        "#,
    );
    let args = Flags::parse_from(["program"]);
    let params = Params::from_args_and_config(&args, config);
    assert!(params.utf8);
}

#[test]
fn test_default_utf8_when_no_config_or_flags() {
    let config = create_test_config("");
    let args = Flags::parse_from(["program"]);
    let params = Params::from_args_and_config(&args, config);
    assert!(!params.utf8);
}

#[test]
fn test_utf8_precedence_controls_utf16_conversion() {
    let temp_dir = tempdir().unwrap();
    let fixture_path = temp_dir.path().join("utf-16le.txt");
    fs::write(
        &fixture_path,
        include_bytes!("../fixtures/encodings/utf-16le.txt"),
    )
    .unwrap();
    let cases: [(&str, &[&str], bool); 6] = [
        ("utf8 = false", &["program"], false),
        ("utf8 = false", &["program", "--utf8"], true),
        ("utf8 = false", &["program", "--no-utf8"], false),
        ("utf8 = true", &["program"], true),
        ("utf8 = true", &["program", "--utf8"], true),
        ("utf8 = true", &["program", "--no-utf8"], false),
    ];

    for (config, arguments, expected_utf8) in cases {
        let args = Flags::parse_from(arguments);
        let params =
            Params::from_args_and_config(&args, create_test_config(config));
        assert_eq!(params.utf8, expected_utf8);

        let mut timings = timings::ProcessingTimings::default();
        let processed =
            read_classify_and_decode(&fixture_path, params.utf8, &mut timings)
                .unwrap();
        if expected_utf8 {
            match processed {
                ProcessedFile::Text(decoded) => {
                    assert!(decoded.text.starts_with("UTF-16 text"));
                    assert_eq!(timings.transcoded_files, 1);
                }
                ProcessedFile::Binary(reason) => {
                    panic!("expected decoded UTF-16, got {reason:?}")
                }
            }
        } else {
            assert_eq!(
                processed,
                ProcessedFile::Binary(BinaryReason::Utf16ConversionDisabled(
                    "UTF-16LE"
                ))
            );
            assert_eq!(timings.transcoded_files, 0);
        }
    }
}

#[test]
fn test_gzip_cli_and_config_precedence() {
    let cases = [
        ("", vec!["program"], false, 6),
        ("gzip = false\ngzip_level = 9", vec!["program"], false, 9),
        ("", vec!["program", "-z"], true, 6),
        ("gzip = true", vec!["program"], true, 6),
        ("gzip = true\ngzip_level = 8", vec!["program"], true, 8),
        (
            "gzip = false\ngzip_level = 9",
            vec!["program", "-z"],
            true,
            9,
        ),
        (
            "gzip = true\ngzip_level = 8",
            vec!["program", "-z=3"],
            true,
            3,
        ),
        (
            "gzip = true\ngzip_level = 8",
            vec!["program", "--no-gzip"],
            false,
            8,
        ),
    ];

    for (config, arguments, expected_gzip, expected_level) in cases {
        let args = Flags::parse_from(arguments);
        let params =
            Params::from_args_and_config(&args, create_test_config(config));
        assert_eq!(params.gzip, expected_gzip);
        assert_eq!(params.gzip_level, expected_level);
    }
}

#[test]
fn test_no_gzip_allows_clipboard_override() {
    let config = create_test_config("gzip = true\ngzip_level = 9");
    let args = Flags::parse_from(["program", "--no-gzip", "--clipboard"]);
    let params = Params::from_args_and_config(&args, config);
    assert!(!params.gzip);
    assert!(params.clipboard);
}
