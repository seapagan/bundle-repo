use std::str::FromStr;
use tiktoken_rs::{cl100k_base, o200k_base, r50k_base, CoreBPE};

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
            _ => Err(format!("Unsupported model: {}", s)),
        }
    }
}
