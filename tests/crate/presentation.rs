use super::*;

#[test]
fn test_plain_phase_preserves_exact_text() {
    let presentation = Presentation::plain();

    assert_eq!(
        presentation.phase("Reading files and generating XML"),
        "-> Reading files and generating XML"
    );
}

#[test]
fn test_plain_conversion_preserves_exact_text() {
    let presentation = Presentation::plain();

    assert_eq!(
        presentation.conversion("legacy.txt", "windows-1252"),
        "-> Converted 'legacy.txt' from windows-1252 to UTF-8"
    );
    assert_eq!(
        presentation.conversion_warning("legacy.txt", "windows-1252"),
        "warning: 'legacy.txt' decoded as windows-1252 with replacement characters; information was lost"
    );
}

#[test]
fn test_plain_diagnostic_prefixes_preserve_exact_text() {
    let presentation = Presentation::plain();

    assert_eq!(
        presentation.warning("Warning: Invalid regex pattern"),
        "Warning: Invalid regex pattern"
    );
    assert_eq!(
        presentation.error("Error: unable to write output"),
        "Error: unable to write output"
    );
    assert_eq!(
        presentation.error("X  Failed to write XML"),
        "X  Failed to write XML"
    );
    assert_eq!(
        presentation.error("Xylophone diagnostic"),
        "Xylophone diagnostic"
    );
    assert_eq!(
        presentation.error("unprefixed diagnostic"),
        "unprefixed diagnostic"
    );
}

#[test]
fn test_error_prefix_matching_uses_exact_markers() {
    let presentation = Presentation::ansi16();

    assert_eq!(
        presentation.error("Xylophone diagnostic"),
        "Xylophone diagnostic"
    );
    if std::env::var_os("NO_COLOR").is_some() {
        return;
    }
    assert_eq!(
        presentation.error("X  Failed to write XML"),
        "\x1b[1;31mX  \x1b[0mFailed to write XML"
    );
}

#[test]
fn test_plain_header_success_and_summary_preserve_exact_text() {
    let presentation = Presentation::plain();
    let table = concat!(
        "     Total Files processed:  3    \n",
        " Total output size (bytes):  2048 \n",
        "      Token count (GPT-4o):  512  "
    );
    let summary_values =
        ["3".to_string(), "2048".to_string(), "512".to_string()];

    assert_eq!(
        presentation.header("1.2.3", "A. Person", "Description"),
        "\nBundleRepo Version 1.2.3, \u{00A9} 2024-2026 A. Person\n\nDescription\n\n"
    );
    assert_eq!(
        presentation.success(" copied XML to clipboard"),
        "-> Successfully copied XML to clipboard"
    );
    assert_eq!(
        presentation.success_with_accent(" wrote XML to '", "output.xml", "'"),
        "-> Successfully wrote XML to 'output.xml'"
    );
    assert_eq!(
        presentation.summary(table, &summary_values),
        format!("\nSummary:\n{table}\n\n")
    );
}

#[test]
fn test_summary_accents_only_values_and_strips_to_plain_output() {
    let table = concat!(
        "     Total Files processed:  3    \n",
        " Total output size (bytes):  2048 \n",
        "      Token count (GPT-4o):  512  "
    );
    let values = ["3".to_string(), "2048".to_string(), "512".to_string()];
    let plain = Presentation::plain().summary(table, &values);

    let coloured = Presentation::ansi16().summary(table, &values);

    assert_eq!(
        Presentation::styled("3", SemanticStyle::SummaryValue),
        "3".cyan().bold()
    );
    if std::env::var_os("NO_COLOR").is_some() {
        assert_eq!(coloured, plain);
        return;
    }
    assert_eq!(
        coloured,
        concat!(
            "\n\x1b[1;36mSummary:\x1b[0m\n",
            "     Total Files processed:  \x1b[1;36m3\x1b[0m    \n",
            " Total output size (bytes):  \x1b[1;36m2048\x1b[0m \n",
            "      Token count (GPT-4o):  \x1b[1;36m512\x1b[0m  \n\n"
        )
    );
    assert_eq!(
        coloured.replace("\x1b[1;36m", "").replace("\x1b[0m", ""),
        plain
    );
}
