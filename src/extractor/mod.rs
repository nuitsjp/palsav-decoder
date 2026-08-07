// Level.sav extractor ported from tools/save-data-cli/node-save-tool/lib.mjs.
// After Oodle/zlib decompression, it scans only as far as
// worldSaveData.CharacterSaveParameterMap and reads the required RawData without converting all GVAS data to JSON.
use crate::model::{CacheCharacter, PlayerContainerIndex};

pub mod base_camps;
pub mod decompress;
pub mod gvas;
pub mod level_meta;
pub mod player_relics;
pub mod world;

/// Equivalent to lib.mjs extractCharacterCache; returns only the characters used by agent-core.
/// Error messages intentionally match the TypeScript implementation.
pub fn extract_characters(level_sav_path: &str) -> Result<Vec<CacheCharacter>, String> {
    let sav_bytes = std::fs::read(level_sav_path).map_err(|error| error.to_string())?;
    let decompressed = decompress::decompress_sav(&sav_bytes)?;
    gvas::extract_characters_from_gvas(&decompressed.payload)
}

/// Equivalent to lib.mjs extractPlayerContainers. Returns None when the Players directory is absent.
/// A failed individual player save only degrades location classification and does not fail extraction.
pub fn extract_player_containers(players_directory: &str) -> Option<PlayerContainerIndex> {
    let entries = std::fs::read_dir(players_directory).ok()?;
    let mut index = PlayerContainerIndex::default();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_sav = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("sav"));
        if !is_sav {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(decompressed) = decompress::decompress_sav(&bytes) else {
            continue;
        };
        let Ok((pal_storage, otomo)) = gvas::read_player_container_ids(&decompressed.payload)
        else {
            continue;
        };
        if let Some(value) = pal_storage {
            index.pal_storage_container_ids.push(value);
        }
        if let Some(value) = otomo {
            index.otomo_container_ids.push(value);
        }
    }
    Some(index)
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
