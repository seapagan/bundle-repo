mod execution;
mod partitions;
mod paths;
mod redaction;

use partitions::{PartitionedScanners, build_partitioned_scanners};
#[cfg(test)]
use redaction::redact_findings;
use redaction::{
    SafeFinding, common_secret_type, normalize_secret_type, normalized_ranges,
    redact_ranges,
};
#[cfg(test)]
use secrets_scanner::{Finding, Scanner};
use secrets_scanner::{RedactionMode, ScanConfig, ScanResult};
use std::error::Error;
use std::fmt;
use std::num::NonZeroUsize;

const PATH_COMPONENT_SCAN_PATH: &str = "repository-path-component";
const MAX_SCAN_WORKERS: usize = 28;
const PARALLEL_SCAN_THRESHOLD: usize = 1024 * 1024;

#[cfg(test)]
pub(crate) fn synthetic_github_pat() -> String {
    ["ghp_", "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8"].concat()
}

pub(crate) struct SecretScanner {
    scanners: PartitionedScanners,
}

pub(crate) struct SecretRedaction {
    pub(crate) text: String,
    pub(crate) findings: usize,
    pub(crate) secret_type: Option<String>,
    pub(crate) omit_content: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SkippedItemKind {
    File,
    Subtree,
}

impl SkippedItemKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Subtree => "subtree",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SkipReason {
    SecretInPath { secret_type: Option<String> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SkippedRepositoryItem {
    pub(crate) kind: SkippedItemKind,
    pub(crate) safe_path: String,
    pub(crate) reason: SkipReason,
}

pub(crate) struct RepositoryPathScan {
    pub(crate) included: Vec<String>,
    pub(crate) skipped: Vec<SkippedRepositoryItem>,
    pub(crate) findings: usize,
}

impl RepositoryPathScan {
    pub(crate) fn unscanned(paths: Vec<String>) -> Self {
        Self {
            included: paths,
            skipped: Vec::new(),
            findings: 0,
        }
    }
}

#[derive(Debug)]
pub(crate) enum SecretScanError {
    PartitionSetup(secrets_scanner::ScannerError),
    InvalidRuleset,
    PartitionIntegrity,
    PartitionScanFailure,
    WorkerPanic,
    InvalidSpan,
    TruncatedFindings,
}

impl fmt::Display for SecretScanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PartitionSetup(error) => {
                write!(
                    formatter,
                    "failed to load a bundled rule partition: {error}"
                )
            }
            Self::InvalidRuleset => formatter
                .write_str("bundled secret rules have an unsupported format"),
            Self::PartitionIntegrity => formatter
                .write_str("bundled secret rule partition validation failed"),
            Self::PartitionScanFailure => {
                formatter.write_str("a secret scan worker failed")
            }
            Self::WorkerPanic => {
                formatter.write_str("a secret scan worker panicked")
            }
            Self::InvalidSpan => {
                formatter.write_str("secret scanner returned an invalid span")
            }
            Self::TruncatedFindings => formatter
                .write_str("secret scanner returned incomplete findings"),
        }
    }
}

impl Error for SecretScanError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PartitionSetup(error) => Some(error),
            Self::PartitionScanFailure => None,
            Self::InvalidRuleset
            | Self::PartitionIntegrity
            | Self::WorkerPanic
            | Self::InvalidSpan
            | Self::TruncatedFindings => None,
        }
    }
}

impl SecretScanner {
    pub(crate) fn from_bundled() -> Result<Self, SecretScanError> {
        let workers =
            resolved_worker_count(std::thread::available_parallelism().ok());
        Self::from_bundled_with_worker_count(workers)
    }

    fn from_bundled_with_worker_count(
        workers: usize,
    ) -> Result<Self, SecretScanError> {
        let config = scanner_config();
        let scanners = build_partitioned_scanners(
            secrets_scanner::rules::BUNDLED_RULES,
            workers,
            &config,
        )?;
        Ok(Self { scanners })
    }

    fn scan(
        &self,
        scanner_path: &str,
        text: &str,
        schedule: ScanSchedule,
    ) -> Result<ScanResult, SecretScanError> {
        if scan_mode(schedule, text.len()) == ScanMode::Parallel {
            execution::scan_parallel(&self.scanners, scanner_path, text)
        } else {
            execution::scan_sequential(&self.scanners, scanner_path, text)
        }
    }

    #[cfg(test)]
    pub(crate) fn from_bundled_for_workers(
        workers: usize,
    ) -> Result<Self, SecretScanError> {
        Self::from_bundled_with_worker_count(workers)
    }

