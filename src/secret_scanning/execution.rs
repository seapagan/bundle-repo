use super::SecretScanError;
use super::partitions::{PartitionedScanners, RuleOrder};
use secrets_scanner::{Finding, ScanResult};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn scan_sequential(
    scanners: &PartitionedScanners,
    path: &str,
    text: &str,
) -> Result<ScanResult, SecretScanError> {
    let results = scanners
        .partitions
        .iter()
        .map(|partition| {
            (
                partition.ordinal,
                partition.scanner.scan_content_detailed(path, text),
            )
        })
        .collect();
    merge_results(scanners, results)
}

fn merge_results(
    scanners: &PartitionedScanners,
    results: Vec<(usize, ScanResult)>,
) -> Result<ScanResult, SecretScanError> {
    let mut seen_partitions = BTreeSet::new();
    let mut ordered = Vec::new();
    for (partition, result) in results {
        if result.findings_truncated {
            return Err(SecretScanError::TruncatedFindings);
        }
        if partition >= scanners.partitions.len()
            || !seen_partitions.insert(partition)
        {
            return Err(SecretScanError::PartitionIntegrity);
        }
        append_findings(scanners, partition, result.findings, &mut ordered)?;
    }
    if seen_partitions.len() != scanners.partitions.len() {
        return Err(SecretScanError::PartitionIntegrity);
    }
    ordered.sort_by_key(|(order, occurrence, _)| {
        (order.phase, order.source_index, *occurrence)
    });
    Ok(ScanResult {
        findings: ordered.into_iter().map(|(_, _, finding)| finding).collect(),
        findings_truncated: false,
    })
}

fn append_findings(
    scanners: &PartitionedScanners,
    partition: usize,
    findings: Vec<Finding>,
    ordered: &mut Vec<(RuleOrder, usize, Finding)>,
) -> Result<(), SecretScanError> {
    let mut occurrences = BTreeMap::<String, usize>::new();
    for finding in findings {
        let order = scanners
            .rule_order
            .get(&finding.rule_id)
            .copied()
            .ok_or(SecretScanError::PartitionIntegrity)?;
        if order.owner_partition != partition {
            return Err(SecretScanError::PartitionIntegrity);
        }
        let occurrence =
            occurrences.entry(finding.rule_id.clone()).or_default();
        ordered.push((order, *occurrence, finding));
        *occurrence += 1;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn merge_test_results(
    scanners: &PartitionedScanners,
    results: Vec<(usize, ScanResult)>,
) -> Result<ScanResult, SecretScanError> {
    merge_results(scanners, results)
}
