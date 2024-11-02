use std::path::PathBuf;
use std::process::exit;

use clap::Parser;
use config::{Config, File, FileFormat};
use dirs_next::home_dir;
use structs::Params;
use tabled::{
    settings::{
        object::{Columns, Rows},
        Alignment, Disable, Modify, Style,
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
    let mut config_path = PathBuf::new();

    // Get the home directory and construct the path
    if let Some(home_dir) = home_dir() {
        config_path.push(home_dir);
        config_path.push(".config/bundlerepo/config.toml");
    }

    let settings = Config::builder()
        .add_source(File::new(config_path.to_str().unwrap(), FileFormat::Toml))
        .build();

    match settings {
        Ok(config) => config.into(), // Convert Config into Params using the From trait
        Err(e) => {
            // If the error is related to the file not being found, return default Params
            if e.to_string().contains("not found") {
                Params::default()
            } else {
                eprintln!("Error loading config: {}", e);
                Params::default()
            }
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

    if !args.stdout {
        cli::show_header();
    }

    // Parse the tokenizer Model from the CLI argument. We will build the
    // tokenizer from this and also use it to display the model name in the
    // summary.
    let model = match args.model.parse::<Model>() {
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
            &args,
            repo_input,
            args.token.as_deref(),
            temp_dir.path(),
        ) {
            Ok(repo_folder) => repo_folder,
            Err(e) => {
                eprintln!("Error: {}", e);
                exit(2);
            }
        }
    } else if let Err(e) = repo::check_current_directory(&args) {
        eprintln!("Error: {}", e);
        exit(3);
    } else {
        PathBuf::from(".")
    };

    // List and group files
    let file_list = filelist::list_files_in_repo(&repo_folder);
    let file_tree = filelist::group_files_by_directory(file_list);

    // Output XML
    // Output XML
    match xml_output::output_repo_as_xml(
        &args,
        file_tree,
        &repo_folder,
        &tokenizer,
    ) {
        Ok((number_of_files, total_size, token_count)) => {
            if !args.stdout {
                // Print the summary only if not using stdout
                if args.clipboard {
                    println!("-> Successfully copied XML to clipboard");
                } else {
                    println!(
                        "-> Successfully wrote XML to {}",
                        args.output_file
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
                    .with(Disable::row(Rows::first()))
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
