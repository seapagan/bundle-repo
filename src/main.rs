use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Instant;

use clap::Parser;
use config::{Config, File, FileFormat};
use dirs_next::home_dir;
use structs::Params;
use tabled::{
    settings::{
        object::{Columns, Rows},
        Alignment, Modify, Remove, Style,
    },
    Table, Tabled,
};
use tempfile::tempdir;
use tokenizer::Model;

mod cli;
mod embedded;
mod filelist;
mod progress;
mod repo;
mod structs;
mod text_processing;
mod timings;
mod tokenizer;
mod xml_output;

#[derive(Tabled)]
struct SummaryTable {
    // metric: &'static str,
    metric: String,
    value: String,
}

fn load_config() -> Params {
    let mut config_builder = Config::builder();

    // Get the home directory and construct the global config path
    if let Some(home_dir) = home_dir() {
        let global_config_path =
            home_dir.join(".config/bundlerepo/config.toml");

        // Add global config as the base if it exists
        if global_config_path.exists() {
            config_builder = config_builder.add_source(File::new(
                global_config_path.to_str().unwrap(),
                FileFormat::Toml,
            ));
        }
    }

    // Check for local config file in the current directory
    let local_config_path = Path::new(".bundlerepo.toml");
    if local_config_path.exists() {
        // Add local config as an override
        config_builder = config_builder.add_source(File::new(
            local_config_path.to_str().unwrap(),
            FileFormat::Toml,
        ));
    }

    match config_builder.build() {
        Ok(config) => config.into(),
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            Params::default()
        }
    }
}

