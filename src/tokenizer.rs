use std::str::FromStr;

use tiktoken_rs::{CoreBPE, cl100k_base, o200k_base};
use tokenizers::Tokenizer;

use crate::embedded::{self, TokenizerFamily};

pub const MODEL_VALUES: [&str; 9] = [
    "gpt5",
    "gpt4o",
    "gpt4",
    "gpt3.5",
    "deepseek-v4",
    "deepseek-v3",
    "deepseek-r1",
    "glm5.2",
    "deepseek",
];

// CoreBPE does not implement Debug, so this wrapper cannot derive it.
pub enum TokenizerType {
    Tiktoken(CoreBPE),
    HuggingFace {
        tokenizer: Box<Tokenizer>,
        family: TokenizerFamily,
    },
}

impl std::fmt::Debug for TokenizerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tiktoken(_) => write!(f, "TokenizerType::Tiktoken(...)"),
            Self::HuggingFace { family, .. } => write!(
                f,
                "TokenizerType::HuggingFace({}, ...)",
                family.display_name()
            ),
        }
    }
}

impl TokenizerType {
    pub fn count_tokens(&self, text: &str) -> Result<usize, String> {
        match self {
            Self::Tiktoken(tokenizer) => {
                Ok(tokenizer.encode_with_special_tokens(text).len())
            }
            Self::HuggingFace { tokenizer, family } => tokenizer
                .encode(text, false)
                .map_err(|error| {
                    format!(
                        "{} tokenization error: {error}",
                        family.display_name()
                    )
                })
                .map(|encoding| encoding.get_ids().len()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Model {
    GPT5,
    GPT4o,
    GPT4,
    GPT3_5,
    DeepSeekV4,
    DeepSeekV3,
    DeepSeekR1,
    Glm5_2,
}

impl Model {
    /// Converts the model to its corresponding tokenizer instance.
    pub fn to_tokenizer(self) -> Result<TokenizerType, String> {
        match self {
            Self::GPT5 | Self::GPT4o => {
                o200k_base().map(TokenizerType::Tiktoken).map_err(|error| {
                    format!("Failed to load o200k_base tokenizer: {error}")
                })
            }
            Self::GPT4 | Self::GPT3_5 => {
                cl100k_base().map(TokenizerType::Tiktoken).map_err(|error| {
                    format!("Failed to load cl100k_base tokenizer: {error}")
                })
            }
            Self::DeepSeekV4 => load_hugging_face(TokenizerFamily::DeepSeekV4),
            Self::DeepSeekV3 => load_hugging_face(TokenizerFamily::DeepSeekV3),
            Self::DeepSeekR1 => load_hugging_face(TokenizerFamily::DeepSeekR1),
            Self::Glm5_2 => load_hugging_face(TokenizerFamily::Glm5_2),
        }
    }

    /// Returns a user-friendly name for the model.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::GPT5 => "GPT-5",
            Self::GPT4o => "GPT-4o",
            Self::GPT4 => "GPT-4",
            Self::GPT3_5 => "GPT-3.5",
            Self::DeepSeekV4 => "DeepSeek V4",
            Self::DeepSeekV3 => "DeepSeek V3",
            Self::DeepSeekR1 => "DeepSeek R1",
            Self::Glm5_2 => "GLM-5.2",
        }
    }
}

fn load_hugging_face(
    family: TokenizerFamily,
) -> Result<TokenizerType, String> {
    let file = embedded::get_tokenizer_json(family)?;
    let tokenizer =
        Tokenizer::from_bytes(file.data.as_ref()).map_err(|error| {
            format!(
                "Failed to load {} tokenizer from {}: {error}",
                family.display_name(),
                family.resource_path()
            )
        })?;

    Ok(TokenizerType::HuggingFace {
        tokenizer: Box::new(tokenizer),
        family,
    })
}

impl FromStr for Model {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_lowercase().as_str() {
            "gpt5" => Ok(Self::GPT5),
            "gpt4o" => Ok(Self::GPT4o),
            "gpt4" => Ok(Self::GPT4),
            "gpt3.5" => Ok(Self::GPT3_5),
            "deepseek-v4" => Ok(Self::DeepSeekV4),
            "deepseek-v3" => Ok(Self::DeepSeekV3),
            "deepseek-r1" | "deepseek" => Ok(Self::DeepSeekR1),
            "glm5.2" => Ok(Self::Glm5_2),
            _ => Err(format!(
                "ERROR: Unsupported model: {value}. Supported models: {}",
                MODEL_VALUES.join(", ")
            )),
        }
    }
}

#[cfg(test)]
#[path = "../tests/crate/tokenizer.rs"]
mod tests;
