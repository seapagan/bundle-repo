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
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    const DEEPSEEK_V3_SHA256: [u8; 32] = [
        0x62, 0x1a, 0xc2, 0xe3, 0x2d, 0x0d, 0xba, 0x65, 0x84, 0x04, 0x41,
        0x23, 0x18, 0x81, 0x8a, 0xaa, 0x8c, 0xe8, 0xcd, 0xa4, 0x92, 0xe5,
        0x98, 0x30, 0x10, 0x9d, 0x8d, 0xa6, 0xb5, 0x17, 0xfb, 0x41,
    ];
    const DEEPSEEK_R1_SHA256: [u8; 32] = [
        0xec, 0xb6, 0xf9, 0xfc, 0x36, 0x98, 0x94, 0x34, 0x6f, 0x05, 0x11,
        0xf4, 0x07, 0x4c, 0xa7, 0x5c, 0xee, 0x5c, 0xd5, 0xf3, 0xb0, 0x6d,
        0x02, 0xf1, 0xba, 0x35, 0xfc, 0xd3, 0x9f, 0x8e, 0x12, 0x1d,
    ];
    const DEEPSEEK_V4_SHA256: [u8; 32] = [
        0x8f, 0x9f, 0x37, 0xca, 0x37, 0xfd, 0xc4, 0xf5, 0xfd, 0x36, 0xd5,
        0xcf, 0x4d, 0x3b, 0x0e, 0x83, 0x92, 0xed, 0xb4, 0xe8, 0x94, 0xfd,
        0x10, 0xcc, 0x0d, 0x70, 0xb4, 0x95, 0x7c, 0x86, 0x33, 0xcf,
    ];
    const GLM5_2_SHA256: [u8; 32] = [
        0x19, 0xe7, 0x73, 0x64, 0x8c, 0xb4, 0xe6, 0x5d, 0xe8, 0x66, 0x0e,
        0xa6, 0x36, 0x5e, 0x10, 0xac, 0xca, 0x11, 0x2d, 0x42, 0xa8, 0x54,
        0x92, 0x3d, 0xf9, 0x3d, 0xb4, 0xa6, 0xf3, 0x33, 0xa8, 0x2d,
    ];

    #[test]
    fn test_tokenizer_resource_sha256_hashes() {
        let cases = [
            (TokenizerFamily::DeepSeekV3, DEEPSEEK_V3_SHA256),
            (TokenizerFamily::DeepSeekR1, DEEPSEEK_R1_SHA256),
            (TokenizerFamily::DeepSeekV4, DEEPSEEK_V4_SHA256),
            (TokenizerFamily::Glm5_2, GLM5_2_SHA256),
        ];

        for (family, expected) in cases {
            let file = get_tokenizer_json(family).unwrap();
            let actual: [u8; 32] = Sha256::digest(file.data.as_ref()).into();

            assert_eq!(actual, expected);
            assert_eq!(file.metadata.sha256_hash(), expected);
        }
    }

    #[test]
    fn test_runtime_resources_include_only_tokenizer_json() {
        for family in [
            TokenizerFamily::DeepSeekV3,
            TokenizerFamily::DeepSeekR1,
            TokenizerFamily::DeepSeekV4,
            TokenizerFamily::Glm5_2,
        ] {
            assert!(Resources::get(family.resource_path()).is_some());
        }

        assert!(Resources::get("tokenizers/SOURCES.md").is_none());
        assert!(
            Resources::get("tokenizers/licenses/DeepSeek-MIT.txt").is_none()
        );
    }

    #[test]
    fn test_missing_resource_error_has_family_and_path() {
        let error = get_resource("tokenizers/missing.json", "Missing family")
            .err()
            .unwrap();

        assert!(error.contains("Missing family tokenizer asset"));
        assert!(error.contains("tokenizers/missing.json"));
    }
}
