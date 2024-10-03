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

    let model = match args.model.parse::<Model>() {
        Ok(model) => model,
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        }
    };

    if args.version {
        println!("{}", cli::version_info());
        exit(0);
    }

    cli::show_header();

    // Parse the model from the CLI argument
    let tokenizer = match args.model.parse::<Model>() {
        Ok(model) => model.to_tokenizer().unwrap(),
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        }
    };

    let file_list = if let Some(ref repo_input) = args.repo {
        // Borrow repo_input
        match repo::clone_repo(repo_input, args.token.as_deref()) {
            Ok(repo_folder) => filelist::list_files_in_repo(&repo_folder),
            Err(e) => {
                eprintln!("Error: {}", e);
                return;
            }
        }
    } else if let Err(e) = repo::check_current_directory() {
        eprintln!("Error: {}", e);
        return;
    } else {
        let repo_path = PathBuf::from(".");
        filelist::list_files_in_repo(&repo_path)
    };

    let file_tree = filelist::group_files_by_directory(file_list);

    let base_path = if let Some(ref repo_input) = args.repo {
        // Borrow repo_input here as well
        match repo::clone_repo(repo_input, args.token.as_deref()) {
            Ok(repo_folder) => repo_folder,
            Err(_) => PathBuf::from("."), // Fall back to current directory
        }
    } else {
        PathBuf::from(".")
    };

    match xml_output::output_repo_as_xml(
        &args.output_file,
        file_tree,
        &base_path,
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
