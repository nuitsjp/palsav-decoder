// Port of tools/save-data-cli/node-save-tool/level-meta.mjs.
// Reads the display world name from LevelMeta.sav (/Script/Pal.PalWorldBaseInfoSaveGame).
// Traversal order and skip rules match the TypeScript implementation.
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use super::decompress::decompress_sav;
use super::gvas::GvasReader;

const SAVE_DATA_PROPERTY: &str = "SaveData";
const SAVE_DATA_STRUCT: &str = "PalWorldBaseInfoSaveData";
const WORLD_NAME_PROPERTY: &str = "WorldName";

/// Reads only WorldName directly below PalWorldBaseInfoSaveData.
fn read_world_name_from_save_data(reader: &mut GvasReader) -> Result<Option<String>, String> {
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            return Ok(None);
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == WORLD_NAME_PROPERTY && type_name == "StrProperty" {
            reader.read_optional_guid_string()?;
            return Ok(Some(reader.read_fstring()?));
        }
        reader.skip_property(&type_name, size)?;
    }
    Ok(None)
}

/// Reads WorldName from a decompressed LevelMeta GVAS payload, or None if absent.
pub fn extract_world_name_from_gvas(payload: &[u8]) -> Result<Option<String>, String> {
    let mut reader = GvasReader::new(payload);
    reader.read_header()?;
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            return Ok(None);
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name != SAVE_DATA_PROPERTY || type_name != "StructProperty" {
            reader.skip_property(&type_name, size)?;
            continue;
        }
        let struct_type = reader.read_fstring()?;
        reader.read_guid_string()?;
        reader.read_optional_guid_string()?;
        if struct_type != SAVE_DATA_STRUCT {
            reader.skip(size)?;
            continue;
        }
        return read_world_name_from_save_data(&mut reader);
    }
    Ok(None)
}

/// Reads the world name from LevelMeta.sav in a save directory.
/// Missing or corrupt files return Err so the caller can apply its fallback.
#[cfg(test)]
fn extract_world_name(save_dir: &Path) -> Result<Option<String>, String> {
    let bytes = std::fs::read(save_dir.join("LevelMeta.sav")).map_err(|error| error.to_string())?;
    let decompressed = decompress_sav(&bytes)?;
    extract_world_name_from_gvas(&decompressed.payload)
}

#[cfg(test)]
mod tests {
    use super::super::gvas::test_fixture::{build_level_meta_payload, build_level_meta_sav};
    use super::*;

    #[test]
    fn reads_a_japanese_world_name_from_a_negative_length_utf16_string() {
        let payload = build_level_meta_payload(Some("スペルド"));
        assert_eq!(
            extract_world_name_from_gvas(&payload).unwrap(),
            Some("スペルド".to_string())
        );
    }

    #[test]
    fn reads_an_ascii_world_name() {
        let payload = build_level_meta_payload(Some("Autosave_W"));
        assert_eq!(
            extract_world_name_from_gvas(&payload).unwrap(),
            Some("Autosave_W".to_string())
        );
    }

    #[test]
    fn preserves_an_empty_world_name() {
        let payload = build_level_meta_payload(Some(""));
        assert_eq!(
            extract_world_name_from_gvas(&payload).unwrap(),
            Some(String::new())
        );
    }

    #[test]
    fn returns_none_when_save_data_has_no_world_name() {
        let payload = build_level_meta_payload(None);
        assert_eq!(extract_world_name_from_gvas(&payload).unwrap(), None);
    }

    #[test]
    fn reads_a_compressed_level_meta_from_a_directory() {
        let dir =
            std::env::temp_dir().join(format!("agent-core-rs-level-meta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("LevelMeta.sav"),
            build_level_meta_sav(Some("テストワールド"), "double-zlib"),
        )
        .unwrap();
        let result = extract_world_name(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(result.unwrap(), Some("テストワールド".to_string()));
    }

    #[test]
    fn returns_an_error_when_level_meta_is_missing() {
        assert!(extract_world_name(Path::new("Z:\\no\\such\\world")).is_err());
    }
}
