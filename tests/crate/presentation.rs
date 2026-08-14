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
        presentation.error("unprefixed diagnostic"),
        "unprefixed diagnostic"
    );
}
