use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfigSourceIdentity {
    Global,
    RepositoryLocal,
}

impl fmt::Display for ConfigSourceIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Global => "global BundleRepo configuration",
            Self::RepositoryLocal => {
                "repository-local .bundlerepo.toml configuration"
            }
        })
    }
}

#[derive(Debug)]
pub(crate) enum MetadataError {
    Secret {
        identity: ConfigSourceIdentity,
        line: usize,
        key: Option<String>,
    },
    InvalidEntry {
        identity: ConfigSourceIdentity,
        line: usize,
        key: Option<String>,
        reason: String,
    },
    Scanner {
        identity: ConfigSourceIdentity,
        line: usize,
    },
    Parse {
        identity: ConfigSourceIdentity,
        line: usize,
        column: usize,
        message: String,
    },
    InvalidType {
        identity: ConfigSourceIdentity,
        line: usize,
        key: Option<String>,
        category: &'static str,
    },
    InvalidRootType {
        identity: ConfigSourceIdentity,
        line: usize,
        category: &'static str,
    },
}

impl fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Secret {
                identity,
                line,
                key,
            } => format_secret_error(formatter, *identity, *line, key),
            Self::InvalidEntry {
                identity,
                line,
                key,
                reason,
            } => format_invalid_entry_error(
                formatter, *identity, *line, key, reason,
            ),
            Self::Scanner { identity, line } => write!(
                formatter,
                "metadata secret scan failed in {identity} at line {line}"
            ),
            Self::Parse {
                identity,
                line,
                column,
                message,
            } => write!(
                formatter,
                "Invalid metadata in {identity} at line {line}, column {column}: {message}"
            ),
            Self::InvalidType {
                identity,
                line,
                key,
                category,
            } => format_invalid_type_error(
                formatter, *identity, *line, key, category,
            ),
            Self::InvalidRootType {
                identity,
                line,
                category,
            } => write!(
                formatter,
                "The metadata value in {identity} at line {line} must be a table, not {category}"
            ),
        }
    }
}

fn format_secret_error(
    formatter: &mut fmt::Formatter<'_>,
    identity: ConfigSourceIdentity,
    line: usize,
    key: &Option<String>,
) -> fmt::Result {
    write!(formatter, "Detected a secret in metadata")?;
    if let Some(key) = key {
        write!(formatter, " entry '{key}'")?;
    } else {
        formatter.write_str(" key")?;
    }
    write!(
        formatter,
        " in {identity} at line {line}.\nIf you believe this detection is a false positive, please report it at https://github.com/seapagan/bundle-repo/issues."
    )
}

fn format_invalid_entry_error(
    formatter: &mut fmt::Formatter<'_>,
    identity: ConfigSourceIdentity,
    line: usize,
    key: &Option<String>,
    reason: &str,
) -> fmt::Result {
    formatter.write_str("Invalid metadata")?;
    if let Some(key) = key {
        write!(formatter, " entry '{key}'")?;
    }
    write!(formatter, " in {identity} at line {line}: {reason}")
}

fn format_invalid_type_error(
    formatter: &mut fmt::Formatter<'_>,
    identity: ConfigSourceIdentity,
    line: usize,
    key: &Option<String>,
    category: &str,
) -> fmt::Result {
    formatter.write_str("Metadata entry")?;
    if let Some(key) = key {
        write!(formatter, " '{key}'")?;
    }
    write!(
        formatter,
        " in {identity} at line {line} must be a string, not {category}"
    )
}

impl std::error::Error for MetadataError {}
