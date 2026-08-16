use super::*;
use crate::secret_scanning::execution::{
    TestExecution, WorkerFault, panic_hook_delivery_for_tests,
    panic_hook_restoration_for_tests, scan_parallel_with_test_execution,
    scan_sequential, scan_sequential_with_test_panic,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const EXECUTION_RULES: &str = r#"
[[rules]]
id = "first"
description = "First"
regex = '''(FIRSTSECRET)'''
keywords = ["FIRSTSECRET"]
secretGroup = 1

[[rules]]
id = "last"
description = "Last"
regex = '''(LASTSECRET)'''
keywords = ["LASTSECRET"]
secretGroup = 1

[[rules]]
id = "multiline"
description = "Multiline"
regex = '''(?s)(BEGINSECRET.*?ENDSECRET)'''
keywords = ["BEGINSECRET"]
secretGroup = 1
"#;

#[test]
fn test_scheduling_requires_multiple_partitions_and_threshold_content() {
    let cases = [
        (1, PARALLEL_SCAN_THRESHOLD - 1, ScanMode::Sequential),
        (1, PARALLEL_SCAN_THRESHOLD, ScanMode::Sequential),
        (1, PARALLEL_SCAN_THRESHOLD + 1, ScanMode::Sequential),
        (2, PARALLEL_SCAN_THRESHOLD - 1, ScanMode::Sequential),
        (2, PARALLEL_SCAN_THRESHOLD, ScanMode::Parallel),
        (2, PARALLEL_SCAN_THRESHOLD + 1, ScanMode::Parallel),
    ];

    for (partitions, text_len, expected) in cases {
        assert_eq!(
            scan_mode(ScanSchedule::Content, text_len, partitions),
            expected
        );
    }

    for partitions in [1, 2] {
        assert_eq!(
            scan_mode(ScanSchedule::RepositoryPath, usize::MAX, partitions),
            ScanMode::Sequential
        );
    }
}

#[test]
fn test_only_scanner_worker_panics_are_hidden_from_previous_hook() {
    assert_eq!(panic_hook_delivery_for_tests(), 1);
}

#[test]
fn test_filtering_hook_restores_exact_hook_repeatedly_and_during_unwind() {
    assert_eq!(panic_hook_restoration_for_tests(), (true, true));
}

#[test]
fn test_sequential_scanner_panic_is_hidden_and_fails_closed() {
    let scanner =
        SecretScanner::from_rules_for_workers(EXECUTION_RULES, 3).unwrap();
    let private_path = "private/repository/path";
    let private_content = "FIRSTSECRET private-content LASTSECRET";
    let (result, deliveries) = scan_sequential_with_test_panic(
        scanner.partitioned(),
        private_path,
        private_content,
    );
    let message = result.unwrap_err().to_string();

    assert_eq!(message, "secret scanning panicked");
    assert_eq!(deliveries, 0);
    assert!(!message.contains(private_path));
    assert!(!message.contains(private_content));
    assert!(!message.contains("private sequential scanner payload"));
    assert!(!message.contains("FIRSTSECRET"));
}

#[test]
fn test_threshold_cases_scan_the_same_complete_content() {
    let scanner =
        SecretScanner::from_rules_for_workers(EXECUTION_RULES, 3).unwrap();
    for size in [
        PARALLEL_SCAN_THRESHOLD - 1,
        PARALLEL_SCAN_THRESHOLD,
        PARALLEL_SCAN_THRESHOLD + 1,
    ] {
        let text = text_with_secret_at_size(size, "LASTSECRET");
        let redaction =
            scanner.redact_text("repository-content", &text).unwrap();

        assert_eq!(redaction.findings, 1);
        assert!(!redaction.text.contains("LASTSECRET"));
        assert_eq!(redaction.text.len(), size - 10 + 22);
    }
}

#[test]
fn test_parallel_partitions_scan_first_last_and_unbounded_multiline_regions() {
    let scanner =
        SecretScanner::from_rules_for_workers(EXECUTION_RULES, 3).unwrap();
    let mut text = String::from("FIRSTSECRET\nBEGINSECRET\n");
    text.push_str(&"middle\n".repeat(PARALLEL_SCAN_THRESHOLD / 7));
    text.push_str("ENDSECRET\nLASTSECRET");

    let findings = scanner.scan_findings("repository-content", &text).unwrap();
    let redaction = scanner.redact_text("repository-content", &text).unwrap();

    assert_eq!(
        findings
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect::<Vec<_>>(),
        ["first", "last", "multiline"]
    );
    for marker in ["FIRSTSECRET", "BEGINSECRET", "ENDSECRET", "LASTSECRET"] {
        assert!(!redaction.text.contains(marker));
    }
}

#[test]
fn test_reverse_worker_completion_preserves_released_order() {
    let scanner =
        SecretScanner::from_rules_for_workers(EXECUTION_RULES, 3).unwrap();
    let scanners = scanner.partitioned();
    let text = "FIRSTSECRET BEGINSECRET body ENDSECRET LASTSECRET";
    let completed = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let execution = TestExecution {
        fault: None,
        completion_order: Some(vec![2, 1, 0]),
        completed: Arc::clone(&completed),
        observed_order: Arc::clone(&observed),
    };

    let sequential =
        scan_sequential(scanners, "repository-content", text).unwrap();
    let parallel = scan_parallel_with_test_execution(
        scanners,
        "repository-content",
        text,
        &execution,
    )
    .unwrap();

    assert_eq!(completed.load(Ordering::SeqCst), 3);
    assert_eq!(*observed.lock().unwrap(), [2, 1, 0]);
    assert_eq!(
        sequential
            .findings
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect::<Vec<_>>(),
        parallel
            .findings
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_parallel_faults_fail_closed_after_every_worker_joins() {
    let scanner =
        SecretScanner::from_rules_for_workers(EXECUTION_RULES, 3).unwrap();
    let scanners = scanner.partitioned();
    let private_content = "FIRSTSECRET private-content LASTSECRET";
    let private_path = "private/repository/path";
    let cases = [
        (WorkerFault::Error, 2, 2, "a secret scan worker failed"),
        (WorkerFault::Panic, 0, 3, "secret scanning panicked"),
        (
            WorkerFault::Missing,
            0,
            3,
            "bundled secret rule partition validation failed",
        ),
        (
            WorkerFault::Duplicate,
            0,
            3,
            "bundled secret rule partition validation failed",
        ),
        (
            WorkerFault::Truncated,
            0,
            3,
            "secret scanner returned incomplete findings",
        ),
    ];

    for (fault, fault_ordinal, expected_completed, expected_message) in cases {
        let completed = Arc::new(AtomicUsize::new(0));
        let execution = TestExecution {
            fault: Some((fault_ordinal, fault)),
            completion_order: None,
            completed: Arc::clone(&completed),
            observed_order: Arc::new(Mutex::new(Vec::new())),
        };
        let error = scan_parallel_with_test_execution(
            scanners,
            private_path,
            private_content,
            &execution,
        )
        .unwrap_err();
        let message = error.to_string();

        assert_eq!(completed.load(Ordering::SeqCst), expected_completed);
        assert_eq!(message, expected_message);
        assert!(!message.contains(private_content));
        assert!(!message.contains(private_path));
        assert!(!message.contains("private scanner panic payload"));
        assert!(!message.contains("FIRSTSECRET"));
    }
}

#[test]
fn test_parallel_scans_are_repeatedly_deterministic() {
    let scanner =
        SecretScanner::from_rules_for_workers(EXECUTION_RULES, 3).unwrap();
    let mut text = "padding".repeat(PARALLEL_SCAN_THRESHOLD / 7 + 1);
    text.push_str(" FIRSTSECRET LASTSECRET");
    let first = scanner.redact_text("repository-content", &text).unwrap();

    for _ in 0..5 {
        let next = scanner.redact_text("repository-content", &text).unwrap();
        assert_eq!(next.text, first.text);
        assert_eq!(next.findings, first.findings);
    }
}

fn text_with_secret_at_size(size: usize, secret: &str) -> String {
    let mut text = "x".repeat(size - secret.len());
    text.push_str(secret);
    text
}
