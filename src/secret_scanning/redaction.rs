use super::SecretScanError;

const MAX_LABEL_LEN: usize = 80;

pub(super) struct SafeFinding {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) secret_type: Option<String>,
    pub(super) rule_id: String,
}

pub(super) struct RedactionRange {
    start: usize,
    end: usize,
    secret_type: Option<String>,
}

#[cfg(test)]
pub(super) fn redact_findings(
    text: &str,
    findings: Vec<SafeFinding>,
) -> Result<String, SecretScanError> {
    let ranges = normalized_ranges(text, findings)?;
    Ok(redact_ranges(text, ranges))
}

pub(super) fn redact_ranges(
    text: &str,
    ranges: Vec<RedactionRange>,
) -> String {
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

pub(super) fn common_secret_type(ranges: &[RedactionRange]) -> Option<String> {
    let first = ranges.first()?.secret_type.as_ref()?;
    ranges
        .iter()
        .all(|range| range.secret_type.as_ref() == Some(first))
        .then(|| first.clone())
}

pub(super) fn normalized_ranges(
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

pub(super) fn normalize_secret_type(
    description: &str,
    rule_id: &str,
) -> Option<String> {
    normalized_description(description).or_else(|| humanize_rule_id(rule_id))
}

fn normalized_description(description: &str) -> Option<String> {
    let mut label = description
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for prefix in [
        "Identified a potential ",
        "Found a pattern resembling a ",
        "Found an ",
        "Found a ",
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