fn main() {
    let args = cli::Flags::parse();
    let timing_enabled = timings::ProcessingTimings::enabled_from_env();
    let mut timings = timings::ProcessingTimings::default();

    if args.version {
        println!("{}", cli::version_info());
        exit(0);
    }

    // Load config values
    let config = load_config();
    let params = Params::from_args_and_config(&args, config);

    if let Err(error) = xml_output::validate_output_options(&params) {
        eprintln!("Error: {error}");
        exit(1);
    }

    if !params.stdout {
        cli::show_header();
    }

    let mut reporter = progress::ProgressReporter::new(
        std::io::stdout(),
        std::io::stderr(),
        params.stdout,
    );

    // Parse the tokenizer Model from the CLI argument. We will build the
    // tokenizer from this and also use it to display the model name in the
    // summary.
    let model = match params.model.clone().unwrap().parse::<Model>() {
        Ok(model) => model,
        Err(e) => {
            reporter.error(&e).unwrap();
            exit(1);
        }
    };

    // Create the tokenizer from the parsed model
    reporter
        .phase(&format!("Loading tokenizer for {}", model.display_name()))
        .unwrap();
    let tokenizer_start = Instant::now();
    let tokenizer = match model.to_tokenizer() {
        Ok(tokenizer) => tokenizer,
        Err(e) => {
            reporter
                .error(&format!("Error: Failed to create tokenizer: {e}"))
                .unwrap();
            exit(1);
        }
    };
    timings.tokenizer_load = tokenizer_start.elapsed();

    // Create a temporary directory for cloning the repository
    let temp_dir = tempdir().unwrap();
    let repo_folder = if let Some(ref repo_input) = args.repo {
        match repo::clone_repo(
            &params,
            repo_input,
            params.token.as_deref(),
            temp_dir.path(),
        ) {
            Ok(repo_folder) => repo_folder,
            Err(e) => {
                eprintln!("Error: {}", e);
                exit(2);
            }
        }
    } else if let Err(e) = repo::check_current_directory(&params) {
        eprintln!("Error: {}", e);
        exit(3);
    } else {
        PathBuf::from(".")
    };

    // List and group files
    let file_list = filelist::list_files_in_repo(
        &repo_folder,
        params.extend_exclude.as_deref(),
        params.exclude.as_deref(),
    );
    let file_tree = filelist::group_files_by_directory(file_list);

    // Output XML
    reporter.phase("Reading files and generating XML").unwrap();
    match xml_output::output_repo_as_xml_with_timings(
        &params,
        file_tree,
        &repo_folder,
        &tokenizer,
        model.display_name(),
        &mut reporter,
        &mut timings,
    ) {
        Ok((number_of_files, total_size, token_count)) => {
            if !params.stdout {
                // Print the summary only if not using stdout
                if params.clipboard {
                    reporter
                        .normal_line("-> Successfully copied XML to clipboard")
                        .unwrap();
                } else {
                    reporter
                        .normal_line(&format!(
                            "-> Successfully wrote XML to '{}'",
                            xml_output::effective_output_file(&params)
                                .display()
                        ))
                        .unwrap();
                }
                let summary_data = vec![
                    SummaryTable {
                        metric: "Total Files processed:".to_string(),
                        value: number_of_files.to_string(),
                    },
                    SummaryTable {
                        metric: "Total output size (bytes):".to_string(),
                        value: total_size.to_string(),
                    },
                    SummaryTable {
                        metric: format!(
                            "Token count ({}):",
                            model.display_name()
                        ),
                        value: token_count.to_string(),
                    },
                ];

                // Build and print the table
                let table = Table::new(summary_data)
                    .with(Remove::row(Rows::first()))
                    .with(Style::empty())
                    .with(Modify::list(Columns::first(), Alignment::right()))
                    .to_string();

                reporter
                    .normal_text(&format!("\nSummary:\n{table}\n\n"))
                    .unwrap();
            }
            if timing_enabled {
                let _ = timings.write_records(&mut std::io::stderr().lock());
            }
        }
        Err(e) => {
            reporter
                .error(&format!("X  Failed to write XML: {e}"))
                .unwrap();
            exit(4);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Flags;
    use crate::text_processing::{
        read_classify_and_decode, BinaryReason, ProcessedFile,
    };
    use clap::Parser;
    use std::fs;
    use std::str::FromStr;

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
    fn test_model_parsing() {
        use std::str::FromStr;

        // Test valid model parsing
        let model = Model::from_str("gpt5").unwrap();
        assert!(matches!(model, Model::GPT5));

        // Test invalid model
        let invalid = Model::from_str("invalid_model");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_summary_table_formatting() {
        let summary_data = vec![
            SummaryTable {
                metric: "Files:".to_string(),
                value: "10".to_string(),
            },
            SummaryTable {
                metric: "Size:".to_string(),
                value: "1024".to_string(),
            },
        ];

        let table = Table::new(summary_data)
            .with(Remove::row(Rows::first()))
            .with(Style::empty())
            .with(Modify::list(Columns::first(), Alignment::right()))
            .to_string();

        // Check that the table is formatted correctly
        assert!(table.contains("Files:"));
        assert!(table.contains("10"));
        assert!(table.contains("Size:"));
        assert!(table.contains("1024"));
    }

    #[test]
    fn test_version_flag() {
        let args = Flags::parse_from(["bundlerepo", "--version"]);
        assert!(args.version);
    }

    #[test]
    fn test_model_parsing_error() {
        let result = Model::from_str("invalid_model");
        assert!(result.is_err());
        let err = result.unwrap_err();
        println!("Error: {}", err);
        assert!(err.contains("Unsupported model"));
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Not a git repository"));
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
    fn test_summary_table_with_clipboard() {
        let summary_data = vec![
            SummaryTable {
                metric: "Files:".to_string(),
                value: "10".to_string(),
            },
            SummaryTable {
                metric: "Size:".to_string(),
                value: "1024".to_string(),
            },
        ];

        let table = Table::new(summary_data)
            .with(Remove::row(Rows::first()))
            .with(Style::empty())
            .with(Modify::list(Columns::first(), Alignment::right()))
            .to_string();

        // Check that the table is formatted correctly with clipboard settings
        assert!(table.contains("Files:"));
        assert!(table.contains("10"));
        assert!(table.contains("Size:"));
        assert!(table.contains("1024"));
    }

    #[test]
    fn test_tokenizer_creation() {
        // Test successful tokenizer creation
        let model = Model::GPT4o;
        let tokenizer_result = model.to_tokenizer();
        assert!(tokenizer_result.is_ok());

        // Test that we can use the tokenizer
        let tokenizer = tokenizer_result.unwrap();
        let count = tokenizer.count_tokens("test string").unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_empty_input_tokenization() {
        let model = Model::GPT4o;
        let tokenizer = model.to_tokenizer().unwrap();
        let count = tokenizer.count_tokens("").unwrap();
        // Just verify the tokenization succeeded
        assert_eq!(count, 0);
    }

    #[test]
    fn test_model_display_in_summary() {
        let model = Model::GPT4o;
        let summary_data = vec![SummaryTable {
            metric: format!("Token count ({}):", model.display_name()),
            value: "100".to_string(),
        }];

        let table = Table::new(summary_data)
            .with(Remove::row(Rows::first()))
            .with(Style::empty())
            .with(Modify::list(Columns::first(), Alignment::right()))
            .to_string();

        assert!(table
            .contains(&format!("Token count ({}):", model.display_name())));
        assert!(table.contains("100"));
    }

    #[test]
    fn test_invalid_model_parsing() {
        // Test direct model parsing without CLI
        let result = "invalid_model".parse::<Model>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unsupported model"));
    }

    #[test]
    fn test_invalid_tokenizer_creation() {
        // This is a bit of a contrived test since our current models all create valid tokenizers
        // but it ensures we handle empty input correctly
        let model = Model::GPT4o;
        let tokenizer = model.to_tokenizer().unwrap();
        let count = tokenizer.count_tokens("test string").unwrap();
        assert!(count > 0);
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
            include_bytes!("../tests/fixtures/encodings/utf-16le.txt"),
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
            let params = Params::from_args_and_config(
                &args,
                create_test_config(config),
            );
            assert_eq!(params.utf8, expected_utf8);

            let mut timings = timings::ProcessingTimings::default();
            let processed = read_classify_and_decode(
                &fixture_path,
                params.utf8,
                &mut timings,
            )
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
                    ProcessedFile::Binary(
                        BinaryReason::Utf16ConversionDisabled("UTF-16LE")
                    )
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
            let params = Params::from_args_and_config(
                &args,
                create_test_config(config),
            );
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
}
