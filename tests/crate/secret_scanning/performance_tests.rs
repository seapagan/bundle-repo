use super::*;
use secrets_scanner::{Finding, Scanner};
use std::hint::black_box;
use std::time::{Duration, Instant};

const BENCHMARK_SAMPLES: usize = 3;

struct Workload {
    label: &'static str,
    text: String,
}

struct ReferenceEvidence {
    findings: Vec<Finding>,
    protected: String,
    samples: Vec<Duration>,
}

#[test]
#[ignore = "manual release-only secret-scanning performance benchmark"]
fn test_secret_scanning_release_performance() {
    let workloads = benchmark_workloads();
    let reference = Scanner::from_bundled()
        .unwrap()
        .with_config(SecretScanner::scanner_config_for_tests());
    let evidence = workloads
        .iter()
        .map(|workload| reference_evidence(&reference, workload))
        .collect::<Vec<_>>();
    drop(reference);

    let partitioned = SecretScanner::from_bundled().unwrap();
    let workers = partitioned.partition_count();
    for (workload, reference) in workloads.iter().zip(&evidence) {
        benchmark_partitioned(&partitioned, workers, workload, reference);
    }
    benchmark_sequential_large_inputs(
        &partitioned,
        workers,
        workloads
            .iter()
            .find(|workload| workload.label == "keyword-density-4mib")
            .unwrap(),
    );
}

fn reference_evidence(
    scanner: &Scanner,
    workload: &Workload,
) -> ReferenceEvidence {
    let result = scanner.scan_content_detailed(
        "repository-content",
        black_box(&workload.text),
    );
    assert!(!result.findings_truncated);
    let protected = protect_reference(&workload.text, &result.findings);
    let samples = measure_samples(|| {
        scanner
            .scan_content_detailed(
                "repository-content",
                black_box(&workload.text),
            )
            .findings
            .len()
    });
    ReferenceEvidence {
        findings: result.findings,
        protected,
        samples,
    }
}

fn benchmark_partitioned(
    scanner: &SecretScanner,
    workers: usize,
    workload: &Workload,
    reference: &ReferenceEvidence,
) {
    let findings = scanner
        .scan_findings("repository-content", &workload.text)
        .unwrap();
    assert_findings_equivalent(&reference.findings, &findings);
    let protected = scanner
        .redact_text("repository-content", &workload.text)
        .unwrap();
    assert!(
        protected.text == reference.protected,
        "protected output mismatch for benchmark label {}",
        workload.label
    );
    let samples = measure_samples(|| {
        scanner
            .scan_findings("repository-content", black_box(&workload.text))
            .unwrap()
            .len()
    });
    report_samples(
        workload,
        workers,
        findings.len(),
        &reference.samples,
        &samples,
    );
}

fn benchmark_sequential_large_inputs(
    scanner: &SecretScanner,
    workers: usize,
    workload: &Workload,
) {
    const INPUTS: usize = 6;
    let samples = measure_samples(|| {
        (0..INPUTS)
            .map(|_| {
                scanner
                    .scan_findings(
                        "repository-content",
                        black_box(&workload.text),
                    )
                    .unwrap()
                    .len()
            })
            .sum()
    });
    let (median, dispersion) = median_and_dispersion(&samples);
    println!(
        "BUNDLEREPO_SECRET_BENCH label=sequential-large-inputs \
         bytes={} inputs={INPUTS} workers={workers} findings={} \
         partitioned_median_nanos={} partitioned_mad_nanos={}",
        workload.text.len() * INPUTS,
        scanner
            .scan_findings("repository-content", &workload.text)
            .unwrap()
            .len()
            * INPUTS,
        median.as_nanos(),
        dispersion.as_nanos(),
    );
}

fn measure_samples(mut operation: impl FnMut() -> usize) -> Vec<Duration> {
    (0..BENCHMARK_SAMPLES)
        .map(|_| {
            let started = Instant::now();
            black_box(operation());
            started.elapsed()
        })
        .collect()
}

