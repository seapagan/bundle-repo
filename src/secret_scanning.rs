use secrets_scanner::{RedactionMode, ScanConfig, Scanner};
use std::error::Error;
use std::fmt;

const CONTENT_SCAN_PATH: &str = "repository-content";
const MAX_LABEL_LEN: usize = 80;

#[cfg(test)]
pub(crate) fn synthetic_github_pat() -> String {
    ["ghp_", "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8"].concat()
}

pub(crate) struct SecretScanner {
    scanner: Scanner,
}

pub(crate) struct SecretRedaction {
    pub(crate) text: String,
    pub(crate) findings: usize,
}

#[derive(Debug)]
pub(crate) enum SecretScanError {
    Setup(secrets_scanner::ScannerError),
    InvalidSpan,
    TruncatedFindings,
}

impl fmt::Display for SecretScanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(error) => {
                write!(formatter, "failed to load bundled rules: {error}")
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
            Self::Setup(error) => Some(error),
            Self::InvalidSpan | Self::TruncatedFindings => None,
        }
    }
}

impl SecretScanner {
    pub(crate) fn from_bundled() -> Result<Self, SecretScanError> {
        let config = ScanConfig {
            redact: true,
            redaction_mode: RedactionMode::Full,
            honor_allow_markers: false,
            capture_context: false,
            max_findings: None,
            max_findings_per_file: None,
            max_file_size: u64::MAX,
            ..ScanConfig::default()
        };
        let scanner = Scanner::from_bundled()
            .map_err(SecretScanError::Setup)?
            .with_config(config);
        Ok(Self { scanner })
    }

    pub(crate) fn redact_text(
        &self,
        text: &str,
    ) -> Result<SecretRedaction, SecretScanError> {
        let result =
            self.scanner.scan_content_detailed(CONTENT_SCAN_PATH, text);
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
        let text = redact_findings(text, spans)?;
        Ok(SecretRedaction { text, findings })
    }

    #[cfg(test)]
    fn rule_ids(&self, text: &str) -> Vec<String> {
        self.scanner
            .scan_content_detailed(CONTENT_SCAN_PATH, text)
            .findings
            .into_iter()
            .map(|finding| finding.rule_id)
            .collect()
    }
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

fn redact_findings(
    text: &str,
    findings: Vec<SafeFinding>,
) -> Result<String, SecretScanError> {
    let ranges = normalized_ranges(text, findings)?;
    if ranges.is_empty() {
        return Ok(text.to_string());
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
    Ok(output)
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
