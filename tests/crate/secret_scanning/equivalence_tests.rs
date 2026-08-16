use super::*;

#[test]
fn test_cross_partition_overlaps_and_adjacent_findings_match_reference() {
    let rules = r#"
[[rules]]
id = "token"
description = "Token"
regex = '''(bcde)'''
secretGroup = 1

[[rules]]
id = "key"
description = "Key"
regex = '''(defg)'''
secretGroup = 1

[[rules]]
id = "suffix"
description = "Suffix"
regex = '''(hi)'''
secretGroup = 1
"#;
    let scanner = SecretScanner::from_rules_for_workers(rules, 3).unwrap();
    let reference = SecretScanner::reference_scanner_from_rules(rules)
        .unwrap()
        .scan_content_detailed("repository-content", "abcdefghij");
    let partitioned = scanner
        .scan_findings("repository-content", "abcdefghij")
        .unwrap();

    assert_findings_equivalent(&reference.findings, &partitioned);
    assert_eq!(
        scanner.redact_text("abcdefghij").unwrap().text,
        "a[Secret removed][Secret removed: Suffix]j"
    );
}

#[test]
fn test_cross_partition_duplicate_default_group_findings_match_reference() {
    let rules = r#"
[[rules]]
id = "duplicate-a"
description = "Duplicate"
regex = '''DUPLICATE'''

[[rules]]
id = "duplicate-b"
description = "Duplicate"
regex = '''DUPLICATE'''
"#;
    assert_equivalent(rules, "repository-content", "DUPLICATE", 2);
    let redaction = SecretScanner::from_rules_for_workers(rules, 2)
        .unwrap()
        .redact_text("DUPLICATE")
        .unwrap();

    assert_eq!(redaction.findings, 2);
    assert_eq!(redaction.text, "[Secret removed: Duplicate]");
}

#[test]
fn test_path_filters_match_and_reject_identically() {
    let rules = r#"
[[rules]]
id = "path-filtered"
description = "Path filtered"
regex = '''token=([A-Z]{8})'''
keywords = ["token="]
path = '''^allowed/'''
secretGroup = 1

[[rules]]
id = "control"
description = "Control"
regex = '''control=([A-Z]{8})'''
keywords = ["control="]
secretGroup = 1
"#;
    let text = "token=ABCDEFGH control=HGFEDCBA";

    assert_equivalent(rules, "allowed/file.txt", text, 2);
    assert_equivalent(rules, "blocked/file.txt", text, 2);
    let scanner = SecretScanner::from_rules_for_workers(rules, 2).unwrap();
    assert_eq!(
        scanner
            .scan_findings("allowed/file.txt", text)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        scanner
            .scan_findings("blocked/file.txt", text)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn test_entropy_accept_reject_and_unicode_offsets_match_reference() {
    let rules = r#"
[[rules]]
id = "entropy"
description = "Entropy"
regex = '''entropy=([A-Za-z0-9]{16})'''
keywords = ["entropy="]
entropy = 3.0
secretGroup = 1

[[rules]]
id = "unicode"
description = "Unicode"
regex = '''unicode=([A-Z]{8})'''
keywords = ["unicode="]
secretGroup = 1
"#;
    let text = "é entropy=AAAAAAAAAAAAAAAA\r\nentropy=aB3dE5gH7jK9mN2p unicode=ABCDEFGH";

    assert_equivalent(rules, "repository-content", text, 2);
    let scanner = SecretScanner::from_rules_for_workers(rules, 2).unwrap();
    let findings = scanner.scan_findings("repository-content", text).unwrap();
    assert_eq!(
        findings
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect::<Vec<_>>(),
        ["entropy", "unicode"]
    );
}

#[test]
fn test_safe_content_and_disabled_inline_markers_are_equivalent() {
    let rules = r#"
[[rules]]
id = "marker"
description = "Marker"
regex = '''token=([A-Z]{8})'''
keywords = ["token="]
secretGroup = 1

[[rules]]
id = "other"
description = "Other"
regex = '''other=([A-Z]{8})'''
keywords = ["other="]
secretGroup = 1
"#;

    assert_equivalent(rules, "repository-content", "safe content", 2);
    assert_equivalent(
        rules,
        "repository-content",
        "token=ABCDEFGH # gitleaks:allow",
        2,
    );
}

fn assert_equivalent(rules: &str, path: &str, text: &str, workers: usize) {
    let reference = SecretScanner::reference_scanner_from_rules(rules)
        .unwrap()
        .scan_content_detailed(path, text);
    let partitioned = SecretScanner::from_rules_for_workers(rules, workers)
        .unwrap()
        .scan_findings(path, text)
        .unwrap();

    assert!(!reference.findings_truncated);
    assert_findings_equivalent(&reference.findings, &partitioned);
}
