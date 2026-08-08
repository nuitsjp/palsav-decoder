// Level.sav extractor ported from tools/save-data-cli/node-save-tool/lib.mjs.
// After Oodle/zlib decompression, it scans only as far as
// worldSaveData.CharacterSaveParameterMap and reads the required RawData without converting all GVAS data to JSON.
#[cfg(test)]
use crate::implementation::model::CacheCharacter;

pub mod base_camps;
pub mod decompress;
pub mod gvas;
pub mod level_meta;
pub mod player_relics;
pub mod world;

/// Equivalent to lib.mjs extractCharacterCache; returns only the characters used by agent-core.
/// Error messages intentionally match the TypeScript implementation.
#[cfg(test)]
fn extract_characters(level_sav_path: &str) -> Result<Vec<CacheCharacter>, String> {
    let sav_bytes = std::fs::read(level_sav_path).map_err(|error| error.to_string())?;
    let decompressed = decompress::decompress_sav(&sav_bytes)?;
    gvas::extract_characters_from_gvas(&decompressed.payload)
}

#[cfg(test)]
mod tests {
    use super::gvas::test_fixture::{
        build_level_sav, expected_standard_characters, standard_world_characters,
    };
    use super::*;

    #[test]
    fn extracts_the_character_list_from_a_file() {
        let path = std::env::temp_dir().join(format!(
            "agent-core-rs-extractor-test-{}.sav",
            std::process::id()
        ));
        std::fs::write(
            &path,
            build_level_sav(&standard_world_characters(), "double-zlib"),
        )
        .unwrap();
        let result = extract_characters(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert_eq!(result.unwrap(), expected_standard_characters());
    }

    #[test]
    fn returns_an_error_for_a_missing_file() {
        assert!(extract_characters("Z:\\no\\such\\Level.sav").is_err());
    }
}
