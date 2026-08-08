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
    assert!(!args.stdout);
}

#[test]
fn test_full_github_url() {
    let args = Flags::parse_from(["program", "https://github.com/user/repo"]);
    assert_eq!(args.repo, Some("https://github.com/user/repo".to_string()));
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
    let args =
        Flags::parse_from(["program", "user/repo", "--file", "output.xml"]);
    assert_eq!(args.output_file, Some("output.xml".to_string()));
}

#[test]
fn test_stdout_flag() {
    let args = Flags::parse_from(["program", "user/repo", "--stdout"]);
    assert!(args.stdout);
}

#[test]
fn test_gzip_flag_forms() {
    for input in [
        vec!["program", "-z"],
        vec!["program", "--gzip"],
        vec!["program", "user/repo", "-z"],
    ] {
        let args = Flags::parse_from(input);
        assert_eq!(args.gzip, Some(None));
    }

    for input in [vec!["program", "-z=9"], vec!["program", "--gzip=9"]] {
        let args = Flags::parse_from(input);
        assert_eq!(args.gzip, Some(Some(9)));
    }
}

#[test]
fn test_gzip_boundary_levels() {
    let level_one = Flags::parse_from(["program", "-z=1"]);
    let level_nine = Flags::parse_from(["program", "--gzip=9"]);
    assert_eq!(level_one.gzip, Some(Some(1)));
    assert_eq!(level_nine.gzip, Some(Some(9)));
}

#[test]
fn test_invalid_gzip_levels() {
    for level in ["0", "10", "fast"] {
        let argument = format!("--gzip={level}");
        let result = Flags::try_parse_from(["program", &argument]);
        let error = result.unwrap_err().to_string();
        assert!(error.contains("gzip level must be an integer from 1 to 9"));
    }
}

#[test]
fn test_gzip_conflicts_with_no_gzip() {
    let result = Flags::try_parse_from(["program", "-z", "--no-gzip"]);
    assert!(result.is_err());
}

#[test]
fn test_bare_gzip_does_not_consume_following_arguments() {
    let positional = Flags::parse_from(["program", "-z", "user/repo"]);
    assert_eq!(positional.gzip, Some(None));
    assert_eq!(positional.repo.as_deref(), Some("user/repo"));

    let clustered = Flags::parse_from(["program", "-zs"]);
    assert_eq!(clustered.gzip, Some(None));
    assert!(clustered.stdout);
}

#[test]
fn test_gzip_level_requires_equals() {
    assert!(Flags::try_parse_from(["program", "-z9"]).is_err());

    for input in [["program", "-z", "9"], ["program", "--gzip", "9"]] {
        let args = Flags::parse_from(input);
        assert_eq!(args.gzip, Some(None));
        assert_eq!(args.repo.as_deref(), Some("9"));
    }
}

#[test]
fn test_model_selection() {
    let args = Flags::parse_from(["program", "user/repo", "--model", "gpt4"]);
    assert_eq!(args.model, Some("gpt4".to_string()));
}

#[test]
fn test_all_supported_model_values() {
    for model in MODEL_VALUES {
        let args = Flags::parse_from(["program", "--model", model]);
        assert_eq!(args.model.as_deref(), Some(model));
    }
}

#[test]
fn test_model_values_are_case_insensitive() {
    for model in ["GPT5", "gPt4O", "DeepSeek-V4", "GLM5.2", "DeepSeek"] {
        let args = Flags::parse_from(["program", "--model", model]);
        assert_eq!(args.model.as_deref(), Some(model));
    }
}

#[test]
fn test_legacy_deepseek_alias_selects_r1() {
    use crate::tokenizer::Model;

    for alias in ["deepseek", "DeepSeek"] {
        let args = Flags::parse_from(["program", "--model", alias]);
        assert_eq!(args.model.unwrap().parse(), Ok(Model::DeepSeekR1));
    }
}

#[test]
fn test_removed_model_values_are_rejected() {
    for model in ["gpt2", "GPT2", "gpt3", "GPT3"] {
        assert!(Flags::try_parse_from(["program", "--model", model]).is_err());
    }
}

#[test]
fn test_model_help_lists_modern_values_and_default() {
    use clap::CommandFactory;

    let help = Flags::command().render_long_help().to_string();
    for model in MODEL_VALUES {
        assert!(help.contains(model), "help omitted {model}");
    }
    assert!(help.contains("Defaults to 'gpt5'"));
    assert!(!help.contains("gpt2"));
    assert!(!help.contains("gpt3,"));
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
    assert!(err.contains("possible values: gpt5, gpt4o, gpt4, gpt3.5"));
    assert!(err.contains("deepseek-v4, deepseek-v3, deepseek-r1"));
    assert!(err.contains("glm5.2, deepseek"));
    assert!(!err.contains("gpt2"));
    assert!(!err.contains("gpt3,"));
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
