use std::path::{Path, PathBuf};
use std::process::exit;

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
mod filelist;
mod repo;
mod structs;
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
        Ok(config) => config.into(), // Convert Config into Params using the From trait
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            Params::default()
        }
    }
}

fn main() {
    let args = cli::Flags::parse();

    if args.version {
        println!("{}", cli::version_info());
        exit(0);
    }

    // Load config values
    let config = load_config();
    let params = Params::from_args_and_config(&args, config);

    if !params.stdout {
        cli::show_header();
    }

    // Parse the tokenizer Model from the CLI argument. We will build the
    // tokenizer from this and also use it to display the model name in the
    // summary.
    let model = match params.model.clone().unwrap().parse::<Model>() {
        Ok(model) => model,
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        }
    };

    // Create the tokenizer from the parsed model
    let tokenizer = match model.to_tokenizer() {
        Ok(tokenizer) => tokenizer,
        Err(e) => {
            eprintln!("Error: Failed to create tokenizer: {}", e);
            exit(1);
        }
    };

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
    match xml_output::output_repo_as_xml(
        &params,
        file_tree,
        &repo_folder,
        &tokenizer,
    ) {
        Ok((number_of_files, total_size, token_count)) => {
            if !params.stdout {
                // Print the summary only if not using stdout
                if params.clipboard {
                    println!("-> Successfully copied XML to clipboard");
                } else {
                    println!(
                        "-> Successfully wrote XML to '{}'",
                        params.output_file.unwrap()
                    );
                }
                println!("\nSummary:");
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

                println!("{}\n", table);
            }
        }
        Err(e) => {
            eprintln!("X  Failed to write XML: {}", e);
            exit(4);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Flags;
    use clap::Parser;

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
}
