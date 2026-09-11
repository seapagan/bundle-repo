use crate::cli;
use config::Config;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug)]
pub enum ConfigError {
    Missing(String),
    TypeError { key: String, message: String },
    Other(config::ConfigError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(key) => {
                write!(f, "Missing TOML value for key: {}", key)
            }
            ConfigError::TypeError { key, message } => {
                write!(f, "Type error for key {}: {}", key, message)
            }
            ConfigError::Other(e) => write!(f, "Config error: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<config::ConfigError> for ConfigError {
    fn from(error: config::ConfigError) -> Self {
        match error {
            e @ config::ConfigError::NotFound(_) => {
                ConfigError::Missing(e.to_string())
            }
            e if e.to_string().contains("invalid type") => {
                ConfigError::TypeError {
                    key: "unknown".to_string(),
                    message: e.to_string(),
                }
            }
            e => ConfigError::Other(e),
        }
    }
}

pub trait TomlValue: Sized {
    const TYPE_NAME: &'static str;

    fn load_from_config(
        config: &Config,
        key: &str,
    ) -> Result<Self, ConfigError>;
}

impl TomlValue for String {
    const TYPE_NAME: &'static str = "string";

    fn load_from_config(
        config: &Config,
        key: &str,
    ) -> Result<Self, ConfigError> {
        config.get_string(key).map_err(|e| {
            if matches!(e, config::ConfigError::NotFound(_)) {
                ConfigError::Missing(key.to_string())
            } else {
                ConfigError::TypeError {
                    key: key.to_string(),
                    message: format!(
                        "Expected {}, got invalid type",
                        Self::TYPE_NAME
                    ),
                }
            }
        })
    }
}

impl TomlValue for bool {
    const TYPE_NAME: &'static str = "boolean";

    fn load_from_config(
        config: &Config,
        key: &str,
    ) -> Result<Self, ConfigError> {
        config.get_bool(key).map_err(|e| {
            if matches!(e, config::ConfigError::NotFound(_)) {
                ConfigError::Missing(key.to_string())
            } else {
                ConfigError::TypeError {
                    key: key.to_string(),
                    message: format!(
                        "Expected {}, got invalid type",
                        Self::TYPE_NAME
                    ),
                }
            }
        })
    }
}

impl TomlValue for i64 {
    const TYPE_NAME: &'static str = "integer";

    fn load_from_config(
        config: &Config,
        key: &str,
    ) -> Result<Self, ConfigError> {
        config.get_int(key).map_err(|e| {
            if matches!(e, config::ConfigError::NotFound(_)) {
                ConfigError::Missing(key.to_string())
            } else {
                ConfigError::TypeError {
                    key: key.to_string(),
                    message: format!(
                        "Expected {}, got invalid type",
                        Self::TYPE_NAME
                    ),
                }
            }
        })
    }
}

impl TomlValue for f64 {
    const TYPE_NAME: &'static str = "float";

    fn load_from_config(
        config: &Config,
        key: &str,
    ) -> Result<Self, ConfigError> {
        config.get_float(key).map_err(|e| {
            if matches!(e, config::ConfigError::NotFound(_)) {
                ConfigError::Missing(key.to_string())
            } else {
                ConfigError::TypeError {
                    key: key.to_string(),
                    message: format!(
                        "Expected {}, got invalid type",
                        Self::TYPE_NAME
                    ),
                }
            }
        })
    }
}

impl<T: TomlValue> TomlValue for Option<T> {
    const TYPE_NAME: &'static str = "optional value";

    fn load_from_config(
        config: &Config,
        key: &str,
    ) -> Result<Self, ConfigError> {
        match T::load_from_config(config, key) {
            Ok(value) => Ok(Some(value)),
            Err(ConfigError::Missing(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl<T: TomlValue> TomlValue for Vec<T> {
    const TYPE_NAME: &'static str = "array";

    fn load_from_config(
        config: &Config,
        key: &str,
    ) -> Result<Self, ConfigError> {
        config
            .get_array(key)
            .map_err(|e| {
                if matches!(e, config::ConfigError::NotFound(_)) {
                    ConfigError::Missing(key.to_string())
                } else {
                    ConfigError::TypeError {
                        key: key.to_string(),
                        message: format!(
                            "Expected {}, got invalid type",
                            Self::TYPE_NAME
                        ),
                    }
                }
            })?
            .into_iter()
            .enumerate()
            .map(|(i, _)| {
                let key = format!("{}[{}]", key, i);
                T::load_from_config(config, &key)
            })
            .collect()
    }
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Params {
    pub output_file: Option<String>,
    pub stdout: bool,
    pub model: Option<String>,
    pub clipboard: bool,
    pub line_numbers: bool,
    pub token: Option<String>,
    pub branch: Option<String>,
    pub extend_exclude: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub include: Option<Vec<String>>,
    pub legacy_excludes: bool,
    pub utf8: bool,
    pub gzip: bool,
    pub gzip_level: u32,
    pub secret_scan: bool,
    pub metadata: BTreeMap<String, String>,
}

pub const DEFAULT_OUTPUT_FILE: &str = "packed-repo.xml";
pub const DEFAULT_MODEL: &str = "gpt5";

impl Default for Params {
    fn default() -> Self {
        Params {
            output_file: Some(DEFAULT_OUTPUT_FILE.to_string()),
            stdout: false,
            model: Some(DEFAULT_MODEL.to_string()),
            clipboard: false,
            line_numbers: false,
            token: None,
            branch: None,
            extend_exclude: None,
            exclude: None,
            include: None,
            legacy_excludes: false,
            utf8: false,
            gzip: false,
            gzip_level: 6,
            secret_scan: true,
            metadata: BTreeMap::new(),
        }
    }
}

impl From<Config> for Params {
    fn from(settings: Config) -> Self {
        let mut params = Params::default();
        params.output_file = configured_optional_or(
            &settings,
            "output_file",
            params.output_file,
        );
        params.stdout = configured_or(&settings, "stdout", params.stdout);
        params.model =
            configured_optional_or(&settings, "model", params.model);
        params.clipboard =
            configured_or(&settings, "clipboard", params.clipboard);
        params.line_numbers =
            configured_or(&settings, "line_numbers", params.line_numbers);
        params.token =
            configured_optional_or(&settings, "token", params.token);
        params.branch =
            configured_optional_or(&settings, "branch", params.branch);
        params.extend_exclude = configured_optional_or(
            &settings,
            "extend_exclude",
            params.extend_exclude,
        );
        params.exclude =
            configured_optional_or(&settings, "exclude", params.exclude);
        params.include =
            configured_optional_or(&settings, "include", params.include);
        params.legacy_excludes = configured_or(
            &settings,
            "legacy_excludes",
            params.legacy_excludes,
        );
        params.utf8 = configured_or(&settings, "utf8", params.utf8);
        params.gzip = configured_or(&settings, "gzip", params.gzip);
        params.gzip_level =
            configured_gzip_level(&settings, params.gzip_level);
        params.secret_scan =
            configured_or(&settings, "secret_scan", params.secret_scan);
        params
    }
}

fn configured_or<T: TomlValue>(settings: &Config, key: &str, default: T) -> T {
    TomlValue::load_from_config(settings, key).unwrap_or(default)
}

fn configured_optional_or<T: TomlValue>(
    settings: &Config,
    key: &str,
    default: Option<T>,
) -> Option<T> {
    TomlValue::load_from_config(settings, key).ok().or(default)
}

fn configured_gzip_level(settings: &Config, default: u32) -> u32 {
    match TomlValue::load_from_config(settings, "gzip_level") {
        Ok(level @ 1..=9) => level as u32,
        _ => default,
    }
}

fn gzip_options(args: &cli::Flags, config: &Params) -> (bool, u32) {
    if args.no_gzip {
        return (false, config.gzip_level);
    }
    match args.gzip {
        Some(None) => (true, config.gzip_level),
        Some(Some(level)) => (true, level),
        None => (config.gzip, config.gzip_level),
    }
}

fn merge_optional_lists(
    cli: &Option<Vec<String>>,
    configured: &Option<Vec<String>>,
) -> Option<Vec<String>> {
    match (cli, configured) {
        (Some(cli), Some(configured)) => {
            Some([cli.clone(), configured.clone()].concat())
        }
        (Some(cli), None) => Some(cli.clone()),
        (None, Some(configured)) => Some(configured.clone()),
        (None, None) => None,
    }
}

fn extended_excludes(
    args: &cli::Flags,
    config: &Params,
) -> Option<Vec<String>> {
    if args.exclude.is_some() || config.exclude.is_some() {
        return None;
    }
    merge_optional_lists(&args.extend_exclude, &config.extend_exclude)
}

const fn paired_boolean_enabled(
    negative: bool,
    positive: bool,
    configured: bool,
) -> bool {
    !negative && (positive || configured)
}

impl Params {
    pub fn from_args_and_config(args: &cli::Flags, config: Params) -> Self {
        let (gzip, gzip_level) = gzip_options(args, &config);
        let extend_exclude = extended_excludes(args, &config);
        let include = merge_optional_lists(&args.include, &config.include);

        Params {
            output_file: args
                .output_file
                .clone()
                .or(config.output_file)
                .or(Params::default().output_file),
            stdout: args.stdout || config.stdout,
            model: args
                .model
                .clone()
                .or(config.model)
                .or(Params::default().model),
            clipboard: args.clipboard || config.clipboard,
            line_numbers: args.lnumbers || config.line_numbers,
            token: args.token.clone().or(config.token),
            branch: args.branch.clone().or(config.branch),
            extend_exclude,
            exclude: args.exclude.clone().or(config.exclude),
            include,
            legacy_excludes: paired_boolean_enabled(
                args.no_legacy_excludes,
                args.legacy_excludes,
                config.legacy_excludes,
            ),
            utf8: paired_boolean_enabled(args.no_utf8, args.utf8, config.utf8),
            gzip,
            gzip_level,
            secret_scan: paired_boolean_enabled(
                args.no_secret_scan,
                args.secret_scan,
                config.secret_scan,
            ),
            metadata: config.metadata,
        }
    }
}

#[cfg(test)]
#[path = "../tests/crate/structs.rs"]
mod tests;
