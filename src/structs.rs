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
            extend_exclude: None,
            exclude: None,
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

        params
    }
}

impl Params {
    pub fn from_args_and_config(args: &cli::Flags, config: Params) -> Self {
        Params {
            output_file: args
                .output_file
                .clone()
                .or(config.output_file)
                .or(Params::default().output_file),
            model: args
                .model
                .clone()
                .or(config.model)
                .or(Params::default().model),
            stdout: args.stdout || config.stdout,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::{Config, File, FileFormat};

    #[test]
    fn test_vec_string_loading() {
        let config_str = r#"
            extend_exclude = ["target", "node_modules"]
        "#;
        let config = Config::builder()
            .add_source(File::from_str(config_str, FileFormat::Toml))
            .build()
            .unwrap();

        let params: Params = config.into();
        assert_eq!(
            params.extend_exclude,
            Some(vec!["target".to_string(), "node_modules".to_string()])
        );
    }

    #[test]
    fn test_basic_types() {
        let config_str = r#"
            string_val = "hello"
            bool_val = true
            int_val = 42
            float_val = 3.14
        "#;
        let config = Config::builder()
            .add_source(File::from_str(config_str, FileFormat::Toml))
            .build()
            .unwrap();

        assert_eq!(
            String::load_from_config(&config, "string_val").unwrap(),
            "hello"
        );
        assert_eq!(bool::load_from_config(&config, "bool_val").unwrap(), true);
        assert_eq!(i64::load_from_config(&config, "int_val").unwrap(), 42);
        assert_eq!(f64::load_from_config(&config, "float_val").unwrap(), 3.14);
    }

    #[test]
    fn test_optional_values() {
        let config_str = r#"
            present_value = "exists"
        "#;
        let config = Config::builder()
            .add_source(File::from_str(config_str, FileFormat::Toml))
            .build()
            .unwrap();

        let present: Option<String> =
            TomlValue::load_from_config(&config, "present_value").unwrap();
        let missing: Option<String> =
            TomlValue::load_from_config(&config, "missing_value").unwrap();

        assert_eq!(present, Some("exists".to_string()));
        assert_eq!(missing, None);
    }

    #[test]
    fn test_type_errors() {
        let config_str = r#"
            should_be_string = [1, 2, 3]
            should_be_int = [1, 2, 3]
            should_be_bool = [1, 2, 3]
            should_be_float = [1, 2, 3]
        "#;
        let config = Config::builder()
            .add_source(File::from_str(config_str, FileFormat::Toml))
            .build()
            .unwrap();

        assert!(matches!(
            String::load_from_config(&config, "should_be_string"),
            Err(ConfigError::TypeError { .. })
        ));
        assert!(matches!(
            i64::load_from_config(&config, "should_be_int"),
            Err(ConfigError::TypeError { .. })
        ));
        assert!(matches!(
            bool::load_from_config(&config, "should_be_bool"),
            Err(ConfigError::TypeError { .. })
        ));
        assert!(matches!(
            f64::load_from_config(&config, "should_be_float"),
            Err(ConfigError::TypeError { .. })
        ));
    }

    #[test]
    fn test_missing_values() {
        let config = Config::builder().build().unwrap();

        assert!(matches!(
            String::load_from_config(&config, "missing"),
            Err(ConfigError::Missing(_))
        ));
    }

    #[test]
    fn test_params_default() {
        let params = Params::default();
        assert_eq!(params.output_file, Some("packed-repo.xml".to_string()));
        assert_eq!(params.stdout, false);
        assert_eq!(params.model, Some("gpt4o".to_string()));
        assert_eq!(params.clipboard, false);
        assert_eq!(params.line_numbers, false);
        assert_eq!(params.token, None);
        assert_eq!(params.branch, None);
        assert_eq!(params.extend_exclude, None);
        assert_eq!(params.exclude, None);
    }

    #[test]
    fn test_params_from_config() {
        let config_str = r#"
            output_file = "custom.xml"
            stdout = true
            model = "different-model"
            clipboard = true
            line_numbers = true
            token = "secret-token"
            branch = "main"
            extend_exclude = ["target", "node_modules"]
            exclude = ["custom.xml"]
        "#;
        let config = Config::builder()
            .add_source(File::from_str(config_str, FileFormat::Toml))
            .build()
            .unwrap();

        let params: Params = config.into();
        assert_eq!(params.output_file, Some("custom.xml".to_string()));
        assert_eq!(params.stdout, true);
        assert_eq!(params.model, Some("different-model".to_string()));
        assert_eq!(params.clipboard, true);
        assert_eq!(params.line_numbers, true);
        assert_eq!(params.token, Some("secret-token".to_string()));
        assert_eq!(params.branch, Some("main".to_string()));
        assert_eq!(
            params.extend_exclude,
            Some(vec!["target".to_string(), "node_modules".to_string()])
        );
        assert_eq!(params.exclude, Some(vec!["custom.xml".to_string()]));
    }

    #[test]
    fn test_config_error_display() {
        let missing = ConfigError::Missing("test_key".to_string());
        let type_error = ConfigError::TypeError {
            key: "test_key".to_string(),
            message: "invalid type".to_string(),
        };
        let other = ConfigError::Other(config::ConfigError::NotFound(
            "test".to_string(),
        ));

        assert_eq!(
            missing.to_string(),
            "Missing TOML value for key: test_key"
        );
        assert_eq!(
            type_error.to_string(),
            "Type error for key test_key: invalid type"
        );
        assert!(other.to_string().contains("Config error:"));
    }

    #[test]
    fn test_vec_error_propagation() {
        let config_str = r#"
            array = "not_an_array"
        "#;
        let config = Config::builder()
            .add_source(File::from_str(config_str, FileFormat::Toml))
            .build()
            .unwrap();

        // Should fail when trying to load as Vec<String>
        let result: Result<Vec<String>, _> =
            TomlValue::load_from_config(&config, "array");
        assert!(matches!(result, Err(ConfigError::TypeError { .. })));
    }

    #[test]
    fn test_option_error_propagation() {
        let config_str = r#"
            wrong_type = [1, 2, 3]
        "#;
        let config = Config::builder()
            .add_source(File::from_str(config_str, FileFormat::Toml))
            .build()
            .unwrap();

        // Should propagate type error but not missing error
        let result: Result<Option<String>, _> =
            TomlValue::load_from_config(&config, "wrong_type");
        assert!(matches!(result, Err(ConfigError::TypeError { .. })));
    }

    #[test]
    fn test_config_error_from_impl() {
        // Test the From<config::ConfigError> implementation
        let not_found = config::ConfigError::NotFound("key".to_string());
        let invalid_type =
            config::ConfigError::Message("invalid type".to_string());
        let other_error =
            config::ConfigError::Message("some other error".to_string());

        assert!(matches!(
            ConfigError::from(not_found),
            ConfigError::Missing(_)
        ));
        assert!(matches!(
            ConfigError::from(invalid_type),
            ConfigError::TypeError { .. }
        ));
        assert!(matches!(
            ConfigError::from(other_error),
            ConfigError::Other(_)
        ));
    }

    #[test]
    fn test_partial_params_from_config() {
        let config_str = r#"
            stdout = true
            line_numbers = true
            output_file = "custom.xml"
        "#;
        let config = Config::builder()
            .add_source(File::from_str(config_str, FileFormat::Toml))
            .build()
            .unwrap();

        let params: Params = config.into();

        // These should be from the config
        assert!(params.stdout);
        assert!(params.line_numbers);
        assert_eq!(params.output_file, Some("custom.xml".to_string()));

        // These should be default values
        assert_eq!(params.model, Some("gpt4o".to_string()));
        assert!(!params.clipboard);
        assert_eq!(params.token, None);
        assert_eq!(params.branch, None);
        assert_eq!(params.extend_exclude, None);
        assert_eq!(params.exclude, None);
    }
}
