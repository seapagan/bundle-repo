use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use config::{Config, File, FileFormat};
use dirs_next::home_dir;
use toml::de::{DeTable, DeValue};
use toml_parser::Source;
use toml_parser::parser::{Event, EventKind, RecursionGuard, parse_document};

use crate::secret_scanning::{MetadataPairSecret, SecretScanner};
use crate::structs::Params;
use crate::xml_output::{
    first_invalid_xml_attribute_char, first_invalid_xml10_char,
};

pub(crate) struct LoadedConfig {
    pub(crate) params: Params,
    pub(crate) metadata_sources: Vec<MetadataSource>,
    pub(crate) legacy_error: Option<String>,
}

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

struct ConfigSource {
    path: PathBuf,
    identity: ConfigSourceIdentity,
}

pub(crate) struct MetadataSource {
    pub(crate) identity: ConfigSourceIdentity,
    pub(crate) entries: Vec<MetadataCandidate>,
}

pub(crate) struct MetadataCandidate {
    pub(crate) key: String,
    pub(crate) key_line: usize,
    pub(crate) value_line: usize,
    pub(crate) value: CandidateValue,
}

pub(crate) enum CandidateValue {
    String(String),
    InvalidType(&'static str),
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
            } => {
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
            Self::InvalidEntry {
                identity,
                line,
                key,
                reason,
            } => {
                formatter.write_str("Invalid metadata")?;
                if let Some(key) = key {
                    write!(formatter, " entry '{key}'")?;
                }
                write!(formatter, " in {identity} at line {line}: {reason}")
            }
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
            } => {
                formatter.write_str("Metadata entry")?;
                if let Some(key) = key {
                    write!(formatter, " '{key}'")?;
                }
                write!(
                    formatter,
                    " in {identity} at line {line} must be a string, not {category}"
                )
            }
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

impl std::error::Error for MetadataError {}

pub(crate) fn validate_and_merge_metadata(
    sources: &[MetadataSource],
    scanner: &SecretScanner,
) -> Result<BTreeMap<String, String>, MetadataError> {
    for source in sources {
        for entry in &source.entries {
            entry.validate(source.identity, scanner)?;
        }
    }
    let mut metadata = BTreeMap::new();
    for source in sources {
        for entry in &source.entries {
            metadata.insert(
                entry.key.clone(),
                entry.string_value(source.identity)?.to_string(),
            );
        }
    }
    metadata.retain(|_, value| !value.trim().is_empty());
    Ok(metadata)
}

impl MetadataCandidate {
    fn validate(
        &self,
        identity: ConfigSourceIdentity,
        scanner: &SecretScanner,
    ) -> Result<(), MetadataError> {
        scan_metadata_text(scanner, &self.key, identity, self.key_line, None)?;
        if self.key.trim().is_empty() {
            return Err(MetadataError::InvalidEntry {
                identity,
                line: self.key_line,
                key: None,
                reason: "metadata key must not be empty or whitespace-only"
                    .to_string(),
            });
        }
        if let Some(invalid) = first_invalid_xml_attribute_char(&self.key) {
            return Err(self.xml_error(
                identity,
                self.key_line,
                invalid,
                "key",
            ));
        }
        let value = self.string_value(identity).map_err(|mut error| {
            if let MetadataError::InvalidType { key, .. } = &mut error {
                *key = Some(self.key.clone());
            }
            error
        })?;
        scan_metadata_pair(scanner, self, value, identity)?;
        scan_metadata_text(
            scanner,
            value,
            identity,
            self.value_line,
            Some(&self.key),
        )?;
        if let Some(invalid) = first_invalid_xml10_char(value) {
            return Err(self.xml_error(
                identity,
                self.value_line,
                invalid,
                "value",
            ));
        }
        Ok(())
    }

    fn xml_error(
        &self,
        identity: ConfigSourceIdentity,
        line: usize,
        invalid: crate::xml_output::InvalidXml10Char,
        role: &str,
    ) -> MetadataError {
        MetadataError::InvalidEntry {
            identity,
            line,
            key: Some(self.key.clone()),
            reason: format!(
                "metadata {role} contains U+{:04X} at byte index {}, which cannot round-trip through XML 1.0",
                invalid.character as u32, invalid.byte_index
            ),
        }
    }

