use std::str::FromStr;
use tiktoken_rs::{cl100k_base, o200k_base, r50k_base, CoreBPE};
use tokenizers::Tokenizer;

use crate::embedded;

// We can't derive Debug for TokenizerType because CoreBPE doesn't implement Debug
pub enum TokenizerType {
    GPT(CoreBPE),
    DeepSeek(Tokenizer),
}

// Manually implement Debug for TokenizerType
impl std::fmt::Debug for TokenizerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenizerType::GPT(_) => write!(f, "TokenizerType::GPT(...)"),
            TokenizerType::DeepSeek(_) => {
                write!(f, "TokenizerType::DeepSeek(...)")
            }
        }
    }
}

impl TokenizerType {
    pub fn count_tokens(&self, text: &str) -> Result<usize, String> {
        match self {
            TokenizerType::GPT(tokenizer) => {
                Ok(tokenizer.encode_with_special_tokens(text).len())
            }
            TokenizerType::DeepSeek(tokenizer) => tokenizer
                .encode(text, false)
                .map_err(|e| format!("DeepSeek tokenization error: {}", e))
                .map(|encoding| encoding.get_ids().len()),
        }
    }
}

#[derive(Debug)]
pub enum Model {
    GPT4o,
    GPT4,
    GPT3_5,
    GPT3,
    GPT2,
    DeepSeek,
}

impl Model {
    /// Converts the `Model` enum to the corresponding tokenizer instance.
    pub fn to_tokenizer(&self) -> Result<TokenizerType, String> {
        match self {
            Model::GPT4o => Ok(TokenizerType::GPT(
                o200k_base().map_err(|e| e.to_string())?,
            )),
            Model::GPT4 | Model::GPT3_5 => Ok(TokenizerType::GPT(
                cl100k_base().map_err(|e| e.to_string())?,
            )),
            Model::GPT3 | Model::GPT2 => {
                Ok(TokenizerType::GPT(r50k_base().map_err(|e| e.to_string())?))
            }
            Model::DeepSeek => {
                let json_data = embedded::get_tokenizer_json()?;
                let tokenizer =
                    Tokenizer::from_bytes(&json_data).map_err(|e| {
                        format!("Failed to load DeepSeek tokenizer: {}", e)
                    })?;
                Ok(TokenizerType::DeepSeek(tokenizer))
            }
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
            Model::DeepSeek => "DeepSeek",
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
            "deepseek" => Ok(Model::DeepSeek),
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
        assert!(matches!(Model::from_str("deepseek"), Ok(Model::DeepSeek)));
    }

    #[test]
    fn test_model_from_str_case_insensitive() {
        assert!(matches!(Model::from_str("GPT4O"), Ok(Model::GPT4o)));
        assert!(matches!(Model::from_str("DEEPSEEK"), Ok(Model::DeepSeek)));
    }

    #[test]
    fn test_model_display_names() {
        assert_eq!(Model::DeepSeek.display_name(), "DeepSeek");
    }

    #[test]
    fn test_tokenizer_compatibility() {
        let test_string = "Hello, world!";

        // Test that GPT models work
        let gpt4 = Model::GPT4.to_tokenizer().unwrap();
        let count = gpt4.count_tokens(test_string).unwrap();
        assert!(count > 0);

        // Skip DeepSeek test if tokenizer file is not present
        if let Ok(deepseek) = Model::DeepSeek.to_tokenizer() {
            let count = deepseek.count_tokens(test_string).unwrap();
            assert!(count > 0);
        }
    }

    #[test]
    fn test_deepseek_tokenizer() {
        let model = Model::DeepSeek;
        let tokenizer = model.to_tokenizer().unwrap();

        // Test basic tokenization
        let count = tokenizer.count_tokens("Hello, world!").unwrap();
        assert!(count > 0);

        // Test empty string
        let count = tokenizer.count_tokens("").unwrap();
        assert_eq!(count, 0);

        // Test multi-line code
        let code = r#"
fn main() {
    println!("Hello, world!");
}
"#;
        let count = tokenizer.count_tokens(code).unwrap();
        assert!(count > 0);

        // Test special characters
        let special = "🦀 Rust is awesome! \n\t\r";
        let count = tokenizer.count_tokens(special).unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_deepseek_model_conversion() {
        // Test model string parsing
        assert!(matches!(Model::from_str("deepseek"), Ok(Model::DeepSeek)));
        assert!(matches!(Model::from_str("DEEPSEEK"), Ok(Model::DeepSeek)));
        assert!(matches!(Model::from_str("DeepSeek"), Ok(Model::DeepSeek)));

        // Test display name
        assert_eq!(Model::DeepSeek.display_name(), "DeepSeek");
    }

    #[test]
    fn test_tokenizer_type_debug() {
        let gpt = Model::GPT4.to_tokenizer().unwrap();
        let deepseek = Model::DeepSeek.to_tokenizer().unwrap();

        // Test Debug implementation
        assert_eq!(format!("{:?}", gpt), "TokenizerType::GPT(...)");
        assert_eq!(format!("{:?}", deepseek), "TokenizerType::DeepSeek(...)");
    }
}
