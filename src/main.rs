use std::path::PathBuf;
use std::process::exit;

use clap::Parser;

mod cli;
mod filelist;
mod repo;
mod xml_output;

fn main() {
    let args = cli::Flags::parse();

    if args.version {
        println!("{}", cli::version_info());
        exit(0);
    }

    let file_list = if let Some(repo_input) = args.repo {
        match repo::clone_repo(&repo_input, args.token.as_deref()) {
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
    if let Err(e) = xml_output::output_repo_as_xml(file_tree) {
        eprintln!("Failed to write XML: {}", e);
    } else {
        println!("File list successfully written to filelist.xml");
    }
}
