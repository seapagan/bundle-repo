use std::sync::OnceLock;

use super::*;

const FIXTURES: [&str; 5] = [
    "Hello, world! Bundle Repo counts tokens.",
    "fn main() {\n    println!(\"Hello, world!\");\n}\n",
    "line one\r\nline two\r\n",
    "Bundle 仓库 トークン 테스트",
    "🦀 Rust + emoji! @#$%^&*()",
];

fn deepseek_v3() -> &'static TokenizerType {
    static TOKENIZER: OnceLock<TokenizerType> = OnceLock::new();
    TOKENIZER.get_or_init(|| Model::DeepSeekV3.to_tokenizer().unwrap())
}

fn deepseek_v4() -> &'static TokenizerType {
    static TOKENIZER: OnceLock<TokenizerType> = OnceLock::new();
    TOKENIZER.get_or_init(|| Model::DeepSeekV4.to_tokenizer().unwrap())
}

fn deepseek_r1() -> &'static TokenizerType {
    static TOKENIZER: OnceLock<TokenizerType> = OnceLock::new();
    TOKENIZER.get_or_init(|| Model::DeepSeekR1.to_tokenizer().unwrap())
}

fn glm5_2() -> &'static TokenizerType {
    static TOKENIZER: OnceLock<TokenizerType> = OnceLock::new();
    TOKENIZER.get_or_init(|| Model::Glm5_2.to_tokenizer().unwrap())
}

#[test]
fn test_supported_models_parse() {
    let cases = [
        ("gpt5", Model::GPT5),
        ("gpt4o", Model::GPT4o),
        ("gpt4", Model::GPT4),
        ("gpt3.5", Model::GPT3_5),
        ("deepseek-v4", Model::DeepSeekV4),
        ("deepseek-v3", Model::DeepSeekV3),
        ("deepseek-r1", Model::DeepSeekR1),
        ("glm5.2", Model::Glm5_2),
    ];

    for (value, expected) in cases {
        assert_eq!(Model::from_str(value), Ok(expected));
    }
}

#[test]
fn test_public_model_values_and_variants_stay_in_sync() {
    let parsed_models = MODEL_VALUES.map(|value| {
        value.parse::<Model>().unwrap_or_else(|error| {
            panic!("CLI model value does not parse: {value}: {error}")
        })
    });

    for model in [
        Model::GPT5,
        Model::GPT4o,
        Model::GPT4,
        Model::GPT3_5,
        Model::DeepSeekV4,
        Model::DeepSeekV3,
        Model::DeepSeekR1,
        Model::Glm5_2,
    ] {
        assert!(
            parsed_models.contains(&model),
            "Model variant has no public CLI value: {model:?}"
        );
    }
}

#[test]
fn test_model_parsing_is_case_insensitive() {
    assert_eq!(Model::from_str("GPT5"), Ok(Model::GPT5));
    assert_eq!(Model::from_str("DeepSeek-V4"), Ok(Model::DeepSeekV4));
    assert_eq!(Model::from_str("DeepSeek"), Ok(Model::DeepSeekR1));
    assert_eq!(Model::from_str("GLM5.2"), Ok(Model::Glm5_2));
}

#[test]
fn test_removed_models_are_rejected() {
    for value in ["gpt2", "GPT2", "gpt3", "GPT3"] {
        let error = Model::from_str(value).unwrap_err();
        assert!(error.contains("Unsupported model"));
        assert!(error.contains(value));
    }
}

#[test]
fn test_unknown_model_error_lists_supported_values() {
    let error = Model::from_str("unknown").unwrap_err();

    assert!(error.contains("Unsupported model: unknown"));
    for value in MODEL_VALUES {
        assert!(error.contains(value), "error omitted {value}");
    }
}

#[test]
fn test_legacy_deepseek_alias_maps_to_r1() {
    let model = Model::from_str("deepseek").unwrap();

    assert_eq!(model, Model::DeepSeekR1);
    assert_eq!(model.display_name(), "DeepSeek R1");
}

#[test]
fn test_model_display_names() {
    let cases = [
        (Model::GPT5, "GPT-5"),
        (Model::GPT4o, "GPT-4o"),
        (Model::GPT4, "GPT-4"),
        (Model::GPT3_5, "GPT-3.5"),
        (Model::DeepSeekV4, "DeepSeek V4"),
        (Model::DeepSeekV3, "DeepSeek V3"),
        (Model::DeepSeekR1, "DeepSeek R1"),
        (Model::Glm5_2, "GLM-5.2"),
    ];

    for (model, expected) in cases {
        assert_eq!(model.display_name(), expected);
    }
}

