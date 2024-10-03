use std::path::PathBuf;
use std::process::exit;

use clap::Parser;
use tokenizer::Model;

mod cli;
mod filelist;
mod repo;
mod tokenizer;
mod xml_output;

fn main() {
    let args = cli::Flags::parse();

    if args.version {
        println!("{}", cli::version_info());
        exit(0);
    }

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

    if let Err(e) = xml_output::output_repo_as_xml(
        &args.output_file,
        file_tree,
        &base_path,
        &tokenizer,
    ) {
        eprintln!("Failed to write XML: {}", e);
    } else {
        println!(
            "Repository Dump successfully written to {}",
            args.output_file
        );
    }
}
