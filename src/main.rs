use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Instant;

use clap::Parser;
use config::{Config, File, FileFormat};
use dirs_next::home_dir;
use structs::Params;
use tabled::{
    Table, Tabled,
    settings::{
        Alignment, Modify, Remove, Style,
        object::{Columns, Rows},
    },
};
use tempfile::tempdir;
use tokenizer::Model;

mod cli;
mod embedded;
mod filelist;
mod progress;
mod repo;
mod structs;
#[cfg(test)]
mod test_fixtures;
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
#[path = "../tests/crate/app.rs"]
mod tests;
