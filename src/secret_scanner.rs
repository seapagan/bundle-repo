//! Secret Scanner module for detecting sensitive information in code and text
//!
//! This module provides functionality to:
//! - Scan content for potential secrets and sensitive information
//! - Return detailed information about found secrets
//! - Optionally redact secrets from the content
//!
//! # Example
//! ```rust
//! use secret_scanner::{scan_content, scan_content_with_redaction};
//!
//! // Basic scanning
//! let result = scan_content("api_key = 'abc123'");
//! for finding in result.findings {
//!     println!("Found secret: {}", finding.pattern_name);
//! }
//!
//! // Scanning with redaction
//! let result = scan_content_with_redaction("api_key = 'abc123'");
//! if let Some(redacted) = result.redacted_content {
//!     println!("Redacted content: {}", redacted);
//! }
//! ```

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Confidence {
    /// Definitely a secret (e.g., private key)
    High,
    /// Likely a secret (e.g., high entropy string with key-like pattern)
    Medium,
    /// Possibly a secret (e.g., password-like pattern)
    Low,
}

#[derive(Debug, Clone)]
pub struct SecretMatch {
    /// Name of the pattern that matched (e.g., "AWS Access Key")
    pub pattern_name: String,
    /// The actual text that was matched
    pub matched_text: String,
    /// Confidence level of the match
    pub confidence: Confidence,
    /// Line number where the secret was found (1-based)
    pub line_number: usize,
    /// Shannon entropy score of the matched text
    pub entropy: f64,
    /// Starting position of the match in the original text
    pub start_position: usize,
    /// Length of the matched text
    pub length: usize,
}

#[derive(Debug)]
pub struct ScanResult {
    /// Vector of all secrets found
    pub findings: Vec<SecretMatch>,
    /// Optional redacted version of the input content
    pub redacted_content: Option<String>,
}