    pub(crate) fn string_value(
        &self,
        identity: ConfigSourceIdentity,
    ) -> Result<&str, MetadataError> {
        match &self.value {
            CandidateValue::String(value) => Ok(value),
            CandidateValue::InvalidType(category) => {
                Err(MetadataError::InvalidType {
                    identity,
                    line: self.value_line,
                    key: None,
                    category,
                })
            }
        }
    }
}

fn scan_metadata_pair(
    scanner: &SecretScanner,
    entry: &MetadataCandidate,
    value: &str,
    identity: ConfigSourceIdentity,
) -> Result<(), MetadataError> {
    let finding = scanner
        .contains_metadata_pair_secret(&entry.key, value)
        .map_err(|_| MetadataError::Scanner {
            identity,
            line: entry.value_line,
        })?;
    match finding {
        MetadataPairSecret::Clean => Ok(()),
        MetadataPairSecret::Value => Err(MetadataError::Secret {
            identity,
            line: entry.value_line,
            key: Some(entry.key.clone()),
        }),
        MetadataPairSecret::Unsafe => Err(MetadataError::Secret {
            identity,
            line: entry.key_line,
            key: None,
        }),
    }
}

fn scan_metadata_text(
    scanner: &SecretScanner,
    text: &str,
    identity: ConfigSourceIdentity,
    line: usize,
    safe_key: Option<&str>,
) -> Result<(), MetadataError> {
    if scanner
        .contains_secret(text)
        .map_err(|_| MetadataError::Scanner { identity, line })?
    {
        return Err(MetadataError::Secret {
            identity,
            line,
            key: safe_key.map(str::to_string),
        });
    }
    Ok(())
}

pub(crate) fn load_config() -> Result<LoadedConfig, MetadataError> {
    let global =
        home_dir().map(|home| home.join(".config/bundlerepo/config.toml"));
    load_config_from_paths(global.as_deref(), Path::new(".bundlerepo.toml"))
}

pub(crate) fn load_config_from_paths(
    global: Option<&Path>,
    local: &Path,
) -> Result<LoadedConfig, MetadataError> {
    let sources = config_sources(global, local);
    let metadata_sources = sources
        .iter()
        .map(parse_metadata_source)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();
    let mut builder = Config::builder();

    for source in &sources {
        builder = builder.add_source(
            File::from(source.path.clone()).format(FileFormat::Toml),
        );
    }

    let (params, legacy_error) = match builder.build() {
        Ok(config) => (config.into(), None),
        Err(error) => (Params::default(), Some(error.to_string())),
    };

    Ok(LoadedConfig {
        params,
        metadata_sources,
        legacy_error,
    })
}

fn config_sources(global: Option<&Path>, local: &Path) -> Vec<ConfigSource> {
    let mut sources = Vec::with_capacity(2);
    if let Some(path) = global.filter(|path| path.exists()) {
        sources.push(ConfigSource {
            path: path.to_path_buf(),
            identity: ConfigSourceIdentity::Global,
        });
    }
    if local.exists() {
        sources.push(ConfigSource {
            path: local.to_path_buf(),
            identity: ConfigSourceIdentity::RepositoryLocal,
        });
    }
    sources
}

fn parse_metadata_source(
    source: &ConfigSource,
) -> Result<Option<MetadataSource>, MetadataError> {
    let Ok(input) = fs::read_to_string(&source.path) else {
        return Ok(None);
    };
    parse_metadata(&input, source.identity)
}

fn parse_metadata(
    input: &str,
    identity: ConfigSourceIdentity,
) -> Result<Option<MetadataSource>, MetadataError> {
    let (document, errors) = DeTable::parse_recoverable(input);
    check_metadata_parse_errors(input, identity, &errors)?;
    let table = document.get_ref();
    let Some((_, metadata_value)) = table
        .iter()
        .find(|(key, _)| key.get_ref().as_ref() == "metadata")
    else {
        return Ok(None);
    };
    let DeValue::Table(entries) = metadata_value.get_ref() else {
        let (line, _) = source_location(input, metadata_value.span().start);
        return Err(MetadataError::InvalidRootType {
            identity,
            line,
            category: value_category(metadata_value.get_ref()),
        });
    };

    let entries = entries
        .iter()
        .map(|(key, value)| MetadataCandidate {
            key: key.get_ref().to_string(),
            key_line: source_location(input, key.span().start).0,
            value_line: source_location(input, value.span().start).0,
            value: candidate_value(value.get_ref()),
        })
        .collect();
    Ok(Some(MetadataSource { identity, entries }))
}

fn check_metadata_parse_errors(
    input: &str,
    identity: ConfigSourceIdentity,
    errors: &[toml::de::Error],
) -> Result<(), MetadataError> {
    if errors.is_empty() {
        return Ok(());
    }
    let regions = metadata_syntax_regions(input);
    for error in errors {
        let Some(span) = error.span() else {
            continue;
        };
        let owns_error = regions
            .iter()
            .rfind(|(start, _)| *start <= span.start)
            .is_some_and(|(_, metadata)| *metadata);
        if owns_error {
            let (line, column) = source_location(input, span.start);
            return Err(MetadataError::Parse {
                identity,
                line,
                column,
                message: error.message().to_string(),
            });
        }
    }
    Ok(())
}

fn metadata_syntax_regions(input: &str) -> Vec<(usize, bool)> {
    let source = Source::new(input);
    let tokens = source.lex().collect::<Vec<_>>();
    let mut regions = vec![(0, false)];
    let (mut root, mut first_key, mut header) = (true, true, false);
    let mut depth: usize = 0;
    let mut receiver = |event: Event| match event.kind() {
        EventKind::StdTableOpen | EventKind::ArrayTableOpen => {
            root = false;
            first_key = true;
            header = true;
            regions.push((event.span().start(), false));
        }
        EventKind::SimpleKey if depth == 0 && first_key => {
            first_key = false;
            let metadata = is_metadata_key(source, event);
            if header {
                regions.last_mut().unwrap().1 = metadata;
                header = false;
            } else if root {
                regions.push((event.span().start(), metadata));
            }
        }
        EventKind::InlineTableOpen | EventKind::ArrayOpen => depth += 1,
        EventKind::InlineTableClose | EventKind::ArrayClose => {
            depth = depth.saturating_sub(1);
        }
        EventKind::Newline if depth == 0 => {
            first_key = root;
            header = false;
            if root {
                regions.push((event.span().end(), false));
            }
        }
        _ => {}
    };
    // Match toml's nesting limit while inspecting its physical syntax.
    let mut receiver = RecursionGuard::new(&mut receiver, 80);
    parse_document(&tokens, &mut receiver, &mut ());
    regions
}

fn is_metadata_key(source: Source<'_>, event: Event) -> bool {
    let Some(raw) = source.get(event) else {
        return false;
    };
    let mut key = String::new();
    let mut errors = Vec::new();
    raw.decode_key(&mut key, &mut errors);
    errors.is_empty() && key == "metadata"
}

fn candidate_value(value: &DeValue<'_>) -> CandidateValue {
    match value {
        DeValue::String(value) => CandidateValue::String(value.to_string()),
        value => CandidateValue::InvalidType(value_category(value)),
    }
}

fn value_category(value: &DeValue<'_>) -> &'static str {
    match value {
        DeValue::String(_) => "string",
        DeValue::Integer(_) => "integer",
        DeValue::Float(_) => "float",
        DeValue::Boolean(_) => "boolean",
        DeValue::Datetime(_) => "datetime",
        DeValue::Array(_) => "array",
        DeValue::Table(_) => "table",
    }
}

fn source_location(input: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(input.len());
    while !input.is_char_boundary(offset) {
        offset -= 1;
    }
    let prefix = &input[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current)| current)
        .chars()
        .count()
        + 1;
    (line, column)
}

#[cfg(test)]
#[path = "../tests/crate/configuration.rs"]
mod tests;
