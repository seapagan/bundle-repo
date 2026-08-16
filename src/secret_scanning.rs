mod execution;
mod partitions;

use partitions::{PartitionedScanners, build_partitioned_scanners};
#[cfg(test)]
use secrets_scanner::Finding;
use secrets_scanner::{RedactionMode, ScanConfig, ScanResult, Scanner};
use std::error::Error;
use std::fmt;
use std::num::NonZeroUsize;

const CONTENT_SCAN_PATH: &str = "repository-content";
const PATH_COMPONENT_SCAN_PATH: &str = "repository-path-component";
const MAX_LABEL_LEN: usize = 80;
const MAX_SCAN_WORKERS: usize = 28;
const PARALLEL_SCAN_THRESHOLD: usize = 1024 * 1024;

#[cfg(test)]
pub(crate) fn synthetic_github_pat() -> String {
    ["ghp_", "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8"].concat()
}

pub(crate) struct SecretScanner {
    scanners: ScannerSet,
}

enum ScannerSet {
    Bundled(Box<Scanner>),
    Partitioned(PartitionedScanners),
}

pub(crate) struct SecretRedaction {
    pub(crate) text: String,
    pub(crate) findings: usize,
    pub(crate) secret_type: Option<String>,
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
    Setup(secrets_scanner::ScannerError),
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
            Self::Setup(error) => {
                write!(formatter, "failed to load bundled rules: {error}")
            }
            Self::PartitionSetup(_) => {
                formatter.write_str("failed to load a bundled rule partition")
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
            Self::Setup(error) | Self::PartitionSetup(error) => Some(error),
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
        let scanners = if workers <= 1 {
            ScannerSet::Bundled(Box::new(
                Scanner::from_bundled()
                    .map_err(SecretScanError::Setup)?
                    .with_config(config),
            ))
        } else {
            ScannerSet::Partitioned(build_partitioned_scanners(
                secrets_scanner::rules::BUNDLED_RULES,
                workers,
                &config,
            )?)
        };
        Ok(Self { scanners })
    }