fn report_samples(
    workload: &Workload,
    workers: usize,
    findings: usize,
    reference: &[Duration],
    partitioned: &[Duration],
) {
    let (reference_median, reference_dispersion) =
        median_and_dispersion(reference);
    let (partitioned_median, partitioned_dispersion) =
        median_and_dispersion(partitioned);
    println!(
        "BUNDLEREPO_SECRET_BENCH label={} bytes={} workers={} findings={} \
         reference_median_nanos={} reference_mad_nanos={} \
         partitioned_median_nanos={} partitioned_mad_nanos={}",
        workload.label,
        workload.text.len(),
        workers,
        findings,
        reference_median.as_nanos(),
        reference_dispersion.as_nanos(),
        partitioned_median.as_nanos(),
        partitioned_dispersion.as_nanos(),
    );
}

fn median_and_dispersion(samples: &[Duration]) -> (Duration, Duration) {
    let mut values = samples.to_vec();
    values.sort_unstable();
    let median = values[values.len() / 2];
    let mut deviations = values
        .iter()
        .map(|value| value.abs_diff(median))
        .collect::<Vec<_>>();
    deviations.sort_unstable();
    (median, deviations[deviations.len() / 2])
}

fn protect_reference(text: &str, findings: &[Finding]) -> String {
    redact_findings(
        text,
        findings
            .iter()
            .map(|finding| SafeFinding {
                start: finding.secret_start_offset,
                end: finding.secret_end_offset,
                secret_type: normalize_secret_type(
                    &finding.rule_description,
                    &finding.rule_id,
                ),
                rule_id: finding.rule_id.clone(),
            })
            .collect(),
    )
    .unwrap()
}

fn benchmark_workloads() -> Vec<Workload> {
    let mut workloads = vec![
        Workload {
            label: "ordinary-sub-mib",
            text: sized_text("ordinary repository text\n", 256 * 1024),
        },
        Workload {
            label: "keyword-density-1mib",
            text: keyword_dense_text(1024 * 1024),
        },
        Workload {
            label: "keyword-density-4mib",
            text: keyword_dense_text(4 * 1024 * 1024),
        },
        Workload {
            label: "keyword-density-16mib",
            text: keyword_dense_text(16 * 1024 * 1024),
        },
        Workload {
            label: "finding-heavy",
            text: finding_heavy_text(),
        },
        Workload {
            label: "unbounded-multiline",
            text: unbounded_multiline_text(),
        },
    ];
    add_optional_tokenizers(&mut workloads);
    workloads
}

fn keyword_dense_text(size: usize) -> String {
    sized_text(
        "token api key secret password credential github gitlab aws slack\n",
        size,
    )
}

fn sized_text(fragment: &str, size: usize) -> String {
    let mut text = fragment.repeat(size.div_ceil(fragment.len()));
    text.truncate(size);
    text
}

fn finding_heavy_text() -> String {
    let secret = synthetic_github_pat();
    (0..256)
        .map(|index| format!("token_{index}={secret}\n"))
        .collect()
}

fn unbounded_multiline_text() -> String {
    let mut text = String::from("-----BEGIN PRIVATE KEY-----\n");
    text.push_str(&"A".repeat(4096));
    text.push_str("\n-----END PRIVATE KEY-----\n");
    text
}

fn add_optional_tokenizers(workloads: &mut Vec<Workload>) {
    let assets = [
        (
            "tokenizer-deepseek-r1",
            "resources/tokenizers/deepseek-r1.json",
        ),
        (
            "tokenizer-deepseek-v3",
            "resources/tokenizers/deepseek-v3.json",
        ),
        (
            "tokenizer-deepseek-v4",
            "resources/tokenizers/deepseek-v4.json",
        ),
        ("tokenizer-glm-5-2", "resources/tokenizers/glm-5.2.json"),
    ];
    for (label, path) in assets {
        if let Ok(text) = std::fs::read_to_string(path) {
            workloads.push(Workload { label, text });
        }
    }
}
