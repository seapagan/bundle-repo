use super::*;
use crate::cli::Flags;
use crate::text_processing::{
    BinaryReason, ProcessedFile, read_classify_and_decode,
};
use clap::Parser;
use git2::{Repository, Signature};
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

fn initialize_repository(path: &Path) {
    let repo = Repository::init(path).unwrap();
    repo.set_head("refs/heads/test-branch").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "test", &tree, &[])
        .unwrap();
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
    let global_config = temp_dir.path().join("global.toml");
    let local_config = temp_dir.path().join("invalid.toml");
    fs::write(
        &global_config,
        "model = \"gpt4\"\nline_numbers = true\ngzip_level = 9\n",
    )
    .unwrap();
    fs::write(&local_config, "model = [").unwrap();

    let params = load_config_from_paths(Some(&global_config), &local_config);

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
fn test_application_runs_local_repository_and_reports_success() {
    let temp_dir = tempdir().unwrap();
    initialize_repository(temp_dir.path());
    fs::write(temp_dir.path().join("example.txt"), "example content").unwrap();
    let output_path = temp_dir.path().join("output.xml");
    let params = Params {
        output_file: Some(output_path.to_string_lossy().into_owned()),
        ..Params::default()
    };
    let args = Flags::parse_from(["program"]);
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = timings::ProcessingTimings::default();

    run_application(
        &args,
        &params,
        temp_dir.path(),
        &mut reporter,
        &mut timings,
    )
    .unwrap();

    let xml = fs::read_to_string(output_path).unwrap();
    assert!(xml.contains("example.txt"));
    assert!(xml.contains("example content"));
    let (normal, diagnostic) = reporter.into_parts();
    let normal = String::from_utf8(normal).unwrap();
    assert!(normal.contains("-> Loading tokenizer for GPT-5"));
    assert!(normal.contains("-> Reading files and generating XML"));
    assert!(normal.contains("-> Successfully wrote XML to"));
    assert!(diagnostic.is_empty());
}

#[test]
fn test_application_maps_tokenizer_failure_to_exit_code() {
    let params = Params {
        model: Some("unknown".to_string()),
        ..Params::default()
    };
    let args = Flags::parse_from(["program"]);
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = timings::ProcessingTimings::default();

    let error = run_application(
        &args,
        &params,
        Path::new("."),
        &mut reporter,
        &mut timings,
    )
    .unwrap_err();

    assert_eq!(error.exit_code(), 1);
    assert!(
        error
            .to_string()
            .starts_with("ERROR: Unsupported model: unknown.")
    );
}

#[test]
fn test_application_maps_clone_failure_to_exit_code() {
    let temp_dir = tempdir().unwrap();
    let params = Params::default();
    let args = Flags::parse_from(["program", "not a repository"]);
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = timings::ProcessingTimings::default();

    let error = run_application(
        &args,
        &params,
        temp_dir.path(),
        &mut reporter,
        &mut timings,
    )
    .unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert_eq!(error.to_string(), "Error: Invalid repository shorthand");
}

#[test]
fn test_application_maps_discovery_failure_to_exit_code() {
    let temp_dir = tempdir().unwrap();
    let params = Params::default();
    let args = Flags::parse_from(["program"]);
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = timings::ProcessingTimings::default();

    let error = run_application(
        &args,
        &params,
        temp_dir.path(),
        &mut reporter,
        &mut timings,
    )
    .unwrap_err();

    assert_eq!(error.exit_code(), 3);
    assert_eq!(error.to_string(), "Error: Not a git repository");
}

#[test]
fn test_application_maps_output_failure_to_exit_code() {
    let temp_dir = tempdir().unwrap();
    initialize_repository(temp_dir.path());
    let params = Params {
        output_file: Some(
            temp_dir
                .path()
                .join("missing/directory/output.xml")
                .to_string_lossy()
                .into_owned(),
        ),
        ..Params::default()
    };
    let args = Flags::parse_from(["program"]);
    let mut reporter =
        progress::ProgressReporter::new(Vec::new(), Vec::new(), false);
    let mut timings = timings::ProcessingTimings::default();

    let error = run_application(
        &args,
        &params,
        temp_dir.path(),
        &mut reporter,
        &mut timings,
    )
    .unwrap_err();

    assert_eq!(error.exit_code(), 4);
    assert!(error.to_string().starts_with("X  Failed to write XML: "));
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
