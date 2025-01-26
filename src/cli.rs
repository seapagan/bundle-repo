use clap::{ArgAction, Parser};

use crate::structs::Params;

const VALID_MODELS: [&str; 6] =
    ["gpt4o", "gpt4", "gpt3.5", "gpt3", "gpt2", "deepseek"];

#[derive(Parser, Debug)]
#[command(
    name = "Repopack Clone Tool",
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
        help = &format!("Filename to save the bundle as. (Defaults to '{}')",
            Params::default().output_file.unwrap_or_default())
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
        long = "model",
        short = 'm',
        help = &format!(
            "Model to use for tokenization count. "
        ),
        value_parser = VALID_MODELS
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
        help = "Force UTF-8 encoding for all text files",
    )]
    pub utf8: bool,

    #[arg(
        long = "no-utf8",
        short = 'U',
        action = ArgAction::SetTrue,
        help = "Disable UTF-8 encoding for text files",
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
        "bundle_repo v{}\n\
        \n{}\n\
        \nReleased under the MIT license by {}\n",
        version, description, authors
    )
}

pub fn show_header() {
    println!(
        "\nBundleRepo Version {}, \u{00A9} 2024-2025 {}",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS")
    );
    println!("\n{}\n", env!("CARGO_PKG_DESCRIPTION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info() {
        let version_str = version_info();
        assert!(version_str.contains(env!("CARGO_PKG_VERSION")));
        assert!(version_str.contains(env!("CARGO_PKG_AUTHORS")));
        assert!(version_str.contains(env!("CARGO_PKG_DESCRIPTION")));
    }

    #[test]
    fn test_basic_repo_arg() {
        let args = Flags::parse_from(["program", "user/repo"]);
        assert_eq!(args.repo, Some("user/repo".to_string()));
        assert_eq!(args.branch, None);
        assert_eq!(args.stdout, false);
    }

    #[test]
    fn test_full_github_url() {
        let args =
            Flags::parse_from(["program", "https://github.com/user/repo"]);
        assert_eq!(
            args.repo,
            Some("https://github.com/user/repo".to_string())
        );
    }

    #[test]
    fn test_branch_option() {
        let args =
            Flags::parse_from(["program", "user/repo", "--branch", "develop"]);
        assert_eq!(args.repo, Some("user/repo".to_string()));
        assert_eq!(args.branch, Some("develop".to_string()));
    }

    #[test]
    fn test_output_file() {
        let args = Flags::parse_from([
            "program",
            "user/repo",
            "--file",
            "output.xml",
        ]);
        assert_eq!(args.output_file, Some("output.xml".to_string()));
    }

    #[test]
    fn test_stdout_flag() {
        let args = Flags::parse_from(["program", "user/repo", "--stdout"]);
        assert!(args.stdout);
    }

    #[test]
    fn test_model_selection() {
        let args =
            Flags::parse_from(["program", "user/repo", "--model", "gpt4"]);
        assert_eq!(args.model, Some("gpt4".to_string()));
    }

    #[test]
    fn test_clipboard_flag() {
        let args = Flags::parse_from(["program", "user/repo", "--clipboard"]);
        assert!(args.clipboard);
    }

    #[test]
    fn test_line_numbers_flag() {
        let args = Flags::parse_from(["program", "user/repo", "--lnumbers"]);
        assert!(args.lnumbers);
    }

    #[test]
    fn test_token_option() {
        let args =
            Flags::parse_from(["program", "user/repo", "--token", "abc123"]);
        assert_eq!(args.token, Some("abc123".to_string()));
    }

    #[test]
    fn test_version_flag() {
        let args = Flags::parse_from(["program", "--version"]);
        assert!(args.version);
    }

    #[test]
    fn test_extend_exclude_patterns() {
        let args = Flags::parse_from([
            "program",
            "user/repo",
            "--extend-exclude",
            "*.log",
            "--extend-exclude",
            "target/",
        ]);
        assert_eq!(
            args.extend_exclude,
            Some(vec!["*.log".to_string(), "target/".to_string()])
        );
    }

    #[test]
    fn test_multiple_flags() {
        let args = Flags::parse_from([
            "program",
            "user/repo",
            "--branch",
            "main",
            "--stdout",
            "--clipboard",
            "--model",
            "gpt4",
        ]);
        assert_eq!(args.repo, Some("user/repo".to_string()));
        assert_eq!(args.branch, Some("main".to_string()));
        assert!(args.stdout);
        assert!(args.clipboard);
        assert_eq!(args.model, Some("gpt4".to_string()));
    }

    #[test]
    fn test_no_repo_arg() {
        let args = Flags::parse_from(["program"]);
        assert_eq!(args.repo, None);
    }

    #[test]
    fn test_invalid_model() {
        let result = Flags::try_parse_from([
            "program",
            "user/repo",
            "--model",
            "invalid_model",
        ]);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid value 'invalid_model'"));
        assert!(err.contains(
            "possible values: gpt4o, gpt4, gpt3.5, gpt3, gpt2, deepseek"
        ));
    }

    #[test]
    fn test_short_flags() {
        let args = Flags::parse_from([
            "program",
            "user/repo",
            "-b",
            "main",
            "-s",
            "-c",
            "-m",
            "gpt4",
        ]);
        assert_eq!(args.branch, Some("main".to_string()));
        assert!(args.stdout);
        assert!(args.clipboard);
        assert_eq!(args.model, Some("gpt4".to_string()));
    }

    #[test]
    fn test_show_header() {
        // The header should contain these values
        let version = env!("CARGO_PKG_VERSION");
        let authors = env!("CARGO_PKG_AUTHORS");
        let desc = env!("CARGO_PKG_DESCRIPTION");

        // Verify the values exist and aren't empty
        assert!(!version.is_empty());
        assert!(!authors.is_empty());
        assert!(!desc.is_empty());

        // We can't easily test the actual stdout output, but we can verify
        // the function doesn't panic
        show_header();
    }

    #[test]
    fn test_utf8_flag_values() {
        fn assert_bool<T: Into<bool>>(_: &T) {}

        // Test --utf8 flag (sets to true)
        let args = Flags::parse_from(["program", "--utf8"]);
        assert!(args.utf8);
        assert!(!args.no_utf8);
        assert_bool(&args.utf8);

        // Test --no-utf8 flag (sets to false)
        let args = Flags::parse_from(["program", "--no-utf8"]);
        assert!(!args.utf8);
        assert!(args.no_utf8);
        assert_bool(&args.utf8);

        // Test -U short flag
        let args = Flags::parse_from(["program", "-U"]);
        assert!(!args.utf8);
        assert!(args.no_utf8);
        assert_bool(&args.utf8);

        // Test default value (should be false)
        let args = Flags::parse_from(["program"]);
        assert!(!args.utf8);
        assert!(!args.no_utf8);
        assert_bool(&args.utf8);

        // Test short flag
        let args = Flags::parse_from(["program", "-u"]);
        assert!(args.utf8);
        assert!(!args.no_utf8);
        assert_bool(&args.utf8);

        // Test that --utf8 and --no-utf8 cannot be used together
        let result = Flags::try_parse_from(["program", "--utf8", "--no-utf8"]);
        assert!(result.is_err());

        // Test that -u and -U cannot be used together
        let result = Flags::try_parse_from(["program", "-u", "-U"]);
        assert!(result.is_err());
    }
}
