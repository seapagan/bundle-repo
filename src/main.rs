use std::path::PathBuf;
use std::process::exit;

use clap::Parser;
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
mod tokenizer;
mod xml_output;

#[derive(Tabled)]
struct SummaryTable {
    // metric: &'static str,
    metric: String,
    value: String,
}

fn main() {
    let args = cli::Flags::parse();

    if args.version {
        println!("{}", cli::version_info());
        exit(0);
    }

    cli::show_header();

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

    // Create a temporary directory in main and pass it to clone_repo
    let temp_dir = tempdir().unwrap();
    let repo_folder = if let Some(ref repo_input) = args.repo {
        match repo::clone_repo(
            repo_input,
            args.token.as_deref(),
            temp_dir.path(),
        ) {
            Ok(repo_folder) => repo_folder,
            Err(e) => {
                eprintln!("Error: {}", e);
                return;
            }
        }
    } else if let Err(e) = repo::check_current_directory() {
        eprintln!("Error: {}", e);
        return;
    } else {
        PathBuf::from(".")
    };

    // List and group files
    let file_list = filelist::list_files_in_repo(&repo_folder);
    let file_tree = filelist::group_files_by_directory(file_list);

    // Output XML
    match xml_output::output_repo_as_xml(
        &args.output_file,
        file_tree,
        &repo_folder,
        &tokenizer,
    ) {
        Ok((number_of_files, total_size, token_count)) => {
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
                    metric: format!("Token count ({}):", model.display_name()),
                    value: token_count.to_string(),
                },
            ];

            // Build and print the table
            let table = Table::new(summary_data)
                .with(Disable::row(Rows::first()))
                .with(Style::empty())
                .with(Modify::list(Columns::first(), Alignment::right()))
                .to_string();

            println!("-> Succesfully wrote XML to {}", args.output_file);
            println!("\nSummary:");
            println!("{}\n", table);
        }
        Err(e) => {
            eprintln!("X  Failed to write XML: {}", e);
        }
    }
}
