use super::*;
use crate::secret_scanning::execution::{
    TestExecution, WorkerFault, scan_parallel_with_test_execution,
    scan_sequential,
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
fn test_scheduling_boundary_uses_decoded_byte_length() {
    assert_eq!(
        scan_mode(ScanSchedule::Content, PARALLEL_SCAN_THRESHOLD - 1),
        ScanMode::Sequential
    );
    assert_eq!(
        scan_mode(ScanSchedule::Content, PARALLEL_SCAN_THRESHOLD),
        ScanMode::Parallel
    );
    assert_eq!(
        scan_mode(ScanSchedule::Content, PARALLEL_SCAN_THRESHOLD + 1),
        ScanMode::Parallel
    );
    assert_eq!(
        scan_mode(ScanSchedule::RepositoryPath, usize::MAX),
        ScanMode::Sequential
    );
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
        let redaction = scanner.redact_text(&text).unwrap();

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
    let redaction = scanner.redact_text(&text).unwrap();

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
    let scanners = scanner.partitioned().unwrap();
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
    let scanners = scanner.partitioned().unwrap();
    let private_content = "FIRSTSECRET private-content LASTSECRET";
    let private_path = "private/repository/path";
    let cases = [
        (WorkerFault::Error, "a secret scan worker failed"),
        (WorkerFault::Panic, "a secret scan worker panicked"),
        (
            WorkerFault::Missing,
            "bundled secret rule partition validation failed",
        ),
        (
            WorkerFault::Duplicate,
            "bundled secret rule partition validation failed",
        ),
        (
            WorkerFault::Truncated,
            "secret scanner returned incomplete findings",
        ),
    ];

    for (fault, expected_message) in cases {
        let completed = Arc::new(AtomicUsize::new(0));
        let execution = TestExecution {
            fault: Some((0, fault)),
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

        assert_eq!(completed.load(Ordering::SeqCst), 3);
        assert_eq!(message, expected_message);
        assert!(!message.contains(private_content));
        assert!(!message.contains(private_path));
        assert!(!message.contains("private worker panic payload"));
        assert!(!message.contains("FIRSTSECRET"));
    }
}

#[test]
fn test_parallel_scans_are_repeatedly_deterministic() {
    let scanner =
        SecretScanner::from_rules_for_workers(EXECUTION_RULES, 3).unwrap();
    let mut text = "padding".repeat(PARALLEL_SCAN_THRESHOLD / 7 + 1);
    text.push_str(" FIRSTSECRET LASTSECRET");
    let first = scanner.redact_text(&text).unwrap();

    for _ in 0..5 {
        let next = scanner.redact_text(&text).unwrap();
        assert_eq!(next.text, first.text);
        assert_eq!(next.findings, first.findings);
    }
}

fn text_with_secret_at_size(size: usize, secret: &str) -> String {
    let mut text = "x".repeat(size - secret.len());
    text.push_str(secret);
    text
}
