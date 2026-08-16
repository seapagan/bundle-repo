use super::*;
use crate::secret_scanning::partitions::hardened_ruleset_for_tests;

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