lazy_static! {
    static ref PATTERNS: Vec<(&'static str, Regex, Confidence)> = vec![
        // AWS
        ("AWS Access Key ID", Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(), Confidence::High),
        ("AWS Secret Key", Regex::new(r#"aws[_-]?secret[_-]?access[_-]?key\s*=\s*[A-Za-z0-9/+=]{40}"#).unwrap(), Confidence::High),
        ("AWS MWS Key", Regex::new(r"amzn\.mws\.[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap(), Confidence::High),

        // Cloud Platform Keys
        ("Google API Key", Regex::new(r"AIza[0-9A-Za-z\-_]{35}").unwrap(), Confidence::High),
        ("Google OAuth", Regex::new(r"[0-9]+-[0-9A-Za-z_]{32}\.apps\.googleusercontent\.com").unwrap(), Confidence::High),
        ("Azure Storage Key", Regex::new(r"(?i)DefaultEndpointsProtocol=https;AccountName=[^;]+;AccountKey=[^;]+;EndpointSuffix=").unwrap(), Confidence::High),
        ("Firebase URL", Regex::new(r"[a-z0-9.-]+\.firebaseio\.com").unwrap(), Confidence::Medium),
        ("Heroku API Key", Regex::new(r"[hH][eE][rR][oO][kK][uU].*[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}").unwrap(), Confidence::High),

        // Platform Tokens
        ("GitHub Token", Regex::new(r"gh[pousr]_[A-Za-z0-9_]{36}").unwrap(), Confidence::High),
        ("GitHub OAuth", Regex::new(r"gho_[A-Za-z0-9_]{36}").unwrap(), Confidence::High),
        ("GitLab Token", Regex::new(r"glpat-[0-9a-zA-Z\-_]{20}").unwrap(), Confidence::High),
        ("Slack Token", Regex::new(r"xox[baprs]-[0-9]{12}-[0-9]{12}-[0-9]{12}-[a-zA-Z0-9]{32}").unwrap(), Confidence::High),
        ("Slack Webhook", Regex::new(r"https://hooks\.slack\.com/services/T[a-zA-Z0-9_]+/B[a-zA-Z0-9_]+/[a-zA-Z0-9_]+").unwrap(), Confidence::High),
        ("Discord Webhook", Regex::new(r"https://discord\.com/api/webhooks/[0-9]+/[A-Za-z0-9_-]+").unwrap(), Confidence::High),
        ("Discord Bot Token", Regex::new(r"[MN][A-Za-z\d]{23}\.[\w-]{6}\.[\w-]{27}").unwrap(), Confidence::High),

        // Payment Services
        ("Stripe API Key", Regex::new(r"(?:r|s)k_live_[0-9a-zA-Z]{24}").unwrap(), Confidence::High),
        ("Stripe Restricted Key", Regex::new(r"rk_live_[0-9a-zA-Z]{24}").unwrap(), Confidence::High),
        ("Square Access Token", Regex::new(r"sq0atp-[0-9A-Za-z\-_]{22}").unwrap(), Confidence::High),
        ("Square OAuth Secret", Regex::new(r"sq0csp-[0-9A-Za-z\-_]{43}").unwrap(), Confidence::High),
        ("PayPal Braintree Access Token", Regex::new(r"access_token\$production\$[0-9a-z]{16}\$[0-9a-f]{32}").unwrap(), Confidence::High),

        // Database
        ("MongoDB URI", Regex::new(r"mongodb(?:\+srv)?://[^:]+:[^@]+@[^/]+/[^\s]+").unwrap(), Confidence::High),
        ("PostgreSQL URI", Regex::new(r"postgres://[^:]+:[^@]+@[^/]+/[^\s]+").unwrap(), Confidence::High),
        ("MySQL URI", Regex::new(r"mysql://[^:]+:[^@]+@[^/]+/[^\s]+").unwrap(), Confidence::High),
        ("Redis URI", Regex::new(r"redis://[^:]+:[^@]+@[^/]+/[^\s]+").unwrap(), Confidence::High),

        // Private Keys & Certificates
        ("RSA Private Key", Regex::new(r"-----BEGIN RSA PRIVATE KEY-----").unwrap(), Confidence::High),
        ("SSH Private Key", Regex::new(r"-----BEGIN (?:DSA|EC|OPENSSH|RSA) PRIVATE KEY-----").unwrap(), Confidence::High),
        ("PGP Private Key", Regex::new(r"-----BEGIN PGP PRIVATE KEY BLOCK-----").unwrap(), Confidence::High),
        ("SSL Certificate", Regex::new(r"-----BEGIN CERTIFICATE-----").unwrap(), Confidence::Medium),

        // Authentication & Tokens
        ("JWT", Regex::new(r"eyJ[A-Za-z0-9-_=]+\.[A-Za-z0-9-_=]+\.?[A-Za-z0-9-_.+/=]*").unwrap(), Confidence::Medium),
        ("Basic Auth", Regex::new(r"basic [a-zA-Z0-9_\-:.=]+").unwrap(), Confidence::Medium),
        ("Bearer Token", Regex::new(r"bearer [a-zA-Z0-9_\-.=]+").unwrap(), Confidence::Medium),
        ("Authorization Header", Regex::new(r"authorization:\s*[a-zA-Z0-9_\-.=]+").unwrap(), Confidence::Medium),

        // Social Media & Communication
        ("Twitter Access Token", Regex::new(r"[1-9][0-9]+-[0-9a-zA-Z]{40}").unwrap(), Confidence::High),
        ("Facebook Access Token", Regex::new(r"EAACEdEose0cBA[0-9A-Za-z]+").unwrap(), Confidence::High),
        ("Instagram Basic Display API", Regex::new(r"IG[TD]_[0-9a-zA-Z]{120,200}").unwrap(), Confidence::High),
        ("Twilio API Key", Regex::new(r"SK[0-9a-fA-F]{32}").unwrap(), Confidence::High),
        ("Mailgun API Key", Regex::new(r"key-[0-9a-zA-Z]{32}").unwrap(), Confidence::High),
        ("Mailchimp API Key", Regex::new(r"[0-9a-f]{32}-us[0-9]{1,2}").unwrap(), Confidence::High),
        ("SendGrid API Key", Regex::new(r"SG\.[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+").unwrap(), Confidence::High),

        // Generic Patterns
        ("Password in Assignment", Regex::new(r#"(?i)password\s*=\s*['"'][^'"]{8,}['"']"#).unwrap(), Confidence::Low),
        ("Secret in Assignment", Regex::new(r#"(?i)secret\s*=\s*['"'][^'"]{8,}['"']"#).unwrap(), Confidence::Low),
        ("API Key Assignment", Regex::new(r#"(?i)api[_-]?key\s*=\s*['"'][^'"]{8,}['"']"#).unwrap(), Confidence::Medium),
        ("Token Assignment", Regex::new(r#"(?i)token\s*=\s*['"'][^'"]{8,}['"']"#).unwrap(), Confidence::Low),
        ("Generic High-Entropy String", Regex::new(r"[A-Za-z0-9+/]{32,}={0,2}").unwrap(), Confidence::Low),
        ("Simple Key Assignment", Regex::new(r#"(?i)key[0-9]*\s*=\s*[a-zA-Z0-9]+[0-9]+"#).unwrap(), Confidence::Low),

        // Environment Variables
        ("Sensitive Env Var", Regex::new(r#"(?i)(?:PASS|SECRET|TOKEN|KEY|AUTH|SIGN|ADMIN).*=\s*['"'][^'"]{8,}['"']"#).unwrap(), Confidence::Medium),

        // Cloud Service Configuration
        ("S3 Bucket Config", Regex::new(r"(?i)s3://[a-z0-9.-]+/[^\s]*").unwrap(), Confidence::Low),
        ("Google Cloud Storage", Regex::new(r"(?i)gs://[a-z0-9.-]+/[^\s]*").unwrap(), Confidence::Low),
        ("Azure Blob Storage", Regex::new(r"(?i)DefaultEndpointsProtocol=https;AccountName=[^;]+;AccountKey=[^;]+").unwrap(), Confidence::High),

        // NPM Package Registry
        ("NPM Token", Regex::new(r"npm_[A-Za-z0-9]{36}").unwrap(), Confidence::High),

        // Cryptocurrency
        ("Bitcoin Address", Regex::new(r"[13][a-km-zA-HJ-NP-Z1-9]{25,34}").unwrap(), Confidence::Medium),
        ("Ethereum Address", Regex::new(r"0x[a-fA-F0-9]{40}").unwrap(), Confidence::Medium),

        // Additional Cloud Services
        ("DigitalOcean Token", Regex::new(r"do[p|at]_[a-zA-Z0-9]{64}").unwrap(), Confidence::High),
        ("Alibaba Access Key", Regex::new(r"LTAI[a-zA-Z0-9]{20}").unwrap(), Confidence::High),
    ];

    static ref ALLOWLIST: HashSet<&'static str> = {
        let mut set = HashSet::new();
        set.insert("AKIAIOSFODNN7EXAMPLE");
        set.insert("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY");
        set.insert("sk_live_example_key123456");
        set.insert("test_token");
        set.insert("example_secret");
        set.insert("0x0000000000000000000000000000000000000000"); // Common Ethereum zero address
        set
    };
}

#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// Minimum entropy threshold for medium/low confidence matches
    pub min_entropy: f64,
    /// Maximum line length to process
    pub max_line_length: usize,
    /// Whether to ignore commented lines
    pub ignore_comments: bool,
    /// Whether to enable redaction
    pub enable_redaction: bool,
    /// Character to use for redaction (default: 'x')
    pub redaction_char: char,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            min_entropy: 3.5,
            max_line_length: 500,
            ignore_comments: true,
            enable_redaction: false,
            redaction_char: 'x',
        }
    }
}

impl ScannerConfig {
    /// Create a new scanner configuration with custom settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the minimum entropy threshold
    pub fn min_entropy(mut self, value: f64) -> Self {
        self.min_entropy = value;
        self
    }

    /// Set the maximum line length
    pub fn max_line_length(mut self, value: usize) -> Self {
        self.max_line_length = value;
        self
    }

    /// Set whether to ignore comments
    pub fn ignore_comments(mut self, value: bool) -> Self {
        self.ignore_comments = value;
        self
    }

    /// Enable or disable redaction
    pub fn enable_redaction(mut self, value: bool) -> Self {
        self.enable_redaction = value;
        self
    }

    /// Set the character to use for redaction
    pub fn redaction_char(mut self, value: char) -> Self {
        self.redaction_char = value;
        self
    }
}

struct SecretScanner {
    config: ScannerConfig,
}

impl SecretScanner {
    fn new(config: ScannerConfig) -> Self {
        Self { config }
    }

    fn scan_content(&self, content: &str) -> ScanResult {
        let mut findings = Vec::new();
        let mut redacted_content = if self.config.enable_redaction {
            Some(String::new())
        } else {
            None
        };

        for (line_number, line) in content.lines().enumerate() {
            if line.len() > self.config.max_line_length {
                if let Some(ref mut redacted) = redacted_content {
                    redacted.push_str(line);
                    redacted.push('\n');
                }
                continue;
            }

            if self.config.ignore_comments
                && (line.trim_start().starts_with("//")
                    || line.trim_start().starts_with("#")
                    || line.trim_start().starts_with("/*"))
            {
                if let Some(ref mut redacted) = redacted_content {
                    redacted.push_str(line);
                    redacted.push('\n');
                }
                continue;
            }

            let mut line_findings = Vec::new();

            for (pattern_name, regex, confidence) in PATTERNS.iter() {
                for matched in regex.find_iter(line) {
                    let matched_text = matched.as_str().to_string();

                    if ALLOWLIST.contains(matched_text.as_str()) {
                        continue;
                    }

                    let entropy = self.calculate_entropy(&matched_text);

                    if *confidence != Confidence::High
                        && entropy < self.config.min_entropy
                    {
                        continue;
                    }

                    // Skip low confidence matches if we already have a higher confidence match for this region
                    if *confidence == Confidence::Low {
                        let overlaps_with_higher_confidence = line_findings
                            .iter()
                            .any(|existing: &SecretMatch| {
                                let existing_range = existing.start_position
                                    ..(existing.start_position
                                        + existing.length);
                                let current_range =
                                    matched.start()..matched.end();
                                existing.confidence != Confidence::Low
                                    && (existing_range
                                        .contains(&current_range.start)
                                        || existing_range.contains(
                                            &(current_range.end - 1),
                                        ))
                            });
                        if overlaps_with_higher_confidence {
                            continue;
                        }
                    }

                    if matched.start() <= matched.end() {
                        // Debug print the positions
                        println!(
                            "Adding match with start: {}, end: {}",
                            matched.start(),
                            matched.end()
                        );

                        line_findings.push(SecretMatch {
                            pattern_name: pattern_name.to_string(),
                            matched_text,
                            confidence: confidence.clone(),
                            line_number: line_number + 1,
                            entropy,
                            start_position: matched.start(),
                            length: matched.end() - matched.start(),
                        });
                    } else {
                        // Debug print when the condition fails
                        println!(
                            "Skipping invalid match with start: {}, end: {}",
                            matched.start(),
                            matched.end()
                        );
                    }
                }
            }

            if let Some(ref mut redacted) = redacted_content {
                if !line_findings.is_empty() {
                    // Sort findings by start position
                    line_findings.sort_by_key(|f| f.start_position);

                    let mut current_pos = 0;
                    for finding in &line_findings {
                        // Process only if current_pos <= start_position to avoid redundant processing
                        if current_pos <= finding.start_position {
                            redacted.push_str(
                                &line[current_pos..finding.start_position],
                            );

                            // Add redaction based on redaction_char and finding length
                            redacted.push_str(
                                &self
                                    .config
                                    .redaction_char
                                    .to_string()
                                    .repeat(finding.length),
                            );

                            // Update current position to the end of this finding
                            current_pos =
                                finding.start_position + finding.length;
                        } else {
                            println!(
            "Skipping invalid or redundant range: current_pos={}, start_position={}",
            current_pos, finding.start_position
        );
                        }
                    }

                    // After the loop, add any remaining text from current_pos to the end of the line
                    if current_pos < line.len() {
                        redacted.push_str(&line[current_pos..]);
                    }
                } else {
                    redacted.push_str(line);
                }
                redacted.push('\n');
            }

            findings.extend(line_findings);
        }

        // Handle private keys that span multiple lines
        if content.contains("-----BEGIN") && content.contains("PRIVATE KEY") {
            let start_pos = content.find("-----BEGIN").unwrap();
            let line_number = content[..start_pos].matches('\n').count() + 1;

            findings.push(SecretMatch {
                pattern_name: "RSA Private Key".to_string(),
                matched_text: content[start_pos..]
                    .lines()
                    .take_while(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
                confidence: Confidence::High,
                line_number,
                entropy: 4.0, // Private keys typically have high entropy
                start_position: start_pos,
                length: content[start_pos..]
                    .lines()
                    .take_while(|line| !line.is_empty())
                    .collect::<String>()
                    .len(),
            });

            if let Some(ref mut redacted) = redacted_content {
                // Remove final newline if original content didn't have one
                if !content.ends_with('\n') && redacted.ends_with('\n') {
                    redacted.pop();
                }
            }
        }

        ScanResult {
            findings,
            redacted_content,
        }
    }

    fn calculate_entropy(&self, text: &str) -> f64 {
        // ... (unchanged)
        let text_bytes = text.as_bytes();
        let text_length = text_bytes.len() as f64;

        if text_length == 0.0 {
            return 0.0;
        }

        let mut frequency = [0.0; 256];

        for &byte in text_bytes {
            frequency[byte as usize] += 1.0;
        }

        let mut entropy = 0.0;
        for &freq in &frequency {
            if freq > 0.0 {
                let probability = freq / text_length;
                entropy -= probability * probability.log2();
            }
        }

        entropy
    }
}

// Public API functions
/// Scan content for potential secrets with default configuration
pub fn scan_content(content: &str) -> ScanResult {
    let config = ScannerConfig::default();
    let scanner = SecretScanner::new(config);
    scanner.scan_content(content)
}

/// Scan content for potential secrets with redaction enabled
pub fn scan_content_with_redaction(content: &str) -> ScanResult {
    let config = ScannerConfig::default().enable_redaction(true);
    let scanner = SecretScanner::new(config);
    scanner.scan_content(content)
}

/// Scan content for potential secrets with custom configuration
pub fn scan_content_with_config(
    content: &str,
    config: ScannerConfig,
) -> ScanResult {
    let scanner = SecretScanner::new(config);
    scanner.scan_content(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redaction() {
        let content = "aws_secret_access_key = AKIAIOSFODNN7EXAMPLE";
        let result = scan_content_with_redaction(content);

        assert!(result.redacted_content.is_some());
        let redacted = result.redacted_content.unwrap();
        println!("Redacted content: {}", redacted);
        assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_multiple_secrets_same_line() {
        let content = "AWS_KEY=AKIA1234567890ABCDEF STRIPE_KEY=sk_live_1234567890abcdefghijklmn";
        let result = scan_content_with_redaction(content);

        println!("Findings: {:?}", result.findings);

        if let Some(redacted) = result.redacted_content {
            println!("Redacted content: {}", redacted);

            assert!(!redacted.contains("AKIA1234567890ABCDEF"));
            assert!(!redacted.contains("sk_live_1234567890abcdefghijklmn"));
            assert_eq!(
                result.findings.len(),
                2,
                "Expected 2 findings, got {}",
                result.findings.len()
            );
        } else {
            panic!("Expected redacted content, got None");
        }
    }

    #[test]
    fn test_basic_private_key() {
        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
        let result = scan_content_with_redaction(content);

        assert!(result.findings.len() > 0, "Should detect private key");
        if let Some(redacted) = result.redacted_content {
            println!("Redacted content: {}", redacted);
            assert!(!redacted.contains("MIIEpQIBAAKCAQEA"));
        } else {
            panic!("Expected redacted content, got None");
        }
    }
}
