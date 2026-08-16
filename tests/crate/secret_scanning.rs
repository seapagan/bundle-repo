use super::*;
use std::sync::OnceLock;

#[path = "secret_scanning/equivalence_tests.rs"]
mod equivalence_tests;
#[path = "secret_scanning/execution_tests.rs"]
mod execution_tests;
#[path = "secret_scanning/hardening_tests.rs"]
mod hardening_tests;
#[path = "secret_scanning/partition_tests.rs"]
mod partition_tests;
#[cfg(not(debug_assertions))]
#[path = "secret_scanning/performance_tests.rs"]
mod performance_tests;

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

fn assert_findings_equivalent(reference: &[Finding], partitioned: &[Finding]) {
    assert_eq!(reference.len(), partitioned.len());
    for (expected, actual) in reference.iter().zip(partitioned) {
        assert_eq!(expected.file, actual.file);
        assert_eq!(expected.line, actual.line);
        assert_eq!(expected.col, actual.col);
        assert_eq!(expected.end_line, actual.end_line);
        assert_eq!(expected.end_col, actual.end_col);
        assert_eq!(expected.col_utf16, actual.col_utf16);
        assert_eq!(expected.end_col_utf16, actual.end_col_utf16);
        assert_eq!(expected.rule_id, actual.rule_id);
        assert_eq!(expected.rule_description, actual.rule_description);
        assert!(expected.matched == actual.matched, "matched value mismatch");
        assert_eq!(expected.entropy.to_bits(), actual.entropy.to_bits());
        assert_eq!(expected.start_offset, actual.start_offset);
        assert_eq!(expected.end_offset, actual.end_offset);
        assert_eq!(expected.secret_start_offset, actual.secret_start_offset);
        assert_eq!(expected.secret_end_offset, actual.secret_end_offset);
        assert!(
            expected.fingerprint == actual.fingerprint,
            "fingerprint mismatch"
        );
        assert_eq!(expected.commit, actual.commit);
        assert!(
            expected.context_lines == actual.context_lines,
            "context mismatch"
        );
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
        normalize_secret_type(
            "Identified a potential DeepSeek API Key, which could lead to unauthorized access",
            "deepseek-api-key",
        ),
        Some("DeepSeek API Key".to_string())
    );
    assert_eq!(
        normalize_secret_type(
            "Found a Confluent Secret Key, potentially risking unauthorized operations",
            "confluent-secret-key",
        ),
        Some("Confluent Secret Key".to_string())
    );
    assert_eq!(
        normalize_secret_type(
            "Found an Etsy Access Token, potentially compromising shop management",
            "etsy-access-token",
        ),
        Some("Etsy Access Token".to_string())
    );
    assert_eq!(
        normalize_secret_type(
            "Found a pattern resembling a Codecov Access Token, posing a risk",
            "codecov-access-token",
        ),
        Some("Codecov Access Token".to_string())
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
    let redacted = scanner().redact_text("safe/example.txt", &text).unwrap();

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
        let redacted =
            scanner().redact_text("safe/example.txt", &text).unwrap();
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

    let redacted = scanner().redact_text("safe/example.txt", &text).unwrap();

    assert!(!redacted.text.contains(&secret));
    assert!(redacted.text.ends_with(']'));
}

#[test]
fn test_repository_paths_omit_files_and_deduplicate_subtrees() {
    let secret = github_pat();
    let root_file = format!("{secret}.env");
    let nested_file = format!("safe/{secret}.txt");
    let subtree_file = format!("fixtures/{secret}/first.txt");
    let subtree_descendant = format!("fixtures/{secret}/nested/second.txt");
    let scan = scanner()
        .scan_repository_paths(vec![
            subtree_descendant,
            "safe/included.txt".to_string(),
            nested_file,
            subtree_file,
            root_file,
        ])
        .unwrap();

    assert_eq!(scan.included, ["safe/included.txt"]);
    assert_eq!(scan.skipped.len(), 3);
    assert_eq!(
        scan.skipped
            .iter()
            .map(|item| (item.kind, item.safe_path.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                SkippedItemKind::File,
                "[Secret removed: GitHub Personal Access Token].env",
            ),
            (
                SkippedItemKind::Subtree,
                "fixtures/[Secret removed: GitHub Personal Access Token]",
            ),
            (
                SkippedItemKind::File,
                "safe/[Secret removed: GitHub Personal Access Token].txt",
            ),
        ]
    );
    assert!(scan.findings >= 3);
    assert!(
        scan.skipped
            .iter()
            .all(|item| !item.safe_path.contains(&secret))
    );
}

#[test]
fn test_component_scan_stays_synthetic_and_ignores_path_allowlists() {
    let rules = r#"
[allowlist]
paths = ['repository-path-component']

[[rules]]
id = 'component-secret'
regex = 'component-([A-Z]{8})'
keywords = ['component-']
secretGroup = 1
"#;
    let scanner = SecretScanner::from_rules_for_workers(rules, 1).unwrap();
    let scan = scanner
        .scan_repository_paths(vec!["safe/component-SECRETAA.txt".to_string()])
        .unwrap();

    assert!(scan.included.is_empty());
    assert_eq!(scan.skipped.len(), 1);
    assert!(!scan.skipped[0].safe_path.contains("SECRETAA"));
}

#[test]
fn test_non_path_rule_zero_span_still_fails_closed() {
    let rules = r#"
[[rules]]
id = 'ordinary'
regex = '(SECRETAA)'
keywords = ['SECRETAA']
secretGroup = 1
"#;
    let scanner = SecretScanner::from_rules_for_workers(rules, 1).unwrap();
    let mut finding = scanner
        .scan_findings("safe.txt", "SECRETAA")
        .unwrap()
        .pop()
        .unwrap();
    finding.secret_start_offset = 0;
    finding.secret_end_offset = 0;
    let result = ScanResult {
        findings: vec![finding],
        findings_truncated: false,
    };

    assert!(matches!(
        scanner.redact_result_for_tests("SECRETAA", result),
        Err(SecretScanError::InvalidSpan)
    ));
}

#[test]
fn test_repository_path_with_conflicting_types_omits_type() {
    let github = github_pat();
    let gitlab = gitlab_pat();
    let path = format!("fixtures/{github}-{gitlab}.txt");

    let scan = scanner().scan_repository_paths(vec![path]).unwrap();

    assert!(scan.included.is_empty());
    assert_eq!(scan.skipped.len(), 1);
    let item = &scan.skipped[0];
    assert_eq!(item.kind, SkippedItemKind::File);
    assert!(!item.safe_path.contains(&github));
    assert!(!item.safe_path.contains(&gitlab));
    assert!(matches!(
        item.reason,
        SkipReason::SecretInPath { secret_type: None }
    ));
}

#[test]
fn test_repository_path_diagnostics_are_deterministic() {
    let secret = github_pat();
    let paths = vec![
        format!("z/{secret}.txt"),
        "included.txt".to_string(),
        format!("a/{secret}/child.txt"),
    ];

    let first = scanner().scan_repository_paths(paths.clone()).unwrap();
    let second = scanner().scan_repository_paths(paths).unwrap();

    assert_eq!(first.included, second.included);
    assert_eq!(first.skipped, second.skipped);
}

#[test]
fn test_repeated_scans_produce_identical_output() {
    let secret = github_pat();
    let text = format!("token = {secret}");
    let first = scanner().redact_text("safe/example.txt", &text).unwrap();
    let second = scanner().redact_text("safe/example.txt", &text).unwrap();

    assert_eq!(first.text, second.text);
    assert_eq!(first.findings, second.findings);
}
