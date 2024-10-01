mod filelist;
mod repo;
mod xml_output;

use clap::{ArgAction, Parser};
use std::path::PathBuf;
use std::process::exit;

#[derive(Parser)]
#[command(
    name = "Repopack Clone Tool",
    author = env!("CARGO_PKG_AUTHORS"),
    about =env!("CARGO_PKG_DESCRIPTION"),
    long_about = None,
)]

struct Cli {
    #[arg(
        help = "GitHub repository to clone (e.g. 'user/repo' or full GitHub \
                URL). If not provided, the current directory will be searched \
                for a Git repository."
    )]
    repo: Option<String>,
    #[arg(
        short,
        long,
        help = "GitHub personal access token (required for private repos and \
                to pass rate limits)"
    )]
    token: Option<String>,
    #[arg(
        long = "version",
        short = 'V',
        action = ArgAction::SetTrue,
        help = "Print version information and exit",
        global = true
    )]
    version: bool,
}

fn main() {
    let args = Cli::parse();

    if args.version {
        println!("{}", version_info());
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

    let grouped_files = filelist::group_files_by_directory(file_list);
    if let Err(e) = xml_output::output_filelist_as_xml(grouped_files) {
        eprintln!("Failed to write XML: {}", e);
    } else {
        println!("File list successfully written to filelist.xml");
    }
}

pub fn version_info() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let authors = env!("CARGO_PKG_AUTHORS");
    let description = env!("CARGO_PKG_DESCRIPTION");

    // Provide default values if fields are empty
    let authors = if authors.is_empty() {
        "Unknown"
    } else {
        authors
    };
    let description = if description.is_empty() {
        "No description provided"
    } else {
        description
    };

    format!(
        "bundle_repo v{}\n\
        \n{}\n\
        \nReleased under the MIT license by {}\n",
        version, description, authors
    )
}