    #[cfg(test)]
    fn scan_findings(
        &self,
        scanner_path: &str,
        text: &str,
    ) -> Result<Vec<Finding>, SecretScanError> {
        Ok(self
            .scan(scanner_path, text, ScanSchedule::Content)?
            .findings)
    }

    #[cfg(test)]
    fn partitioned(&self) -> &PartitionedScanners {
        &self.scanners
    }

    #[cfg(test)]
    pub(crate) fn from_rules_for_workers(
        rules: &str,
        workers: usize,
    ) -> Result<Self, SecretScanError> {
        let scanners =
            build_partitioned_scanners(rules, workers, &scanner_config())?;
        Ok(Self { scanners })
    }

    #[cfg(test)]
    fn reference_scanner_from_rules(
        rules: &str,
    ) -> Result<Scanner, SecretScanError> {
        Scanner::from_toml(rules)
            .map(|scanner| scanner.with_config(scanner_config()))
            .map_err(SecretScanError::PartitionSetup)
    }

    #[cfg(test)]
    fn scanner_config_for_tests() -> ScanConfig {
        scanner_config()
    }

    #[cfg(test)]
    fn partition_count(&self) -> usize {
        self.partitioned().partitions.len()
    }

    pub(crate) fn redact_text(
        &self,
        path: &str,
        text: &str,
    ) -> Result<SecretRedaction, SecretScanError> {
        self.redact(path, text, ScanSchedule::Content)
    }

    fn redact(
        &self,
        scanner_path: &str,
        text: &str,
        schedule: ScanSchedule,
    ) -> Result<SecretRedaction, SecretScanError> {
        let result = self.scan(scanner_path, text, schedule)?;
        self.redact_result(text, result)
    }

    fn redact_result(
        &self,
        text: &str,
        result: ScanResult,
    ) -> Result<SecretRedaction, SecretScanError> {
        if result.findings_truncated {
            return Err(SecretScanError::TruncatedFindings);
        }
        let rule_order = &self.scanners.rule_order;
        let mut omit_content = false;
        let mut findings = 0;
        let spans = result
            .findings
            .into_iter()
            .filter_map(|finding| {
                let is_path_only =
                    rule_order.get(&finding.rule_id).is_some_and(|order| {
                        order.phase == partitions::RulePhase::PathOnly
                    });
                if is_path_only {
                    omit_content = true;
                    return None;
                }
                findings += 1;
                Some(SafeFinding {
                    start: finding.secret_start_offset,
                    end: finding.secret_end_offset,
                    secret_type: normalize_secret_type(
                        &finding.rule_description,
                        &finding.rule_id,
                    ),
                    rule_id: finding.rule_id,
                })
            })
            .collect();
        let ranges = normalized_ranges(text, spans)?;
        let secret_type = common_secret_type(&ranges);
        let text = redact_ranges(text, ranges);
        Ok(SecretRedaction {
            text,
            findings,
            secret_type,
            omit_content,
        })
    }

    #[cfg(test)]
    fn redact_result_for_tests(
        &self,
        text: &str,
        result: ScanResult,
    ) -> Result<SecretRedaction, SecretScanError> {
        self.redact_result(text, result)
    }

    #[cfg(test)]
    fn rule_ids(&self, text: &str) -> Vec<String> {
        self.scan("repository-content", text, ScanSchedule::Content)
            .unwrap()
            .findings
            .into_iter()
            .map(|finding| finding.rule_id)
            .collect()
    }
}

fn scanner_config() -> ScanConfig {
    ScanConfig {
        redact: true,
        redaction_mode: RedactionMode::Full,
        honor_allow_markers: false,
        capture_context: false,
        max_findings: None,
        max_findings_per_file: None,
        max_file_size: u64::MAX,
        ..ScanConfig::default()
    }
}

fn resolved_worker_count(available: Option<NonZeroUsize>) -> usize {
    available.map_or(1, NonZeroUsize::get).min(MAX_SCAN_WORKERS)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanSchedule {
    RepositoryPath,
    Content,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanMode {
    Sequential,
    Parallel,
}

fn scan_mode(schedule: ScanSchedule, text_len: usize) -> ScanMode {
    if schedule == ScanSchedule::Content && text_len >= PARALLEL_SCAN_THRESHOLD
    {
        ScanMode::Parallel
    } else {
        ScanMode::Sequential
    }
}

#[cfg(test)]
#[path = "../tests/crate/secret_scanning.rs"]
mod tests;
