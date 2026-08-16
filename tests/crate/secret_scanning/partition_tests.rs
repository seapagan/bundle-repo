use super::*;
use crate::secret_scanning::execution::merge_test_results;
use crate::secret_scanning::partitions::{
    RulePhase, build_partitioned_scanners, hardened_ruleset_for_tests,
    prove_compiled_partition_for_tests,
};
use secrets_scanner::ScanResult;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;

const SYNTHETIC_RULES: &str = r#"
title = "partition tests"
minVersion = "8.0.0"
futureScalar = 7
futureArray = ["alpha", "beta"]

[futureTable]
enabled = true

[[rules]]
id = "path-only"
description = "Path only"
path = '^repository-content$'

[[rules]]
id = "keyworded-a"
description = "Keyworded A"
regex = '''token_a\s*=\s*([A-Z0-9]{8})'''
keywords = ["token_a"]
secretGroup = 1

[[rules]]
id = "unkeyworded"
description = "Unkeyworded"
regex = '''UNKEY-([A-Z]{8})'''
secretGroup = 1

[[rules]]
id = "keyworded-b"
description = "Keyworded B"
regex = '''token_b\s*=\s*([A-Z0-9]{8})'''
keywords = ["token_b"]
secretGroup = 1
"#;

const HARDENING_RULES: &str = r#"
[allowlist]
paths = ['global-only']

[[allowlists]]
id = "targeted-or"
paths = ['global-or']
regexes = ['KEEP']
targetRules = ["detector"]

[[allowlists]]
id = "targeted-and"
condition = "AND"
paths = ['global-and']
stopwords = ["keep"]
targetRules = ["detector"]

[[rules]]
id = "detector"
description = "Detector"
path = '\.cfg$'
regex = 'token=([A-Z]{8})'
keywords = ['token=']
secretGroup = 1

[[rules.allowlists]]
paths = ['local-only']

[[rules.allowlists]]
paths = ['local-or']
regexes = ['LOCALKEEP']

[[rules.allowlists]]
condition = "and"
paths = ['local-and']
regexes = ['ANDKEEP']
"#;

#[test]
fn test_worker_policy_uses_one_and_caps_at_twenty_eight() {
    assert_eq!(MAX_SCAN_WORKERS, 28);
    assert_eq!(resolved_worker_count(None), 1);
    for available in [1, 2, 7, 16, 27, 28, 29, 64] {
        let expected = available.min(MAX_SCAN_WORKERS);
        assert_eq!(
            resolved_worker_count(NonZeroUsize::new(available)),
            expected
        );
    }
}

#[test]
fn test_hardening_preserves_detectors_and_transforms_all_allowlist_forms() {
    let original = toml::from_str::<toml::Table>(HARDENING_RULES).unwrap();
    let hardened = hardened_ruleset_for_tests(HARDENING_RULES).unwrap();

    let original_rule =
        original["rules"].as_array().unwrap()[0].as_table().unwrap();
    let hardened_rule =
        hardened["rules"].as_array().unwrap()[0].as_table().unwrap();
    for key in [
        "id",
        "description",
        "path",
        "regex",
        "keywords",
        "secretGroup",
    ] {
        assert_eq!(hardened_rule.get(key), original_rule.get(key));
    }

    assert!(!hardened.contains_key("allowlist"));
    let globals = hardened["allowlists"].as_array().unwrap();
    assert_eq!(globals.len(), 1);
    let global = globals[0].as_table().unwrap();
    assert_eq!(global["id"].as_str(), Some("targeted-or"));
    assert!(!global.contains_key("paths"));
    assert_eq!(global["regexes"], original["allowlists"][0]["regexes"]);
    assert_eq!(
        global["targetRules"],
        original["allowlists"][0]["targetRules"]
    );

    let locals = hardened_rule["allowlists"].as_array().unwrap();
    assert_eq!(locals.len(), 1);
    assert!(!locals[0].as_table().unwrap().contains_key("paths"));
    assert_eq!(
        locals[0]["regexes"],
        original_rule["allowlists"][1]["regexes"]
    );
}

