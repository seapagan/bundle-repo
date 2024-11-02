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
            e if e.to_string().contains("not found") => {
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
            if e.to_string().contains("not found") {
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
            if e.to_string().contains("not found") {
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
            if e.to_string().contains("not found") {
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
            if e.to_string().contains("not found") {
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
                if e.to_string().contains("not found") {
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

macro_rules! config_to_params {
    ($settings:expr, $params:ident, $( $field:ident ),* ) => {
        $(
            match TomlValue::load_from_config(&$settings, stringify!($field)) {
                Ok(value) => $params.$field = value,
                Err(ConfigError::Missing(_)) => (), // Use default value
                Err(e) => eprintln!("Error loading TOML field {}: {}", stringify!($field), e),
            }
        )*
    };
}

#[derive(Debug, Deserialize)]
pub struct Params {
    pub output_file: Option<String>,
    pub stdout: bool,
    pub model: Option<String>,
    pub clipboard: bool,
    pub line_numbers: bool,
    pub token: Option<String>,
    pub branch: Option<String>,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            output_file: Some("packed-repo.xml".to_string()),
            stdout: false,
            model: Some("gpt4o".to_string()),
            clipboard: false,
            line_numbers: false,
            token: None,
            branch: None,
        }
    }
}

impl From<Config> for Params {
    fn from(settings: Config) -> Self {
        let mut params = Params::default();

        config_to_params!(
            settings,
            params,
            output_file,
            stdout,
            model,
            clipboard,
            line_numbers,
            token,
            branch
        );

        params
    }
}
