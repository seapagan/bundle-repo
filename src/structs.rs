use crate::cli;
use config::Config;
use serde::Deserialize;
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
    pub utf8: bool,
    pub gzip: bool,
    pub gzip_level: u32,
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
            utf8: false,
            gzip: false,
            gzip_level: 6,
        }
    }
}

impl From<Config> for Params {
    fn from(settings: Config) -> Self {
        let mut params = Params::default();

        // Helper function to update field only if present in config
        let update_if_present = |key: &str| -> Option<String> {
            TomlValue::load_from_config(&settings, key).ok()
        };

        // Only update fields if they are present in config
        if let Some(val) = update_if_present("output_file") {
            params.output_file = Some(val);
        }
        if let Ok(val) = TomlValue::load_from_config(&settings, "stdout") {
            params.stdout = val;
        }
        if let Some(val) = update_if_present("model") {
            params.model = Some(val);
        }
        if let Ok(val) = TomlValue::load_from_config(&settings, "clipboard") {
            params.clipboard = val;
        }
        if let Ok(val) = TomlValue::load_from_config(&settings, "line_numbers")
        {
            params.line_numbers = val;
        }
        if let Some(val) = update_if_present("token") {
            params.token = Some(val);
        }
        if let Some(val) = update_if_present("branch") {
            params.branch = Some(val);
        }
        if let Ok(val) =
            TomlValue::load_from_config(&settings, "extend_exclude")
        {
            params.extend_exclude = val;
        }
        if let Ok(val) = TomlValue::load_from_config(&settings, "exclude") {
            params.exclude = val;
        }
        if let Ok(val) = TomlValue::load_from_config(&settings, "utf8") {
            params.utf8 = val;
        }
        if let Ok(val) = TomlValue::load_from_config(&settings, "gzip") {
            params.gzip = val;
        }
        if let Ok(level @ 1..=9) =
            TomlValue::load_from_config(&settings, "gzip_level")
        {
            params.gzip_level = level as u32;
        }
        params
    }
}

impl Params {
    pub fn from_args_and_config(args: &cli::Flags, config: Params) -> Self {
        let (gzip, gzip_level) = if args.no_gzip {
            (false, config.gzip_level)
        } else {
            match args.gzip {
                Some(None) => (true, config.gzip_level),
                Some(Some(level)) => (true, level),
                None => (config.gzip, config.gzip_level),
            }
        };

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
            extend_exclude: if args.exclude.is_some()
                || config.exclude.is_some()
            {
                None
            } else {
                match (&args.extend_exclude, config.extend_exclude) {
                    (Some(cli_excludes), Some(config_excludes)) => {
                        Some([cli_excludes.clone(), config_excludes].concat())
                    }
                    (Some(cli_excludes), None) => Some(cli_excludes.clone()),
                    (None, Some(config_excludes)) => Some(config_excludes),
                    (None, None) => None,
                }
            },
            exclude: match (&args.exclude, config.exclude) {
                (Some(cli_excludes), Some(_config_excludes)) => {
                    Some(cli_excludes.clone())
                }
                (Some(cli_excludes), None) => Some(cli_excludes.clone()),
                (None, Some(config_excludes)) => Some(config_excludes),
                (None, None) => None,
            },
            utf8: if args.no_utf8 {
                false
            } else if args.utf8 {
                true
            } else {
                config.utf8
            },
            gzip,
            gzip_level,
        }
    }
}

#[cfg(test)]
#[path = "../tests/crate/structs.rs"]
mod tests;