#[test]
fn test_bundled_hardening_preserves_every_detector_definition() {
    let source =
        toml::from_str::<toml::Table>(secrets_scanner::rules::BUNDLED_RULES)
            .unwrap();
    let hardened =
        hardened_ruleset_for_tests(secrets_scanner::rules::BUNDLED_RULES)
            .unwrap();
    let source_rules = source["rules"].as_array().unwrap();
    let hardened_rules = hardened["rules"].as_array().unwrap();

    assert_eq!(hardened_rules.len(), source_rules.len());
    for (source, hardened) in source_rules.iter().zip(hardened_rules) {
        let mut source_detector = source.as_table().unwrap().clone();
        let mut hardened_detector = hardened.as_table().unwrap().clone();
        source_detector.remove("allowlists");
        hardened_detector.remove("allowlists");
        assert_eq!(hardened_detector, source_detector);
    }
}

#[test]
fn test_path_filters_apply_without_global_or_local_path_suppression() {
    let rules = r#"
[[allowlists]]
paths = ['suppressed']
regexes = ['GLOBALOK']
targetRules = ['global-detector']

[[rules]]
id = 'global-detector'
path = '\.cfg$'
regex = 'global=([A-Z]{8})'
keywords = ['global=']
secretGroup = 1

[[rules]]
id = 'local-detector'
path = '\.cfg$'
regex = 'local=([A-Z]{8})'
keywords = ['local=']
secretGroup = 1

[[rules.allowlists]]
paths = ['suppressed']
regexes = ['LOCALOKE']
"#;
    let scanner = SecretScanner::from_rules_for_workers(rules, 2).unwrap();
    let text = "global=SECRETAA local=SECRETAA";

    assert_eq!(
        scanner.scan_findings("suppressed.cfg", text).unwrap().len(),
        2
    );
    assert!(
        scanner
            .scan_findings("suppressed.txt", text)
            .unwrap()
            .is_empty()
    );
    assert!(
        scanner
            .scan_findings("ordinary.cfg", "global=GLOBALOK local=LOCALOKE",)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn test_and_path_allowlists_are_removed_instead_of_broadened() {
    let rules = r#"
[[allowlists]]
condition = 'AND'
paths = ['safe\.cfg$']
regexes = ['SECRETAA']
targetRules = ['global-rule']

[[rules]]
id = 'global-rule'
regex = 'global=([A-Z]{8})'
keywords = ['global=']
secretGroup = 1

[[rules]]
id = 'local-rule'
regex = 'local=([A-Z]{8})'
keywords = ['local=']
secretGroup = 1

[[rules.allowlists]]
condition = 'and'
paths = ['safe\.cfg$']
stopwords = ['secretaa']
"#;
    let scanner = SecretScanner::from_rules_for_workers(rules, 2).unwrap();
    let findings = scanner
        .scan_findings("safe.cfg", "global=SECRETAA local=SECRETAA")
        .unwrap();

    assert_eq!(findings.len(), 2);
}

#[test]
fn test_bundled_path_gated_hashicorp_rule_is_reachable() {
    let scanner = SecretScanner::from_bundled_for_workers(2).unwrap();
    let password = ["A7b9", "C2d4", "E6f8"].concat();
    let text = format!("password = \"{password}\"");

    let matching = scanner.scan_findings("main.tf", &text).unwrap();
    let non_matching = scanner.scan_findings("main.txt", &text).unwrap();
    assert!(
        matching
            .iter()
            .any(|finding| finding.rule_id == "hashicorp-tf-password")
    );
    assert!(
        non_matching
            .iter()
            .all(|finding| finding.rule_id != "hashicorp-tf-password")
    );
}

#[test]
fn test_single_and_partitioned_scanners_share_hardened_semantics() {
    let rules = r#"
[allowlist]
paths = ['suppressed']

[[rules]]
id = 'detector'
path = '\.cfg$'
regex = 'token=([A-Z]{8})'
keywords = ['token=']
secretGroup = 1
"#;
    let one = SecretScanner::from_rules_for_workers(rules, 1).unwrap();
    let many = SecretScanner::from_rules_for_workers(rules, 3).unwrap();
    let one = one
        .scan_findings("suppressed.cfg", "token=SECRETAA")
        .unwrap();
    let many = many
        .scan_findings("suppressed.cfg", "token=SECRETAA")
        .unwrap();

    assert_findings_equivalent(&one, &many);
    assert_eq!(one.len(), 1);
}

#[test]
fn test_one_worker_uses_one_hardened_partition() {
    let scanner = SecretScanner::from_bundled_for_workers(1).unwrap();

    assert_eq!(scanner.partition_count(), 1);
    assert_eq!(scanner.partitioned().partitions.len(), 1);
}

#[test]
fn test_assignment_is_round_robin_balanced_and_stable() {
    let first =
        SecretScanner::from_rules_for_workers(SYNTHETIC_RULES, 3).unwrap();
    let second =
        SecretScanner::from_rules_for_workers(SYNTHETIC_RULES, 3).unwrap();
    let first = first.partitioned();
    let second = second.partitioned();

    assert_eq!(first.rule_order, second.rule_order);
    let by_source = first
        .rule_order
        .iter()
        .map(|(id, order)| {
            (order.source_index, (id.as_str(), order.owner_partition))
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        by_source.values().copied().collect::<Vec<_>>(),
        [
            ("path-only", 0),
            ("keyworded-a", 1),
            ("unkeyworded", 2),
            ("keyworded-b", 0),
        ]
    );
    let mut counts = vec![0; 3];
    for order in first.rule_order.values() {
        counts[order.owner_partition] += 1;
    }
    assert_eq!(counts, [2, 1, 1]);
}

#[test]
fn test_rule_phases_follow_released_scanner_order() {
    let scanner =
        SecretScanner::from_rules_for_workers(SYNTHETIC_RULES, 2).unwrap();
    let order = &scanner.partitioned().rule_order;

    assert_eq!(order["path-only"].phase, RulePhase::PathOnly);
    assert_eq!(order["keyworded-a"].phase, RulePhase::Keyworded);
    assert_eq!(order["keyworded-b"].phase, RulePhase::Keyworded);
    assert_eq!(order["unkeyworded"].phase, RulePhase::Unkeyworded);
}

#[test]
fn test_every_top_level_value_survives_each_subset() {
    let source = toml::from_str::<toml::Table>(SYNTHETIC_RULES).unwrap();
    let mut expected_base = source.clone();
    expected_base.remove("rules");
    let scanners = build_partitioned_scanners(
        SYNTHETIC_RULES,
        3,
        &SecretScanner::scanner_config_for_tests(),
    )
    .unwrap();

    for partition in &scanners.partitions {
        let mut subset =
            toml::from_str::<toml::Table>(&partition.serialized_ruleset)
                .unwrap();
        let rules = subset.remove("rules").unwrap();
        assert_eq!(subset, expected_base);
        assert!(rules.as_array().is_some_and(|rules| !rules.is_empty()));
    }
}

#[test]
fn test_real_bundled_rules_have_exact_ownership_and_compiled_union() {
    let scanner = SecretScanner::from_bundled_for_workers(4).unwrap();
    let scanners = scanner.partitioned();
    let compiled = scanners
        .partitions
        .iter()
        .flat_map(|partition| partition.scanner.engine().rules())
        .map(|rule| rule.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(compiled.len(), scanners.rule_order.len());
    assert!(
        scanners
            .rule_order
            .keys()
            .all(|id| compiled.contains(id.as_str()))
    );
}

#[test]
fn test_compiled_count_set_and_union_mismatches_fail_closed() {
    let cases = [
        (&["a", "b"][..], &["a"][..], 1),
        (&["a"][..], &["unknown"][..], 1),
        (&["a"][..], &["a"][..], 2),
        (&["a", "a"][..], &["a"][..], 1),
    ];
    for (assigned, compiled, compiled_count) in cases {
        assert!(matches!(
            prove_compiled_partition_for_tests(
                assigned,
                compiled,
                compiled_count,
                &mut std::collections::BTreeSet::new(),
            ),
            Err(SecretScanError::PartitionIntegrity)
        ));
    }

    let mut union = ["a".to_string()].into_iter().collect();
    assert!(matches!(
        prove_compiled_partition_for_tests(&["a"], &["a"], 1, &mut union,),
        Err(SecretScanError::PartitionIntegrity)
    ));
}

#[test]
fn test_malformed_ruleset_shapes_fail_closed() {
    let cases = [
        "not = [valid",
        "title = 'missing rules'",
        "rules = 'wrong type'",
        "rules = []",
        "rules = [1]",
        "[[rules]]\nregex = 'x'",
        "[[rules]]\nid = 1\nregex = 'x'",
        "[[rules]]\nid = ''\nregex = 'x'",
        "[[rules]]\nid = 'duplicate'\nregex = 'a'\n\
         [[rules]]\nid = 'duplicate'\nregex = 'b'",
        "[[rules]]\nid = 'keywords'\nregex = 'x'\nkeywords = 'x'",
        "[[rules]]\nid = 'keywords'\nregex = 'x'\nkeywords = [1]",
        "[[rules]]\nid = 'keywords'\nregex = 'x'\nkeywords = ['']",
        "[[rules]]\nid = 'predicate'",
        "[[rules]]\nid = 'regex'\nregex = 1",
        "[[rules]]\nid = 'path'\npath = 1",
    ];

    for rules in cases {
        let error = SecretScanner::from_rules_for_workers(rules, 2)
            .err()
            .unwrap();
        assert!(matches!(error, SecretScanError::InvalidRuleset));
    }
}

#[test]
fn test_strict_subset_construction_failures_are_fatal() {
    let cases = [
        "regex = '['",
        "path = '['",
        "regex = '(value)'\nsecretGroup = 2",
        "regex = 'value'\n[[rules.allowlists]]\nregexes = ['[']",
    ];

    for body in cases {
        let rules = format!(
            "[[rules]]\nid = 'strict'\ndescription = 'strict'\n{body}\n"
        );
        let error = SecretScanner::from_rules_for_workers(&rules, 2)
            .err()
            .unwrap();
        let cause = match &error {
            SecretScanError::PartitionSetup(cause) => cause.to_string(),
            _ => panic!("expected partition setup error"),
        };
        assert_eq!(
            error.to_string(),
            format!("failed to load a bundled rule partition: {cause}")
        );
    }

    let invalid_global = "[allowlist]\nregexes = ['[']\n\
        [[rules]]\nid = 'strict'\nregex = 'value'\n";
    assert!(matches!(
        SecretScanner::from_rules_for_workers(invalid_global, 2),
        Err(SecretScanError::PartitionSetup(_))
    ));
}

#[test]
fn test_global_allowlists_and_cross_partition_targets_match_full_scanner() {
    let rules = r#"
[[allowlists]]
id = "cross-partition"
regexTarget = "secret"
regexes = ['^ALLOWME$']
targetRules = ["rule-a", "rule-b"]

[[allowlists]]
id = "plural"
regexTarget = "secret"
regexes = ['^PLURALX$']

[[rules]]
id = "rule-a"
regex = '''a=([A-Z]{7})'''
keywords = ["a="]
secretGroup = 1

[[rules]]
id = "rule-b"
regex = '''b=([A-Z]{7})'''
keywords = ["b="]
secretGroup = 1

[[rules]]
id = "rule-c"
regex = '''c=([A-Z]{7})'''
keywords = ["c="]
secretGroup = 1
"#;
    let text = "a=ALLOWME b=ALLOWME c=ALLOWME c=PLURALX";

    compare_reference_and_partitioned(rules, "repository-content", text, 2);
    let scanner = SecretScanner::from_rules_for_workers(rules, 2).unwrap();
    assert_eq!(
        scanner
            .scan_findings("repository-content", text)
            .unwrap()
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect::<Vec<_>>(),
        ["rule-c"]
    );
}

#[test]
fn test_singular_and_per_rule_allowlists_match_full_scanner() {
    let rules = r#"
[allowlist]
regexTarget = "secret"
regexes = ['^GLOBALX$']

[[rules]]
id = "rule-a"
regex = '''a=([A-Z]{7})'''
keywords = ["a="]
secretGroup = 1

[[rules.allowlists]]
regexTarget = "secret"
regexes = ['^LOCALXX$']

[[rules]]
id = "rule-b"
regex = '''b=([A-Z]{7})'''
keywords = ["b="]
secretGroup = 1
"#;

    compare_reference_and_partitioned(
        rules,
        "repository-content",
        "a=GLOBALX a=LOCALXX b=LOCALXX",
        2,
    );
}

#[test]
fn test_partitioned_findings_match_every_reference_field_and_order() {
    let text =
        "token_b=BBBB2222 UNKEY-UNKEYVAL token_a=AAAA1111 token_a=CCCC3333";

    compare_reference_and_partitioned(
        SYNTHETIC_RULES,
        "repository-content",
        text,
        3,
    );
}

#[test]
fn test_real_bundled_partitioned_findings_and_redaction_match_reference() {
    let secrets = [github_pat(), gitlab_pat(), aws_access_key()];
    let text = format!(
        "github = \"{}\"\r\ngitlab = \"{}\"\naws = \"{}\"\n",
        secrets[0], secrets[1], secrets[2]
    );
    let reference = Scanner::from_bundled()
        .unwrap()
        .with_config(SecretScanner::scanner_config_for_tests())
        .scan_content_detailed("repository-content", &text);
    let scanner = SecretScanner::from_bundled_for_workers(4).unwrap();
    let partitioned =
        scanner.scan_findings("repository-content", &text).unwrap();

    assert_findings_equivalent(&reference.findings, &partitioned);
    let expected = redact_findings(
        &text,
        reference
            .findings
            .into_iter()
            .map(|finding| SafeFinding {
                start: finding.secret_start_offset,
                end: finding.secret_end_offset,
                secret_type: normalize_secret_type(
                    &finding.rule_description,
                    &finding.rule_id,
                ),
                rule_id: finding.rule_id,
            })
            .collect(),
    )
    .unwrap();
    let actual = scanner.redact_text("repository-content", &text).unwrap();
    assert_eq!(actual.text, expected);
    assert_eq!(actual.findings, partitioned.len());
}

#[test]
fn test_merge_rejects_missing_duplicate_and_wrong_partition_results() {
    let scanner =
        SecretScanner::from_rules_for_workers(SYNTHETIC_RULES, 2).unwrap();
    let scanners = scanner.partitioned();
    let empty = || ScanResult {
        findings: Vec::new(),
        findings_truncated: false,
    };

    assert!(matches!(
        merge_test_results(scanners, vec![(0, empty())]),
        Err(SecretScanError::PartitionIntegrity)
    ));
    assert!(matches!(
        merge_test_results(scanners, vec![(0, empty()), (0, empty())]),
        Err(SecretScanError::PartitionIntegrity)
    ));
    assert!(matches!(
        merge_test_results(scanners, vec![(0, empty()), (2, empty())]),
        Err(SecretScanError::PartitionIntegrity)
    ));

    let mut results =
        partition_results(scanners, "repository-content", "token_a=AAAA1111");
    let finding = results[1].1.findings.pop().unwrap();
    results[0].1.findings.push(finding.clone());
    assert!(matches!(
        merge_test_results(scanners, results),
        Err(SecretScanError::PartitionIntegrity)
    ));

    let mut results =
        partition_results(scanners, "repository-content", "token_a=AAAA1111");
    results[1].1.findings[0].rule_id = "unknown-rule".to_string();
    assert!(matches!(
        merge_test_results(scanners, results),
        Err(SecretScanError::PartitionIntegrity)
    ));

    let mut results =
        partition_results(scanners, "repository-content", "safe");
    results[0].1.findings_truncated = true;
    assert!(matches!(
        merge_test_results(scanners, results),
        Err(SecretScanError::TruncatedFindings)
    ));
}

fn partition_results(
    scanners: &crate::secret_scanning::partitions::PartitionedScanners,
    path: &str,
    text: &str,
) -> Vec<(usize, ScanResult)> {
    scanners
        .partitions
        .iter()
        .map(|partition| {
            (
                partition.ordinal,
                partition.scanner.scan_content_detailed(path, text),
            )
        })
        .collect()
}

fn compare_reference_and_partitioned(
    rules: &str,
    path: &str,
    text: &str,
    workers: usize,
) {
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
