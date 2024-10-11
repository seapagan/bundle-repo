use clap::{ArgAction, Parser};

#[derive(Parser)]
#[command(
    name = "Repopack Clone Tool",
    author = env!("CARGO_PKG_AUTHORS"),
    about =env!("CARGO_PKG_DESCRIPTION"),
    long_about = None,
)]

pub struct Flags {
    #[arg(
        help = "GitHub repository to clone (e.g. 'user/repo' or full GitHub \
                URL). If not provided, the current directory will be searched \
                for a Git repository."
    )]
    pub repo: Option<String>,

    #[arg(
        long = "branch",
        short = 'b',
        help = "Specify a branch to checkout for remote repositories"
    )]
    pub branch: Option<String>,

    #[arg(
        long = "file",
        short = 'f',
        help = "Filename to save the bundle as.",
        default_value = "packed-repo.xml"
    )]
    pub output_file: String,

    #[arg(
        long = "stdout",
        short = 's',
        action = clap::ArgAction::SetTrue,
        help = "Output the XML directly to stdout without creating a file."
    )]
    pub stdout: bool,

    #[arg(
        long = "model",
        short = 'm',
        default_value = "gpt4o",
        help = "Model to use for tokenization. Supported \
                models: 'gpt4o', 'gpt4', 'gpt3.5', 'gpt3', 'gpt2'"
    )]
    pub model: String,

    #[arg(
        long = "clipboard",
        short = 'c',
        action = ArgAction::SetTrue,
        help = "Copy the XML to the clipboard after creating it."
    )]
    pub clipboard: bool,

    #[arg(
    long = "lnumbers",
    short = 'l',
    action = clap::ArgAction::SetTrue,
    help = "Add line numbers to each code file in the output."
    )]
    pub lnumbers: bool,

    #[arg(
        short,
        long,
        help = "GitHub personal access token (required for private repos and \
                to pass rate limits)"
    )]
    pub token: Option<String>,

    #[arg(
        long = "version",
        short = 'V',
        action = ArgAction::SetTrue,
        help = "Print version information and exit",
        global = true
    )]
    pub version: bool,
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

pub fn show_header() {
    println!(
        "\nBundleRepo Version {}, \u{00A9} 2024 {}",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS")
    );
    println!("\n{}\n", env!("CARGO_PKG_DESCRIPTION"))
}
