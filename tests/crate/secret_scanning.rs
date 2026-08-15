use super::*;
use std::sync::OnceLock;

fn scanner() -> &'static SecretScanner {
    static SCANNER: OnceLock<SecretScanner> = OnceLock::new();
    SCANNER.get_or_init(|| SecretScanner::from_bundled().unwrap())
}

fn finding(
    start: usize,
    end: usize,
    secret_type: Option<&str>,
    rule_id: &str,
) -> SafeFinding {
    SafeFinding {
        start,
        end,
        secret_type: secret_type.map(str::to_string),
        rule_id: rule_id.to_string(),
    }
}

fn github_pat() -> String {
    synthetic_github_pat()
}

fn gitlab_pat() -> String {
    ["glpat-", "Ab1Cd2Ef3Gh4Ij5Kl6Mn"].concat()
}

fn aws_access_key() -> String {
    ["AKIA", "A2B3C4D5E6F7G2H3"].concat()
}

#[test]
fn test_exact_span_replacement_preserves_surrounding_syntax() {
    let text = "token = \"sensitive-value\";";
    let start = text.find("sensitive-value").unwrap();
    let redacted = redact_findings(
        text,
        vec![finding(
            start,
            start + "sensitive-value".len(),
            Some("API Token"),
            "api-token",
        )],
    )
    .unwrap();

    assert_eq!(redacted, "token = \"[Secret removed: API Token]\";");
}

#[test]
fn test_multiple_duplicate_overlapping_and_adjacent_spans_are_deterministic() {
    let text = "abcdefghij";
    let findings = vec![
        finding(1, 4, Some("Token"), "token-b"),
        finding(1, 4, Some("Token"), "token-a"),
        finding(3, 6, Some("Token"), "token-c"),
        finding(6, 8, Some("Key"), "key"),
    ];
    let redacted = redact_findings(text, findings).unwrap();

    assert_eq!(redacted, "a[Secret removed: Token][Secret removed: Key]ij");
}

#[test]
fn test_conflicting_overlap_uses_generic_marker() {
    let redacted = redact_findings(
        "abcdefgh",
        vec![
            finding(1, 5, Some("Token"), "token"),
            finding(3, 7, Some("Key"), "key"),
        ],
    )
    .unwrap();

    assert_eq!(redacted, "a[Secret removed]h");
}

#[test]
fn test_invalid_spans_fail_closed() {
    for (start, end) in [(0, 0), (3, 2), (0, 9)] {
        let error = redact_findings(
            "value",
            vec![finding(start, end, None, "invalid")],
        )
        .unwrap_err();
        assert!(matches!(error, SecretScanError::InvalidSpan));
        assert_eq!(
            error.to_string(),
            "secret scanner returned an invalid span"
        );
    }
}

#[test]
fn test_mid_codepoint_span_expands_to_utf8_boundaries() {
    let redacted =
        redact_findings("ésecret", vec![finding(1, 3, None, "boundary")])
            .unwrap();

    assert_eq!(redacted, "[Secret removed]ecret");
}

#[test]
fn test_multiline_span_preserves_line_terminators() {
    let text = "prefix alpha\nbeta\r\ngamma\romega suffix";
    let start = text.find("alpha").unwrap();
    let end = text.find(" suffix").unwrap();
    let redacted =
        redact_findings(text, vec![finding(start, end, None, "multiline")])
            .unwrap();

    assert_eq!(redacted, "prefix [Secret removed]\n\r\n\r suffix");
}

#[test]
fn test_secret_type_normalization_and_fallback_are_bounded() {
    assert_eq!(
        normalize_secret_type(
            "Uncovered a GitHub Personal Access Token, potentially exposing repositories",
            "github-pat",
        ),
        Some("GitHub Personal Access Token".to_string())
    );
    assert_eq!(
        normalize_secret_type(
            "Detected AWS Access Key (kingfisher, confidence: high)",
            "kingfisher.aws.1",
        ),
        Some("AWS Access Key".to_string())
    );
    assert_eq!(
        normalize_secret_type("", "generic-api-key"),
        Some("Generic API Key".to_string())
    );
    assert_eq!(normalize_secret_type("<>unsafe", "..."), None);
    assert_eq!(normalize_secret_type(&"x".repeat(81), "..."), None);
}

#[test]
fn test_bundled_scanner_redacts_three_stable_rule_families() {
    let secrets = [github_pat(), gitlab_pat(), aws_access_key()];
    let text = format!(
        "github = \"{}\"\ngitlab = \"{}\"\naws = \"{}\"\n",
        secrets[0], secrets[1], secrets[2]
    );
    let ids = scanner().rule_ids(&text);
    let redacted = scanner().redact_text(&text).unwrap();

    for expected in ["github-pat", "gitlab-pat", "aws-access-token"] {
        assert!(ids.iter().any(|rule_id| rule_id == expected));
    }
    for secret in secrets {
        assert!(!redacted.text.contains(&secret));
    }
    assert!(redacted.findings >= 3);
}

#[test]
fn test_allow_markers_cannot_suppress_a_secret() {
    for marker in ["gitleaks:allow", "secrets-scanner:allow"] {
        let secret = github_pat();
        let text = format!("token = {secret} # {marker}");
        let redacted = scanner().redact_text(&text).unwrap();
        assert!(!redacted.text.contains(&secret));
        assert!(redacted.findings > 0);
    }
}

#[test]
fn test_large_content_is_scanned_through_the_end() {
    let secret = github_pat();
    let mut text = "x".repeat(2 * 1024 * 1024 + 1);
    text.push('\n');
    text.push_str(&secret);

    let redacted = scanner().redact_text(&text).unwrap();

    assert!(!redacted.text.contains(&secret));
    assert!(redacted.text.ends_with(']'));
}

#[test]
fn test_repeated_scans_produce_identical_output() {
    let secret = github_pat();
    let text = format!("token = {secret}");
    let first = scanner().redact_text(&text).unwrap();
    let second = scanner().redact_text(&text).unwrap();

    assert_eq!(first.text, second.text);
    assert_eq!(first.findings, second.findings);
}
