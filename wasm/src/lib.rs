use palsav_decoder::assemble_world;
use wasm_bindgen::prelude::*;

const MAX_LEVEL_BYTES: usize = 192 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;
const MAX_PLAYER_BYTES: usize = 32 * 1024 * 1024;
const MAX_PLAYER_FILES: usize = 32;
const MAX_RESULT_BYTES: usize = 96 * 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: usize = 384 * 1024 * 1024;

#[wasm_bindgen]
pub struct WebDecoder {
    level: Vec<u8>,
    metadata: Option<Vec<u8>>,
    players: Vec<(String, Vec<u8>)>,
}

#[wasm_bindgen]
impl WebDecoder {
    #[wasm_bindgen(constructor)]
    pub fn new(level: Vec<u8>) -> Result<WebDecoder, JsValue> {
        limit("Level.sav", level.len(), MAX_LEVEL_BYTES)?;
        validate_declared_size(&level)?;
        Ok(Self {
            level,
            metadata: None,
            players: Vec::new(),
        })
    }

    pub fn set_metadata(&mut self, metadata: Vec<u8>) -> Result<(), JsValue> {
        limit("LevelMeta.sav", metadata.len(), MAX_METADATA_BYTES)?;
        validate_declared_size(&metadata)?;
        self.metadata = Some(metadata);
        Ok(())
    }

    pub fn add_player(&mut self, player_uid: String, bytes: Vec<u8>) -> Result<(), JsValue> {
        if self.players.len() >= MAX_PLAYER_FILES {
            return Err(code("LIMIT_EXCEEDED", "Too many player files."));
        }
        limit("player save", bytes.len(), MAX_PLAYER_BYTES)?;
        validate_declared_size(&bytes)?;
        self.players.push((player_uid, bytes));
        Ok(())
    }

    pub fn finish_json(self) -> Result<String, JsValue> {
        let players = self
            .players
            .iter()
            .map(|(uid, bytes)| (uid.clone(), bytes.as_slice()))
            .collect();
        let document = assemble_world(&self.level, self.metadata.as_deref(), players)
            .map_err(|error| code(classify_error(&error), &error))?;
        let json = serde_json::to_string(&document)
            .map_err(|error| code("CORRUPT_SAVE", &error.to_string()))?;
        limit("decoded result", json.len(), MAX_RESULT_BYTES)?;
        Ok(json)
    }
}

#[wasm_bindgen]
pub fn decoder_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn limit(label: &str, actual: usize, maximum: usize) -> Result<(), JsValue> {
    if actual > maximum {
        return Err(code(
            "LIMIT_EXCEEDED",
            &format!("{label} exceeds the browser limit ({actual}/{maximum} bytes)."),
        ));
    }
    Ok(())
}

fn validate_declared_size(bytes: &[u8]) -> Result<(), JsValue> {
    let header = bytes
        .get(..4)
        .ok_or_else(|| code("CORRUPT_SAVE", "The save header is truncated."))?;
    let declared = u32::from_le_bytes(header.try_into().expect("four-byte slice")) as usize;
    limit("decompressed save", declared, MAX_DECOMPRESSED_BYTES)
}

fn classify_error(error: &str) -> &'static str {
    if error.contains("exceeds") {
        "LIMIT_EXCEEDED"
    } else if error.contains("Unknown Palworld .sav compression") {
        "UNSUPPORTED_FORMAT"
    } else {
        "CORRUPT_SAVE"
    }
}

fn code(code: &str, detail: &str) -> JsValue {
    JsValue::from_str(&format!("{code}:{detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_limits_and_unknown_formats_without_exposing_paths() {
        assert_eq!(classify_error("payload exceeds limit"), "LIMIT_EXCEEDED");
        assert_eq!(
            classify_error("Unknown Palworld .sav compression: 0x1/0x2"),
            "UNSUPPORTED_FORMAT"
        );
        assert_eq!(classify_error("Invalid GVAS magic."), "CORRUPT_SAVE");
    }

    #[test]
    fn browser_decompressed_limit_is_lower_than_the_native_guard() {
        assert_eq!(MAX_DECOMPRESSED_BYTES, 384 * 1024 * 1024);
    }
}
