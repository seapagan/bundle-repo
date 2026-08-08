use clap::{ArgAction, Parser};

use crate::structs::{DEFAULT_MODEL, DEFAULT_OUTPUT_FILE};
use crate::tokenizer::MODEL_VALUES;

fn parse_gzip_level(value: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(level @ 1..=9) => Ok(level),
        _ => Err("gzip level must be an integer from 1 to 9".to_string()),
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "bundlerepo",
    author = env!("CARGO_PKG_AUTHORS"),
    about = env!("CARGO_PKG_DESCRIPTION"),
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
        help = &format!("Filename to save the bundle as. (Defaults to '{DEFAULT_OUTPUT_FILE}')")
    )]
    pub output_file: Option<String>,

    #[arg(
        long = "stdout",
        short = 's',
        action = clap::ArgAction::SetTrue,
        help = "Output the XML directly to stdout without creating a file."
    )]
    pub stdout: bool,

    #[arg(
        long = "gzip",
        short = 'z',
        value_name = "LEVEL",
        num_args = 0..=1,
        require_equals = true,
        value_parser = parse_gzip_level,
        help = "Compress output with gzip at an optional level from 1 to 9 (use =LEVEL)"
    )]
    pub gzip: Option<Option<u32>>,

    #[arg(
        long = "no-gzip",
        action = ArgAction::SetTrue,
        conflicts_with = "gzip",
        help = "Disable gzip output, overriding configuration"
    )]
    pub no_gzip: bool,

    #[arg(
        long = "model",
        short = 'm',
        help = &format!(
            "Model to use for tokenization count. (Defaults to '{DEFAULT_MODEL}')"
        ),
        ignore_case = true,
        value_parser = MODEL_VALUES
    )]
    pub model: Option<String>,

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

    #[arg(
        long = "extend-exclude",
        short = 'e',
        value_name = "PATTERN",
        help = "Add file/directory pattern to exclude, can be specified multiple times.",
        action = ArgAction::Append
    )]
    pub extend_exclude: Option<Vec<String>>,

    #[arg(
        long = "exclude",
        short = 'x',
        value_name = "PATTERN",
        help = "Replace the existing exclude patterns with the specified pattern(s). Can be specified multiple times.",
        action = ArgAction::Append
    )]
    pub exclude: Option<Vec<String>>,

    #[arg(
        long = "utf8",
        short = 'u',
        action = ArgAction::SetTrue,
        help = "Detect and convert legacy text encodings to UTF-8",
    )]
    pub utf8: bool,

    #[arg(
        long = "no-utf8",
        short = 'U',
        action = ArgAction::SetTrue,
        help = "Disable legacy text conversion to UTF-8",
        conflicts_with = "utf8",
    )]
    pub no_utf8: bool,
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
        "BundleRepo v{}\n\
        \n{}\n\
        \nReleased under the MIT license by {}\n",
        version, description, authors
    )
}

pub fn show_header() {
    println!(
        "\nBundleRepo Version {}, \u{00A9} 2024-2026 {}",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS")
    );
    println!("\n{}\n", env!("CARGO_PKG_DESCRIPTION"))
}

#[cfg(test)]
#[path = "../tests/crate/cli.rs"]
mod tests;