#[test]
fn test_tiktoken_model_pairs_have_equivalent_counts() {
    let gpt5 = Model::GPT5.to_tokenizer().unwrap();
    let gpt4o = Model::GPT4o.to_tokenizer().unwrap();
    let gpt4 = Model::GPT4.to_tokenizer().unwrap();
    let gpt3_5 = Model::GPT3_5.to_tokenizer().unwrap();

    for fixture in FIXTURES {
        assert_eq!(
            gpt5.count_tokens(fixture).unwrap(),
            gpt4o.count_tokens(fixture).unwrap()
        );
        assert_eq!(
            gpt4.count_tokens(fixture).unwrap(),
            gpt3_5.count_tokens(fixture).unwrap()
        );
    }
}

#[test]
fn test_deepseek_alias_and_r1_counts_remain_equivalent() {
    let r1 = deepseek_r1();
    let alias = Model::from_str("deepseek").unwrap().to_tokenizer().unwrap();

    for fixture in FIXTURES {
        assert_eq!(
            alias.count_tokens(fixture).unwrap(),
            r1.count_tokens(fixture).unwrap()
        );
    }
}

#[test]
fn test_deepseek_r1_reasoning_token_differs_from_v3() {
    let alias = Model::from_str("deepseek").unwrap().to_tokenizer().unwrap();

    for (fixture, expected_id) in [("<think>", 128798), ("</think>", 128799)] {
        let r1_ids = hugging_face_ids(deepseek_r1(), fixture);

        assert_eq!(r1_ids, [expected_id]);
        assert_eq!(hugging_face_ids(&alias, fixture), r1_ids);
        assert_ne!(hugging_face_ids(deepseek_v3(), fixture), r1_ids);
    }
}

#[test]
fn test_deepseek_v4_specific_added_token_differs_from_v3_and_r1() {
    let fixture = "<｜begin▁of▁repo▁name｜>";
    let v4_ids = hugging_face_ids(deepseek_v4(), fixture);

    assert_eq!(v4_ids, [128815]);
    assert_ne!(v4_ids, hugging_face_ids(deepseek_v3(), fixture));
    assert_ne!(v4_ids, hugging_face_ids(deepseek_r1(), fixture));
}

#[test]
fn test_all_backends_tokenize_fixtures_and_empty_input() {
    let tokenizers = [
        (gpt5(), [9, 12, 6, 8, 13]),
        (gpt4(), [9, 12, 6, 13, 13]),
        (deepseek_v3(), [10, 12, 8, 9, 15]),
        (deepseek_r1(), [10, 12, 8, 9, 15]),
        (deepseek_v4(), [10, 12, 8, 9, 15]),
        (glm5_2(), [9, 12, 6, 10, 13]),
    ];

    for (tokenizer, expected_counts) in tokenizers {
        assert_eq!(tokenizer.count_tokens("").unwrap(), 0);
        for (fixture, expected) in FIXTURES.into_iter().zip(expected_counts) {
            let count = tokenizer.count_tokens(fixture).unwrap();
            assert_eq!(count, expected, "{tokenizer:?} changed count");
        }
    }
}

#[test]
fn test_tokenizer_type_debug_includes_backend_context() {
    let tiktoken = Model::GPT5.to_tokenizer().unwrap();
    assert_eq!(format!("{tiktoken:?}"), "TokenizerType::Tiktoken(...)");
    assert_eq!(
        format!("{:?}", deepseek_v4()),
        "TokenizerType::HuggingFace(DeepSeek V4, ...)"
    );
}

fn gpt5() -> &'static TokenizerType {
    static TOKENIZER: OnceLock<TokenizerType> = OnceLock::new();
    TOKENIZER.get_or_init(|| Model::GPT5.to_tokenizer().unwrap())
}

fn gpt4() -> &'static TokenizerType {
    static TOKENIZER: OnceLock<TokenizerType> = OnceLock::new();
    TOKENIZER.get_or_init(|| Model::GPT4.to_tokenizer().unwrap())
}

fn hugging_face_ids(tokenizer: &TokenizerType, text: &str) -> Vec<u32> {
    match tokenizer {
        TokenizerType::HuggingFace { tokenizer, .. } => {
            tokenizer.encode(text, false).unwrap().get_ids().to_vec()
        }
        TokenizerType::Tiktoken(_) => {
            panic!("expected Hugging Face backend")
        }
    }
}
