use std::str::FromStr;
use tiktoken_rs::{cl100k_base, o200k_base, r50k_base, CoreBPE};

#[derive(Debug)]
pub enum Model {
    GPT4o,
    GPT4,
    GPT3_5,
    GPT3,
    GPT2,
}

impl Model {
    /// Converts the `Model` enum to the corresponding tokenizer instance.
    pub fn to_tokenizer(&self) -> Result<CoreBPE, String> {
        match self {
            Model::GPT4o => Ok(o200k_base().unwrap()),
            Model::GPT4 | Model::GPT3_5 => Ok(cl100k_base().unwrap()),
            Model::GPT3 | Model::GPT2 => Ok(r50k_base().unwrap()),
        }
    }

    /// Returns a user-friendly name for the model.
    pub fn display_name(&self) -> &'static str {
        match self {
            Model::GPT4o => "GPT-4o",
            Model::GPT4 => "GPT-4",
            Model::GPT3_5 => "GPT-3.5",
            Model::GPT3 => "GPT-3",
            Model::GPT2 => "GPT-2",
        }
    }
}

impl FromStr for Model {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gpt4o" => Ok(Model::GPT4o),
            "gpt4" => Ok(Model::GPT4),
            "gpt3.5" => Ok(Model::GPT3_5),
            "gpt3" => Ok(Model::GPT3),
            "gpt2" => Ok(Model::GPT2),
            _ => Err(format!("ERROR: Unsupported model: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_from_str_valid() {
        assert!(matches!(Model::from_str("gpt4o"), Ok(Model::GPT4o)));
        assert!(matches!(Model::from_str("gpt4"), Ok(Model::GPT4)));
        assert!(matches!(Model::from_str("gpt3.5"), Ok(Model::GPT3_5)));
        assert!(matches!(Model::from_str("gpt3"), Ok(Model::GPT3)));
        assert!(matches!(Model::from_str("gpt2"), Ok(Model::GPT2)));
    }

    #[test]
    fn test_model_from_str_case_insensitive() {
        assert!(matches!(Model::from_str("GPT4O"), Ok(Model::GPT4o)));
        assert!(matches!(Model::from_str("GPT4"), Ok(Model::GPT4)));
        assert!(matches!(Model::from_str("GPT3.5"), Ok(Model::GPT3_5)));
        assert!(matches!(Model::from_str("GPT3"), Ok(Model::GPT3)));
        assert!(matches!(Model::from_str("GPT2"), Ok(Model::GPT2)));
    }

    #[test]
    fn test_model_from_str_invalid() {
        // Test invalid model names
        assert!(Model::from_str("invalid").is_err());
        assert!(Model::from_str("gpt5").is_err());
        assert!(Model::from_str("").is_err());
        assert!(Model::from_str(" ").is_err());

        // Verify error messages
        assert_eq!(
            Model::from_str("invalid").unwrap_err(),
            "ERROR: Unsupported model: invalid".to_string()
        );
    }

    #[test]
    fn test_model_to_tokenizer() {
        // Test that each model returns a valid tokenizer
        assert!(Model::GPT4o.to_tokenizer().is_ok());
        assert!(Model::GPT4.to_tokenizer().is_ok());
        assert!(Model::GPT3_5.to_tokenizer().is_ok());
        assert!(Model::GPT3.to_tokenizer().is_ok());
        assert!(Model::GPT2.to_tokenizer().is_ok());
    }

    #[test]
    fn test_model_display_names() {
        assert_eq!(Model::GPT4o.display_name(), "GPT-4o");
        assert_eq!(Model::GPT4.display_name(), "GPT-4");
        assert_eq!(Model::GPT3_5.display_name(), "GPT-3.5");
        assert_eq!(Model::GPT3.display_name(), "GPT-3");
        assert_eq!(Model::GPT2.display_name(), "GPT-2");
    }

    #[test]
    fn test_tokenizer_compatibility() {
        // GPT4o should use o200k_base
        let gpt4o = Model::GPT4o.to_tokenizer().unwrap();
        // GPT4 and GPT3.5 should use cl100k_base
        let gpt4 = Model::GPT4.to_tokenizer().unwrap();
        let gpt3_5 = Model::GPT3_5.to_tokenizer().unwrap();
        // GPT3 and GPT2 should use r50k_base
        let gpt3 = Model::GPT3.to_tokenizer().unwrap();
        let gpt2 = Model::GPT2.to_tokenizer().unwrap();

        // Test that models using the same base tokenizer produce identical results
        let test_string = "Hello, world!";

        // Test GPT4o produces different results (uses different base)
        assert_ne!(
            gpt4o.encode_with_special_tokens(test_string),
            gpt4.encode_with_special_tokens(test_string)
        );

        // Test models with same base produce identical results
        assert_eq!(
            gpt4.encode_with_special_tokens(test_string),
            gpt3_5.encode_with_special_tokens(test_string)
        );
        assert_eq!(
            gpt3.encode_with_special_tokens(test_string),
            gpt2.encode_with_special_tokens(test_string)
        );
    }
}
