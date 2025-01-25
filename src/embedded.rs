use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "resources/"]
pub struct Resources;

pub fn get_tokenizer_json() -> Result<Vec<u8>, String> {
    Resources::get("tokenizer.json")
        .ok_or_else(|| {
            "DeepSeek tokenizer.json not found in embedded resources"
                .to_string()
        })
        .map(|f| f.data.to_vec())
}