    fn scan(
        &self,
        scanner_path: &str,
        text: &str,
        schedule: ScanSchedule,
    ) -> Result<ScanResult, SecretScanError> {
        match &self.scanners {
            ScannerSet::Bundled(scanner) => {
                Ok(scanner.scan_content_detailed(scanner_path, text))
            }
            ScannerSet::Partitioned(scanners) => {
                if scan_mode(schedule, text.len()) == ScanMode::Parallel {
                    execution::scan_parallel(scanners, scanner_path, text)
                } else {
                    execution::scan_sequential(scanners, scanner_path, text)
                }
            }
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
    fn is_bundled(&self) -> bool {
        matches!(self.scanners, ScannerSet::Bundled(_))
    }

    #[cfg(test)]
    fn partitioned(&self) -> Option<&PartitionedScanners> {
        match &self.scanners {
            ScannerSet::Bundled(_) => None,
            ScannerSet::Partitioned(scanners) => Some(scanners),
        }
    }

    #[cfg(test)]
    fn from_rules_for_workers(
        rules: &str,
        workers: usize,
    ) -> Result<Self, SecretScanError> {
        let scanners =
            build_partitioned_scanners(rules, workers, &scanner_config())?;
        Ok(Self {
            scanners: ScannerSet::Partitioned(scanners),
        })
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
        self.partitioned().map_or(1, |set| set.partitions.len())
    }

    pub(crate) fn redact_text(
        &self,
        text: &str,
    ) -> Result<SecretRedaction, SecretScanError> {
        self.redact(CONTENT_SCAN_PATH, text, ScanSchedule::Content)
    }

    pub(crate) fn scan_repository_paths(
        &self,
        paths: Vec<String>,
    ) -> Result<RepositoryPathScan, SecretScanError> {
        let mut included = Vec::with_capacity(paths.len());
        let mut skipped = Vec::new();
        let mut findings = 0;

        for path in paths {
            if covered_by_subtree(&path, &skipped) {
                continue;
            }
            let components = path.split('/').collect::<Vec<_>>();
            let mut safe_components = Vec::with_capacity(components.len());
            let mut affected = None;
            for (index, component) in components.iter().enumerate() {
                let redaction = self.redact(
                    PATH_COMPONENT_SCAN_PATH,
                    component,
                    ScanSchedule::RepositoryPath,
                )?;
                findings += redaction.findings;
                safe_components.push(redaction.text);
                if redaction.findings > 0 {
                    affected = Some((index, redaction.secret_type));
                    break;
                }
            }

            let Some((index, secret_type)) = affected else {
                included.push(path);
                continue;
            };
            let kind = if index + 1 == components.len() {
                SkippedItemKind::File
            } else {
                SkippedItemKind::Subtree
            };
            let pending = PendingSkipped {
                original_prefix: components[..=index].join("/"),
                item: SkippedRepositoryItem {
                    kind,
                    safe_path: safe_components.join("/"),
                    reason: SkipReason::SecretInPath { secret_type },
                },
            };
            record_skipped(&mut skipped, pending);
        }

        let mut skipped = skipped
            .into_iter()
            .map(|pending| pending.item)
            .collect::<Vec<_>>();
        skipped.sort_by(|left, right| {
            skipped_sort_key(left).cmp(&skipped_sort_key(right))
        });
        Ok(RepositoryPathScan {
            included,
            skipped,
            findings,
        })
    }

    fn redact(
        &self,
        scanner_path: &str,
        text: &str,
        schedule: ScanSchedule,
    ) -> Result<SecretRedaction, SecretScanError> {
        let result = self.scan(scanner_path, text, schedule)?;
        if result.findings_truncated {
            return Err(SecretScanError::TruncatedFindings);
        }
        let findings = result.findings.len();
        let spans = result
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
            .collect();
        let ranges = normalized_ranges(text, spans)?;
        let secret_type = common_secret_type(&ranges);
        let text = redact_ranges(text, ranges);
        Ok(SecretRedaction {
            text,
            findings,
            secret_type,
        })
    }

    #[cfg(test)]
    fn rule_ids(&self, text: &str) -> Vec<String> {
        self.scan(CONTENT_SCAN_PATH, text, ScanSchedule::Content)
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

struct PendingSkipped {
    original_prefix: String,
    item: SkippedRepositoryItem,
}

fn covered_by_subtree(path: &str, skipped: &[PendingSkipped]) -> bool {
    skipped.iter().any(|pending| {
        pending.item.kind == SkippedItemKind::Subtree
            && path_is_at_or_below(path, &pending.original_prefix)
    })
}

fn record_skipped(skipped: &mut Vec<PendingSkipped>, pending: PendingSkipped) {
    if pending.item.kind == SkippedItemKind::Subtree {
        skipped.retain(|existing| {
            !path_is_at_or_below(
                &existing.original_prefix,
                &pending.original_prefix,
            )
        });
    }
    skipped.push(pending);
}

fn path_is_at_or_below(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn skipped_sort_key(
    item: &SkippedRepositoryItem,
) -> (&str, &str, Option<&str>) {
    let secret_type = match &item.reason {
        SkipReason::SecretInPath { secret_type } => secret_type.as_deref(),
    };
    (item.safe_path.as_str(), item.kind.as_str(), secret_type)
}

struct SafeFinding {
    start: usize,
    end: usize,
    secret_type: Option<String>,
    rule_id: String,
}

struct RedactionRange {
    start: usize,
    end: usize,
    secret_type: Option<String>,
}

#[cfg(test)]
fn redact_findings(
    text: &str,
    findings: Vec<SafeFinding>,
) -> Result<String, SecretScanError> {
    let ranges = normalized_ranges(text, findings)?;
    Ok(redact_ranges(text, ranges))
}

fn redact_ranges(text: &str, ranges: Vec<RedactionRange>) -> String {
    if ranges.is_empty() {
        return text.to_string();
    }

    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    for range in ranges {
        output.push_str(&text[cursor..range.start]);
        output.push_str(&redaction_marker(range.secret_type.as_deref()));
        preserve_line_terminators(&text[range.start..range.end], &mut output);
        cursor = range.end;
    }
    output.push_str(&text[cursor..]);
    output
}

fn common_secret_type(ranges: &[RedactionRange]) -> Option<String> {
    let first = ranges.first()?.secret_type.as_ref()?;
    ranges
        .iter()
        .all(|range| range.secret_type.as_ref() == Some(first))
        .then(|| first.clone())
}

fn normalized_ranges(
    text: &str,
    findings: Vec<SafeFinding>,
) -> Result<Vec<RedactionRange>, SecretScanError> {
    let mut findings = findings
        .into_iter()
        .map(|finding| normalize_finding(text, finding))
        .collect::<Result<Vec<_>, _>>()?;
    findings.sort_by(|left, right| {
        (
            left.start,
            left.end,
            left.secret_type.as_deref(),
            left.rule_id.as_str(),
        )
            .cmp(&(
                right.start,
                right.end,
                right.secret_type.as_deref(),
                right.rule_id.as_str(),
            ))
    });

    let mut merged: Vec<RedactionRange> = Vec::new();
    for finding in findings {
        if let Some(previous) = merged.last_mut()
            && finding.start < previous.end
        {
            previous.end = previous.end.max(finding.end);
            if previous.secret_type != finding.secret_type {
                previous.secret_type = None;
            }
            continue;
        }
        merged.push(RedactionRange {
            start: finding.start,
            end: finding.end,
            secret_type: finding.secret_type,
        });
    }
    Ok(merged)
}

fn normalize_finding(
    text: &str,
    mut finding: SafeFinding,
) -> Result<SafeFinding, SecretScanError> {
    if finding.start >= finding.end || finding.end > text.len() {
        return Err(SecretScanError::InvalidSpan);
    }
    while !text.is_char_boundary(finding.start) {
        finding.start -= 1;
    }
    while !text.is_char_boundary(finding.end) {
        finding.end += 1;
    }
    Ok(finding)
}

fn preserve_line_terminators(removed: &str, output: &mut String) {
    let mut bytes = removed.as_bytes().iter().copied().peekable();
    while let Some(byte) = bytes.next() {
        match byte {
            b'\r' => {
                output.push('\r');
                if bytes.next_if_eq(&b'\n').is_some() {
                    output.push('\n');
                }
            }
            b'\n' => output.push('\n'),
            _ => {}
        }
    }
}

fn redaction_marker(secret_type: Option<&str>) -> String {
    match secret_type {
        Some(secret_type) => format!("[Secret removed: {secret_type}]"),
        None => "[Secret removed]".to_string(),
    }
}

fn normalize_secret_type(description: &str, rule_id: &str) -> Option<String> {
    normalized_description(description).or_else(|| humanize_rule_id(rule_id))
}

fn normalized_description(description: &str) -> Option<String> {
    let mut label = description
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for prefix in [
        "Uncovered a possible ",
        "Uncovered an ",
        "Uncovered a ",
        "Uncovered ",
        "Identified ",
        "Detected ",
        "Discovered ",
    ] {
        if let Some(stripped) = label.strip_prefix(prefix) {
            label = stripped.to_string();
            break;
        }
    }
    for prefix in ["a pattern that may indicate ", "a possible "] {
        if let Some(stripped) = label.strip_prefix(prefix) {
            label = stripped.to_string();
        }
    }
    label.truncate(
        label
            .find(" (")
            .or_else(|| label.find(','))
            .unwrap_or(label.len()),
    );
    validate_label(label.trim_matches([' ', '.', ':', ';']))
}

fn humanize_rule_id(rule_id: &str) -> Option<String> {
    let component = rule_id.split('.').rev().find(|component| {
        !component
            .chars()
            .all(|character| character.is_ascii_digit())
    })?;
    let words = component
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(humanize_word)
        .collect::<Vec<_>>();
    validate_label(&words.join(" "))
}

fn humanize_word(word: &str) -> String {
    match word.to_ascii_lowercase().as_str() {
        "api" => "API".to_string(),
        "aws" => "AWS".to_string(),
        "github" => "GitHub".to_string(),
        "gitlab" => "GitLab".to_string(),
        "id" => "ID".to_string(),
        "oauth" => "OAuth".to_string(),
        "pat" => "Personal Access Token".to_string(),
        other => {
            let mut characters = other.chars();
            characters
                .next()
                .map(|first| {
                    first.to_ascii_uppercase().to_string()
                        + characters.as_str()
                })
                .unwrap_or_default()
        }
    }
}

fn validate_label(label: &str) -> Option<String> {
    let valid = !label.is_empty()
        && label.len() <= MAX_LABEL_LEN
        && label.split_ascii_whitespace().count() <= 10
        && label.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, ' ' | '-' | '/' | '+' | '.')
        });
    valid.then(|| label.to_string())
}

#[cfg(test)]
#[path = "../tests/crate/secret_scanning.rs"]
mod tests;
