mod filelist;
mod repo;
mod xml_output;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "Repopack Clone Tool")]
#[command(author = "Your Name")]
#[command(version = "1.0")]
#[command(about = "Clone a repo or check if the current folder is a Git repo", long_about = None)]
struct Cli {
    repo: Option<String>,
    #[arg(short, long)]
    token: Option<String>,
}

fn main() {
    let args = Cli::parse();

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

    let grouped_files = filelist::group_files_by_directory(file_list);
    if let Err(e) = xml_output::output_filelist_as_xml(grouped_files) {
        eprintln!("Failed to write XML: {}", e);
    } else {
        println!("File list successfully written to filelist.xml");
    }
}
