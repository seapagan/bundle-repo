use rust_embed::{EmbeddedFile, RustEmbed};

#[derive(RustEmbed)]
#[folder = "resources/"]
#[include = "tokenizers/*.json"]
pub struct Resources;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenizerFamily {
    DeepSeekV3,
    DeepSeekR1,
    DeepSeekV4,
    Glm5_2,
}

impl TokenizerFamily {
    pub const fn resource_path(self) -> &'static str {
        match self {
            Self::DeepSeekV3 => "tokenizers/deepseek-v3.json",
            Self::DeepSeekR1 => "tokenizers/deepseek-r1.json",
            Self::DeepSeekV4 => "tokenizers/deepseek-v4.json",
            Self::Glm5_2 => "tokenizers/glm-5.2.json",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::DeepSeekV3 => "DeepSeek V3",
            Self::DeepSeekR1 => "DeepSeek R1",
            Self::DeepSeekV4 => "DeepSeek V4",
            Self::Glm5_2 => "GLM-5.2",
        }
    }
}

pub fn get_tokenizer_json(
    family: TokenizerFamily,
) -> Result<EmbeddedFile, String> {
    get_resource(family.resource_path(), family.display_name())
}

fn get_resource(
    path: &str,
    family_name: &str,
) -> Result<EmbeddedFile, String> {
    Resources::get(path).ok_or_else(|| {
        format!(
            "{family_name} tokenizer asset not found in embedded resources: \
                 {path}"
        )
    })
}

#[cfg(test)]
#[path = "../tests/crate/embedded.rs"]
mod tests;
