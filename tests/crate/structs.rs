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
            float_val = 2.5
        "#;
    let config = Config::builder()
        .add_source(File::from_str(config_str, FileFormat::Toml))
        .build()
        .unwrap();

    assert_eq!(
        String::load_from_config(&config, "string_val").unwrap(),
        "hello"
    );
    assert!(bool::load_from_config(&config, "bool_val").unwrap());
    assert_eq!(i64::load_from_config(&config, "int_val").unwrap(), 42);
    assert_eq!(f64::load_from_config(&config, "float_val").unwrap(), 2.5);
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
    assert!(!params.stdout);
    assert_eq!(params.model, Some("gpt5".to_string()));
    assert!(!params.clipboard);
    assert!(!params.line_numbers);
    assert_eq!(params.token, None);
    assert_eq!(params.branch, None);
    assert_eq!(params.extend_exclude, None);
    assert_eq!(params.exclude, None);
    assert!(!params.utf8);
    assert!(!params.gzip);
    assert_eq!(params.gzip_level, 6);
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
    assert!(params.stdout);
    assert_eq!(params.model, Some("different-model".to_string()));
    assert!(params.clipboard);
    assert!(params.line_numbers);
    assert_eq!(params.token, Some("secret-token".to_string()));
    assert_eq!(params.branch, Some("main".to_string()));
    assert_eq!(
        params.extend_exclude,
        Some(vec!["target".to_string(), "node_modules".to_string()])
    );
    assert_eq!(params.exclude, Some(vec!["custom.xml".to_string()]));
    assert!(!params.utf8);
}

#[test]
fn test_config_error_display() {
    let missing = ConfigError::Missing("test_key".to_string());
    let type_error = ConfigError::TypeError {
        key: "test_key".to_string(),
        message: "invalid type".to_string(),
    };
    let other =
        ConfigError::Other(config::ConfigError::NotFound("test".to_string()));

    assert_eq!(missing.to_string(), "Missing TOML value for key: test_key");
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
    assert_eq!(params.model, Some("gpt5".to_string()));
    assert!(!params.clipboard);
    assert_eq!(params.token, None);
    assert_eq!(params.branch, None);
    assert_eq!(params.extend_exclude, None);
    assert_eq!(params.exclude, None);
    assert!(!params.utf8);
}

#[test]
fn test_legacy_deepseek_config_selects_r1() {
    use crate::tokenizer::Model;

    for alias in ["deepseek", "DeepSeek"] {
        let config = Config::builder()
            .add_source(File::from_str(
                &format!("model = \"{alias}\""),
                FileFormat::Toml,
            ))
            .build()
            .unwrap();
        let params: Params = config.into();

        assert_eq!(
            params.model.unwrap().parse::<Model>(),
            Ok(Model::DeepSeekR1)
        );
    }
}

#[test]
fn test_model_config_is_case_insensitive() {
    use crate::tokenizer::Model;

    for (value, expected) in [
        ("GPT5", Model::GPT5),
        ("DeepSeek-V4", Model::DeepSeekV4),
        ("GLM5.2", Model::Glm5_2),
    ] {
        let config = Config::builder()
            .add_source(File::from_str(
                &format!("model = \"{value}\""),
                FileFormat::Toml,
            ))
            .build()
            .unwrap();
        let params: Params = config.into();

        assert_eq!(params.model.unwrap().parse::<Model>(), Ok(expected));
    }
}

#[test]
fn test_removed_model_configs_are_unsupported() {
    use crate::tokenizer::Model;

    for model in ["gpt2", "GPT2", "gpt3", "GPT3"] {
        let config = Config::builder()
            .add_source(File::from_str(
                &format!("model = \"{model}\""),
                FileFormat::Toml,
            ))
            .build()
            .unwrap();
        let params: Params = config.into();
        let error = params.model.unwrap().parse::<Model>().unwrap_err();

        assert!(error.contains("Unsupported model"));
        assert!(error.contains(model));
    }
}

#[test]
fn test_unknown_model_config_reports_supported_values() {
    use crate::tokenizer::{MODEL_VALUES, Model};

    let config = Config::builder()
        .add_source(File::from_str("model = \"unknown\"", FileFormat::Toml))
        .build()
        .unwrap();
    let params: Params = config.into();
    let error = params.model.unwrap().parse::<Model>().unwrap_err();

    for value in MODEL_VALUES {
        assert!(error.contains(value), "error omitted {value}");
    }
}

#[test]
fn test_utf8_config_true() {
    let config_str = r#"
            utf8 = true
        "#;
    let config = Config::builder()
        .add_source(File::from_str(config_str, FileFormat::Toml))
        .build()
        .unwrap();
    let params: Params = config.into();
    assert!(params.utf8);
}

#[test]
fn test_utf8_config_false() {
    let config_str = r#"
            utf8 = false
        "#;
    let config = Config::builder()
        .add_source(File::from_str(config_str, FileFormat::Toml))
        .build()
        .unwrap();
    let params: Params = config.into();
    assert!(!params.utf8);
}

#[test]
fn test_utf8_config_default() {
    let config_str = r#"
            # no utf8 setting
        "#;
    let config = Config::builder()
        .add_source(File::from_str(config_str, FileFormat::Toml))
        .build()
        .unwrap();
    let params: Params = config.into();
    assert!(!params.utf8);
}

#[test]
fn test_utf8_config_invalid_type() {
    let config_str = r#"
            utf8 = "not a bool"
        "#;
    let config = Config::builder()
        .add_source(File::from_str(config_str, FileFormat::Toml))
        .build()
        .unwrap();
    let params: Params = config.into();
    assert!(!params.utf8);
}

#[test]
fn test_gzip_config_values() {
    let config = Config::builder()
        .add_source(File::from_str(
            "gzip = true\ngzip_level = 9",
            FileFormat::Toml,
        ))
        .build()
        .unwrap();
    let params: Params = config.into();
    assert!(params.gzip);
    assert_eq!(params.gzip_level, 9);
}

#[test]
fn test_invalid_gzip_config_types_are_ignored() {
    for config_str in ["gzip = [1]", "gzip_level = \"high\""] {
        let config = Config::builder()
            .add_source(File::from_str(config_str, FileFormat::Toml))
            .build()
            .unwrap();
        let params: Params = config.into();
        assert!(!params.gzip);
        assert_eq!(params.gzip_level, 6);
    }
}

#[test]
fn test_invalid_gzip_config_levels_are_ignored() {
    for level in [0, 10] {
        let config_str = format!("gzip_level = {level}");
        let config = Config::builder()
            .add_source(File::from_str(&config_str, FileFormat::Toml))
            .build()
            .unwrap();
        let params: Params = config.into();
        assert_eq!(params.gzip_level, 6);
    }
}
