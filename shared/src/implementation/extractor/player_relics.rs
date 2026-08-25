// Extracts relic and journal acquisition state from Players/<PlayerUId>.sav.
// Ported from tools/save-data-cli/node-save-tool/player-relics.mjs.
// Error strings, ordering, and deduplication match JavaScript because the resulting JSON payloads
// are compared byte for byte.
#[cfg(test)]
use super::decompress::decompress_sav;
use super::gvas::GvasReader;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

const LEGACY_RELIC_PROPERTY: &str = "RelicObtainForInstanceFlag";
const RELICS_BY_TYPE_PROPERTY: &str = "RelicObtainForInstanceFlagByType";
const NOTES_PROPERTY: &str = "NoteObtainForInstanceFlag";
const ITEM_PICKUPS_PROPERTY: &str = "ItemPickupObtainForInstanceFlag";
const FAST_TRAVEL_PROPERTY: &str = "FastTravelPointUnlockFlag";
const DIMENSION_STORAGE_ARRAY_PROPERTY: &str = "SaveParameterArray";
const DIMENSION_STORAGE_STRUCT: &str = "PalDimensionPalStorageSaveParameter";

/// Return value of extractPlayerRelicsFromGvas: { relics, notes, ruins }.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRelics {
    /// Relic type to acquired placement GUIDs in ascending order; key order matches toStableDocument.
    pub relics: Map<String, Value>,
    /// Acquired journal IDs in ascending order.
    pub notes: Vec<String>,
    /// Acquired ancient ruin placement GUIDs in ascending order.
    pub ruins: Vec<String>,
    /// Unlocked fast-travel placement GUIDs in ascending order.
    pub fast_travel_point_ids: Vec<String>,
}

/// Return value of extractPlayerRelicState. JSON keys and ordering match JavaScript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerRelicState {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "relicsByType")]
    pub relics_by_type: Map<String, Value>,
    #[serde(rename = "noteIds")]
    pub note_ids: Vec<String>,
    #[serde(rename = "itemPickupGuids")]
    pub item_pickup_guids: Vec<String>,
    #[serde(rename = "fastTravelPointIds")]
    pub fast_travel_point_ids: Vec<String>,
}

impl From<PlayerRelicState> for crate::implementation::model::PlayerRelicState {
    fn from(value: PlayerRelicState) -> Self {
        Self {
            schema_version: value.schema_version,
            relics_by_type: value.relics_by_type,
            note_ids: value.note_ids,
            item_pickup_guids: value.item_pickup_guids,
            fast_travel_point_ids: value.fast_travel_point_ids,
        }
    }
}

/// Equivalent to player-relics.mjs extractPlayerRelicsFromGvas.
pub fn extract_player_relics_from_gvas(gvas_payload: &[u8]) -> Result<PlayerRelics, String> {
    let mut reader = GvasReader::new(gvas_payload);
    reader.read_header()?;
    enter_struct_property(&mut reader, "SaveData")?;
    enter_struct_property(&mut reader, "RecordData")?;

    let mut relics: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut legacy_relics: BTreeSet<String> = BTreeSet::new();
    let mut notes: Vec<String> = Vec::new();
    let mut ruins: BTreeSet<String> = BTreeSet::new();
    let mut fast_travel_point_ids: BTreeSet<String> = BTreeSet::new();

    while !reader.end() {
        let property_name = reader.read_fstring()?;
        if property_name == "None" {
            break;
        }

        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        match normalize_property_name(&property_name) {
            LEGACY_RELIC_PROPERTY => {
                legacy_relics = read_name_bool_map(&mut reader, &type_name)?;
            }
            RELICS_BY_TYPE_PROPERTY => {
                merge_relics(&mut relics, read_relics_by_type(&mut reader, &type_name)?);
            }
            NOTES_PROPERTY => {
                // Journals use row names such as Day5 rather than placement GUIDs.
                notes = read_note_flag_map(&mut reader, &type_name)?;
            }
            ITEM_PICKUPS_PROPERTY => {
                // Ancient ruins use placement GUIDs and share relic GUID normalization.
                ruins = read_name_bool_map(&mut reader, &type_name)?;
            }
            FAST_TRAVEL_PROPERTY => {
                fast_travel_point_ids =
                    read_guid_bool_map(&mut reader, &type_name, "fast travel flags")?;
            }
            _ => reader.skip_property(&type_name, size)?,
        }
    }

    if !legacy_relics.is_empty() {
        add_relics(&mut relics, "CapturePower", legacy_relics);
    }

    // JavaScript [...notes].sort() uses default UTF-16 code unit ordering.
    notes.sort_by(|left, right| compare_utf16(left, right));

    Ok(PlayerRelics {
        relics: to_stable_document(relics),
        notes,
        // Byte ordering in BTreeSet matches default JavaScript ordering for lowercase hex.
        ruins: ruins.into_iter().collect(),
        fast_travel_point_ids: fast_travel_point_ids.into_iter().collect(),
    })
}

/// Players 配下へ共存する次元パルボックス保存を、配列要素を展開せずに識別する。
/// 通常のプレイヤー保存はルートに SaveData を持つ一方、この保存は
/// SaveParameterArray<PalDimensionPalStorageSaveParameter> だけを持つ。
pub fn is_dimension_pal_storage_gvas(gvas_payload: &[u8]) -> Result<bool, String> {
    let mut reader = GvasReader::new(gvas_payload);
    reader.read_header()?;
    while !reader.end() {
        let property_name = reader.read_fstring()?;
        if property_name == "None" {
            return Ok(false);
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if normalize_property_name(&property_name) != DIMENSION_STORAGE_ARRAY_PROPERTY {
            reader.skip_property(&type_name, size)?;
            continue;
        }
        if type_name != "ArrayProperty" {
            return Ok(false);
        }
        let element_type = reader.read_fstring()?;
        reader.read_optional_guid_string()?;
        if element_type != "StructProperty" {
            return Ok(false);
        }
        reader.read_u32()?;
        let metadata_name = reader.read_fstring()?;
        let metadata_type = reader.read_fstring()?;
        reader.read_u64()?;
        let struct_type = reader.read_fstring()?;
        reader.read_guid_string()?;
        reader.read_optional_guid_string()?;
        return Ok(
            normalize_property_name(&metadata_name) == DIMENSION_STORAGE_ARRAY_PROPERTY
                && metadata_type == "StructProperty"
                && struct_type == DIMENSION_STORAGE_STRUCT,
        );
    }
    Ok(false)
}

/// Equivalent to player-relics.mjs extractPlayerRelicState.
#[cfg(test)]
fn extract_player_relic_state(player_sav_path: &str) -> Result<PlayerRelicState, String> {
    let sav_bytes = std::fs::read(player_sav_path).map_err(|error| error.to_string())?;
    let decompressed = decompress_sav(&sav_bytes)?;
    let PlayerRelics {
        relics,
        notes,
        ruins,
        fast_travel_point_ids,
    } = extract_player_relics_from_gvas(&decompressed.payload)?;
    Ok(PlayerRelicState {
        schema_version: 1,
        relics_by_type: relics,
        note_ids: notes,
        item_pickup_guids: ruins,
        fast_travel_point_ids,
    })
}

fn enter_struct_property(reader: &mut GvasReader, target_name: &str) -> Result<(), String> {
    while !reader.end() {
        let property_name = reader.read_fstring()?;
        if property_name == "None" {
            break;
        }

        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if normalize_property_name(&property_name) != target_name {
            reader.skip_property(&type_name, size)?;
            continue;
        }

        if type_name != "StructProperty" {
            return Err(format!(
                "Expected StructProperty for {target_name}, got {type_name}."
            ));
        }

        reader.read_fstring()?;
        reader.read_guid_string()?;
        reader.read_optional_guid_string()?;
        return Ok(());
    }

    Err(format!("{target_name} was not found."))
}

fn read_relics_by_type(
    reader: &mut GvasReader,
    type_name: &str,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    if type_name != "ArrayProperty" {
        return Err(format!(
            "Expected ArrayProperty for {RELICS_BY_TYPE_PROPERTY}, got {type_name}."
        ));
    }

    let element_type = reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    if element_type != "StructProperty" {
        return Err(format!(
            "Expected StructProperty elements for {RELICS_BY_TYPE_PROPERTY}, got {element_type}."
        ));
    }

    let count = reader.read_u32()?;
    read_struct_array_header(reader)?;
    let mut relics: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for _ in 0..count {
        let mut relic_type: Option<String> = None;
        let mut flags: BTreeSet<String> = BTreeSet::new();

        while !reader.end() {
            let property_name = reader.read_fstring()?;
            if property_name == "None" {
                break;
            }

            let property_type = reader.read_fstring()?;
            let property_size = reader.read_u64()?;
            match normalize_property_name(&property_name) {
                "Type" => {
                    relic_type = normalize_relic_type(
                        reader.read_string_like_property(&property_type, property_size)?,
                    );
                }
                "Flags" => {
                    flags = read_name_bool_map(reader, &property_type)?;
                }
                _ => reader.skip_property(&property_type, property_size)?,
            }
        }

        if let Some(relic_type) = relic_type {
            if !flags.is_empty() {
                add_relics(&mut relics, &relic_type, flags);
            }
        }
    }

    Ok(relics)
}

fn read_struct_array_header(reader: &mut GvasReader) -> Result<(), String> {
    reader.read_fstring()?;
    let inner_type = reader.read_fstring()?;
    reader.read_u64()?;
    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;
    if inner_type != "StructProperty" {
        return Err(format!(
            "Expected StructProperty array metadata, got {inner_type}."
        ));
    }

    Ok(())
}

/// Reads relic acquisition flags (placement GUID to bool).
fn read_name_bool_map(
    reader: &mut GvasReader,
    type_name: &str,
) -> Result<BTreeSet<String>, String> {
    read_guid_bool_map(reader, type_name, "relic flags")
}

fn read_guid_bool_map(
    reader: &mut GvasReader,
    type_name: &str,
    label: &str,
) -> Result<BTreeSet<String>, String> {
    if type_name != "MapProperty" {
        return Err(format!(
            "Expected MapProperty for {label}, got {type_name}."
        ));
    }

    let key_type = reader.read_fstring()?;
    let value_type = reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    if key_type != "NameProperty" || value_type != "BoolProperty" {
        return Err(format!(
            "Unexpected {} flag map types: {key_type}/{value_type}.",
            label.trim_end_matches(" flags")
        ));
    }

    let removed_count = reader.read_u32()?;
    for _ in 0..removed_count {
        reader.read_fstring()?;
    }

    let count = reader.read_u32()?;
    let mut result = BTreeSet::new();
    for _ in 0..count {
        let guid = normalize_relic_guid(&reader.read_fstring()?);
        let obtained = reader.read_byte()? != 0;
        if let Some(guid) = guid {
            if obtained {
                result.insert(guid);
            }
        }
    }

    Ok(result)
}

/// Reads journal acquisition flags (row name to bool). Row names are not normalized as GUIDs.
/// Deduplication preserves insertion order like JavaScript Set; the caller performs final sorting.
fn read_note_flag_map(reader: &mut GvasReader, type_name: &str) -> Result<Vec<String>, String> {
    if type_name != "MapProperty" {
        return Err(format!(
            "Expected MapProperty for note flags, got {type_name}."
        ));
    }

    let key_type = reader.read_fstring()?;
    let value_type = reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    if key_type != "NameProperty" || value_type != "BoolProperty" {
        return Err(format!(
            "Unexpected note flag map types: {key_type}/{value_type}."
        ));
    }

    let removed_count = reader.read_u32()?;
    for _ in 0..removed_count {
        reader.read_fstring()?;
    }

    let count = reader.read_u32()?;
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for _ in 0..count {
        let note_id = reader.read_fstring()?;
        let obtained = reader.read_byte()? != 0;
        if !note_id.is_empty() && obtained && seen.insert(note_id.clone()) {
            result.push(note_id);
        }
    }

    Ok(result)
}

/// Equivalent to JavaScript /^(.*)_\d+$/. A dot does not match a newline, so a newline before the
/// underscore prevents a match. This also intentionally handles a leading underscore differently
/// from gvas.rs normalize_name.
fn normalize_property_name(name: &str) -> &str {
    let digits_start = name.len() - name.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits_start == name.len() || digits_start == 0 {
        return name;
    }
    if name.as_bytes()[digits_start - 1] != b'_' {
        return name;
    }

    let prefix = &name[..digits_start - 1];
    if prefix.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return name;
    }

    prefix
}

/// Converts EPalRelicType::CapturePower to CapturePower; an empty value becomes None.
fn normalize_relic_type(value: Option<String>) -> Option<String> {
    let value = value?;
    if value.is_empty() {
        return None;
    }

    let normalized = match value.rfind("::") {
        Some(separator) => &value[separator + 2..],
        None => &value[..],
    };
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

/// Removes braces and hyphens, lowercases, and returns None unless the result is 32 hex digits.
fn normalize_relic_guid(value: &str) -> Option<String> {
    let compact: String = value
        .chars()
        .filter(|character| !matches!(character, '{' | '}' | '-'))
        .flat_map(char::to_lowercase)
        .collect();
    if compact.len() == 32
        && compact
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Some(compact)
    } else {
        None
    }
}

fn merge_relics(
    target: &mut BTreeMap<String, BTreeSet<String>>,
    source: BTreeMap<String, BTreeSet<String>>,
) {
    for (relic_type, guids) in source {
        add_relics(target, &relic_type, guids);
    }
}

fn add_relics(
    target: &mut BTreeMap<String, BTreeSet<String>>,
    relic_type: &str,
    guids: BTreeSet<String>,
) {
    target
        .entry(relic_type.to_string())
        .or_default()
        .extend(guids);
}

fn to_stable_document(relics: BTreeMap<String, BTreeSet<String>>) -> Map<String, Value> {
    let mut entries: Vec<(String, BTreeSet<String>)> = relics
        .into_iter()
        .filter(|(_, guids)| !guids.is_empty())
        .collect();
    entries.sort_by(|left, right| compare_locale_en(&left.0, &right.0));

    let mut document = Map::new();
    for (relic_type, guids) in entries {
        // Byte ordering in BTreeSet matches default JavaScript ordering for lowercase hex.
        let values: Vec<Value> = guids.into_iter().map(Value::String).collect();
        document.insert(relic_type, Value::Array(values));
    }

    document
}

/// Compares values using JavaScript default UTF-16 code unit ordering.
fn compare_utf16(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

/// Approximation of localeCompare(_, "en"). Real relic types are ASCII PascalCase, so a
/// case-insensitive comparison followed by lowercase-first tie-breaking is equivalent.
fn compare_locale_en(left: &str, right: &str) -> Ordering {
    let primary = compare_utf16(&left.to_lowercase(), &right.to_lowercase());
    if primary != Ordering::Equal {
        return primary;
    }

    for (left_char, right_char) in left.chars().zip(right.chars()) {
        if left_char == right_char {
            continue;
        }
        return if left_char.is_lowercase() {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }

    left.chars().count().cmp(&right.chars().count())
}

#[cfg(test)]
mod tests {
    use super::super::gvas::test_fixture::{
        deflate, fstring_byte_length, wrap_as_level_sav, write_header, GvasWriter,
    };
    use super::*;

    const MAGIC_PLZ: u32 = 0x005a_6c50;
    type RelicTypeFixture<'a> = (Option<&'a str>, Vec<(&'a str, bool)>);

    /// enterStructProperty does not use size, so zero is sufficient.
    fn open_struct(writer: &mut GvasWriter, name: &str, struct_type: &str) {
        writer.fstring(name);
        writer.fstring("StructProperty");
        writer.u64v(0);
        writer.fstring(struct_type);
        writer.zero_guid();
        writer.no_guid_flag();
    }

    fn write_flag_map(
        writer: &mut GvasWriter,
        name: &str,
        key_type: &str,
        value_type: &str,
        removed: &[&str],
        entries: &[(&str, bool)],
    ) {
        writer.fstring(name);
        writer.fstring("MapProperty");
        writer.u64v(0); // This path does not use MapProperty size.
        writer.fstring(key_type);
        writer.fstring(value_type);
        writer.no_guid_flag();
        writer.u32v(removed.len() as u32);
        for key in removed {
            writer.fstring(key);
        }
        writer.u32v(entries.len() as u32);
        for (key, obtained) in entries {
            writer.fstring(key);
            writer.u8v(if *obtained { 1 } else { 0 });
        }
    }

    fn write_float_property(writer: &mut GvasWriter, name: &str, value: f32) {
        writer.fstring(name);
        writer.fstring("FloatProperty");
        writer.u64v(4);
        writer.no_guid_flag();
        writer.f32v(value);
    }

    fn write_name_property(writer: &mut GvasWriter, name: &str, value: &str) {
        writer.fstring(name);
        writer.fstring("NameProperty");
        writer.u64v(fstring_byte_length(value));
        writer.no_guid_flag();
        writer.fstring(value);
    }

    /// Writes RelicObtainForInstanceFlagByType as a StructProperty array.
    fn write_relics_by_type(
        writer: &mut GvasWriter,
        name: &str,
        element_type: &str,
        inner_type: &str,
        entries: &[RelicTypeFixture<'_>],
    ) {
        writer.fstring(name);
        writer.fstring("ArrayProperty");
        writer.u64v(0); // This path does not use ArrayProperty size.
        writer.fstring(element_type);
        writer.no_guid_flag();
        writer.u32v(entries.len() as u32);
        // Array metadata consumed by readStructArrayHeader.
        writer.fstring(name);
        writer.fstring(inner_type);
        writer.u64v(0);
        writer.fstring("PalRelicObtainInfo");
        writer.zero_guid();
        writer.no_guid_flag();

        for (relic_type, flags) in entries {
            if let Some(relic_type) = relic_type {
                write_name_property(writer, "Type", relic_type);
            }
            // Unknown property for the skip path.
            write_float_property(writer, "UnknownRatio", 0.5);
            write_flag_map(writer, "Flags", "NameProperty", "BoolProperty", &[], flags);
            writer.fstring("None");
        }
    }

    fn payload_with_record_data(body: impl Fn(&mut GvasWriter)) -> Vec<u8> {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        open_struct(&mut writer, "SaveData", "PalWorldPlayerSaveData");
        open_struct(&mut writer, "RecordData", "PalPlayerRecordSaveData");
        body(&mut writer);
        writer.fstring("None");
        writer.into_bytes()
    }

    fn guid(nibble: char) -> String {
        std::iter::repeat_n(nibble, 32).collect()
    }

    fn expected(entries: &[(&str, &[&str])]) -> Map<String, Value> {
        let mut map = Map::new();
        for (relic_type, guids) in entries {
            map.insert(
                relic_type.to_string(),
                Value::Array(
                    guids
                        .iter()
                        .map(|guid| Value::String(guid.to_string()))
                        .collect(),
                ),
            );
        }
        map
    }

    /// Standard player save containing relics by type and journals.
    fn standard_payload() -> Vec<u8> {
        payload_with_record_data(|writer| {
            // Irrelevant property for the skipProperty path.
            write_float_property(writer, "PlayTime", 12.5);
            write_relics_by_type(
                writer,
                "RelicObtainForInstanceFlagByType_2",
                "StructProperty",
                "StructProperty",
                &[
                    (
                        Some("EPalRelicType::Sunreach"),
                        vec![
                            // Braces, hyphens, and uppercase characters are normalized.
                            ("{BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB}", true),
                            ("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", true),
                            // Unacquired entries are excluded.
                            ("cccccccccccccccccccccccccccccccc", false),
                            // Keys that are not GUIDs are excluded.
                            ("NotAGuid", true),
                        ],
                    ),
                    (
                        Some("EPalRelicType::CapturePower"),
                        vec![
                            ("dddddddddddddddddddddddddddddddd", true),
                            // Duplicate within the same type.
                            ("DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD", true),
                        ],
                    ),
                    // Entries without Type are discarded.
                    (None, vec![("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", true)]),
                ],
            );
            // Legacy format is merged into CapturePower.
            write_flag_map(
                writer,
                "RelicObtainForInstanceFlag",
                "NameProperty",
                "BoolProperty",
                &["removed-key"],
                &[
                    ("ffffffffffffffffffffffffffffffff", true),
                    ("dddddddddddddddddddddddddddddddd", true),
                ],
            );
            write_flag_map(
                writer,
                "NoteObtainForInstanceFlag_0",
                "NameProperty",
                "BoolProperty",
                &[],
                &[
                    ("Day5", true),
                    ("Day1", true),
                    ("Day5", true),
                    ("Day9", false),
                    ("", true),
                ],
            );
            write_flag_map(
                writer,
                "ItemPickupObtainForInstanceFlag_1",
                "NameProperty",
                "BoolProperty",
                &["removed-ruin"],
                &[
                    ("{99999999-9999-9999-9999-999999999999}", true),
                    ("11111111111111111111111111111111", true),
                    ("22222222222222222222222222222222", false),
                    ("NotAGuid", true),
                    ("11111111111111111111111111111111", true),
                ],
            );
        })
    }

    #[test]
    fn extracts_relics_by_type_and_journals() {
        let result = extract_player_relics_from_gvas(&standard_payload()).unwrap();
        let (a, b, d, f) = (guid('a'), guid('b'), guid('d'), guid('f'));
        assert_eq!(
            result.relics,
            expected(&[
                ("CapturePower", &[d.as_str(), f.as_str()][..]),
                ("Sunreach", &[a.as_str(), b.as_str()][..]),
            ])
        );
        assert_eq!(result.notes, vec!["Day1".to_string(), "Day5".to_string()]);
    }

    #[test]
    fn normalizes_ruin_guids_and_excludes_unacquired_or_invalid_values() {
        let result = extract_player_relics_from_gvas(&standard_payload()).unwrap();
        assert_eq!(result.ruins, vec![guid('1'), guid('9')]);
    }

    #[test]
    fn fast_travelは取得前0件から取得後の対象guidだけを増分抽出する() {
        let before = payload_with_record_data(|writer| {
            write_flag_map(
                writer,
                "FastTravelPointUnlockFlag",
                "NameProperty",
                "BoolProperty",
                &[],
                &[],
            );
        });
        let after = payload_with_record_data(|writer| {
            write_flag_map(
                writer,
                "FastTravelPointUnlockFlag_2",
                "NameProperty",
                "BoolProperty",
                &["dddddddddddddddddddddddddddddddd"],
                &[
                    ("BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB", true),
                    ("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", true),
                    ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true),
                    ("cccccccccccccccccccccccccccccccc", false),
                    ("not-a-guid", true),
                ],
            );
        });

        assert!(extract_player_relics_from_gvas(&before)
            .unwrap()
            .fast_travel_point_ids
            .is_empty());
        assert_eq!(
            extract_player_relics_from_gvas(&after)
                .unwrap()
                .fast_travel_point_ids,
            vec![guid('a'), guid('b')]
        );
    }

    #[test]
    fn fast_travelフラグのmap型が異なれば未対応扱いへ丸めず失敗する() {
        let wrong_property = payload_with_record_data(|writer| {
            writer.fstring("FastTravelPointUnlockFlag");
            writer.fstring("ArrayProperty");
            writer.u64v(0);
        });
        assert_eq!(
            extract_player_relics_from_gvas(&wrong_property).unwrap_err(),
            "Expected MapProperty for fast travel flags, got ArrayProperty."
        );
        let wrong_value = payload_with_record_data(|writer| {
            write_flag_map(
                writer,
                "FastTravelPointUnlockFlag",
                "NameProperty",
                "IntProperty",
                &[],
                &[],
            );
        });
        assert_eq!(
            extract_player_relics_from_gvas(&wrong_value).unwrap_err(),
            "Unexpected fast travel flag map types: NameProperty/IntProperty."
        );
    }

    #[test]
    fn rejects_ruin_flags_that_are_not_a_map() {
        let payload = payload_with_record_data(|writer| {
            writer.fstring("ItemPickupObtainForInstanceFlag");
            writer.fstring("ArrayProperty");
            writer.u64v(0);
        });
        assert_eq!(
            extract_player_relics_from_gvas(&payload).unwrap_err(),
            "Expected MapProperty for relic flags, got ArrayProperty."
        );
    }

    #[test]
    fn returns_empty_state_for_a_save_without_relics_journals_or_ruins() {
        let payload = payload_with_record_data(|writer| {
            write_float_property(writer, "PlayTime", 1.0);
        });
        let result = extract_player_relics_from_gvas(&payload).unwrap();
        assert!(result.relics.is_empty());
        assert!(result.notes.is_empty());
        assert!(result.ruins.is_empty());
    }

    #[test]
    fn omits_types_with_no_acquired_entries() {
        let payload = payload_with_record_data(|writer| {
            write_relics_by_type(
                writer,
                "RelicObtainForInstanceFlagByType",
                "StructProperty",
                "StructProperty",
                &[(
                    Some("EPalRelicType::Sunreach"),
                    vec![("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", false)],
                )],
            );
        });
        assert!(extract_player_relics_from_gvas(&payload)
            .unwrap()
            .relics
            .is_empty());
    }

    #[test]
    fn reads_relic_state_from_a_sav_file() {
        let sav = wrap_as_level_sav(&standard_payload(), "zlib");
        let path = std::env::temp_dir().join(format!(
            "agent-core-rs-player-relics-{}.sav",
            std::process::id()
        ));
        std::fs::write(&path, sav).unwrap();
        let state = extract_player_relic_state(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        let state = state.unwrap();
        assert_eq!(state.schema_version, 1);
        assert_eq!(state.note_ids, vec!["Day1".to_string(), "Day5".to_string()]);
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            format!(
                "{{\"schemaVersion\":1,\"relicsByType\":{{\"CapturePower\":[\"{d}\",\"{f}\"],\"Sunreach\":[\"{a}\",\"{b}\"]}},\"noteIds\":[\"Day1\",\"Day5\"],\"itemPickupGuids\":[\"{one}\",\"{nine}\"],\"fastTravelPointIds\":[]}}",
                a = guid('a'),
                b = guid('b'),
                d = guid('d'),
                f = guid('f'),
                one = guid('1'),
                nine = guid('9'),
            )
        );
    }

    #[test]
    fn returns_an_error_for_a_missing_file() {
        assert!(extract_player_relic_state("Z:\\no\\such\\player.sav").is_err());
    }

    #[test]
    fn rejects_save_data_with_the_wrong_type() {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        writer.fstring("SaveData");
        writer.fstring("ArrayProperty");
        writer.u64v(0);
        assert_eq!(
            extract_player_relics_from_gvas(&writer.into_bytes()).unwrap_err(),
            "Expected StructProperty for SaveData, got ArrayProperty."
        );
    }

    #[test]
    fn returns_an_error_when_record_data_is_absent() {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        open_struct(&mut writer, "SaveData", "PalWorldPlayerSaveData");
        write_float_property(&mut writer, "PlayTime", 1.0);
        writer.fstring("None");
        assert_eq!(
            extract_player_relics_from_gvas(&writer.into_bytes()).unwrap_err(),
            "RecordData was not found."
        );
    }

    #[test]
    fn rejects_a_relic_array_with_the_wrong_property_type() {
        let payload = payload_with_record_data(|writer| {
            writer.fstring("RelicObtainForInstanceFlagByType");
            writer.fstring("MapProperty");
            writer.u64v(0);
        });
        assert_eq!(
            extract_player_relics_from_gvas(&payload).unwrap_err(),
            "Expected ArrayProperty for RelicObtainForInstanceFlagByType, got MapProperty."
        );
    }

    #[test]
    fn rejects_a_relic_array_with_the_wrong_element_type() {
        let payload = payload_with_record_data(|writer| {
            writer.fstring("RelicObtainForInstanceFlagByType");
            writer.fstring("ArrayProperty");
            writer.u64v(0);
            writer.fstring("NameProperty");
            writer.no_guid_flag();
        });
        assert_eq!(
            extract_player_relics_from_gvas(&payload).unwrap_err(),
            "Expected StructProperty elements for RelicObtainForInstanceFlagByType, got NameProperty."
        );
    }

    #[test]
    fn rejects_a_relic_array_with_the_wrong_metadata_type() {
        let payload = payload_with_record_data(|writer| {
            write_relics_by_type(
                writer,
                "RelicObtainForInstanceFlagByType",
                "StructProperty",
                "ArrayProperty",
                &[],
            );
        });
        assert_eq!(
            extract_player_relics_from_gvas(&payload).unwrap_err(),
            "Expected StructProperty array metadata, got ArrayProperty."
        );
    }

    #[test]
    fn rejects_relic_flags_that_are_not_a_map() {
        let payload = payload_with_record_data(|writer| {
            writer.fstring("RelicObtainForInstanceFlag");
            writer.fstring("ArrayProperty");
            writer.u64v(0);
        });
        assert_eq!(
            extract_player_relics_from_gvas(&payload).unwrap_err(),
            "Expected MapProperty for relic flags, got ArrayProperty."
        );
    }

    #[test]
    fn rejects_relic_flags_with_wrong_key_or_value_types() {
        let payload = payload_with_record_data(|writer| {
            write_flag_map(
                writer,
                "RelicObtainForInstanceFlag",
                "StrProperty",
                "BoolProperty",
                &[],
                &[],
            );
        });
        assert_eq!(
            extract_player_relics_from_gvas(&payload).unwrap_err(),
            "Unexpected relic flag map types: StrProperty/BoolProperty."
        );
    }

    #[test]
    fn rejects_journal_flags_that_are_not_a_map() {
        let payload = payload_with_record_data(|writer| {
            writer.fstring("NoteObtainForInstanceFlag");
            writer.fstring("ArrayProperty");
            writer.u64v(0);
        });
        assert_eq!(
            extract_player_relics_from_gvas(&payload).unwrap_err(),
            "Expected MapProperty for note flags, got ArrayProperty."
        );
    }

    #[test]
    fn rejects_journal_flags_with_wrong_key_or_value_types() {
        let payload = payload_with_record_data(|writer| {
            write_flag_map(
                writer,
                "NoteObtainForInstanceFlag",
                "NameProperty",
                "IntProperty",
                &[],
                &[],
            );
        });
        assert_eq!(
            extract_player_relics_from_gvas(&payload).unwrap_err(),
            "Unexpected note flag map types: NameProperty/IntProperty."
        );
    }

    #[test]
    fn rejects_a_truncated_payload() {
        assert_eq!(
            extract_player_relics_from_gvas(&[]).unwrap_err(),
            "Unexpected end of GVAS payload."
        );
        let payload = standard_payload();
        assert_eq!(
            extract_player_relics_from_gvas(&payload[..payload.len() - 10]).unwrap_err(),
            "Unexpected end of GVAS payload."
        );
    }

    #[test]
    fn returns_an_error_without_panicking_for_corrupt_compressed_data() {
        let mut writer = GvasWriter::default();
        writer.u32v(64);
        writer.u32v(8);
        writer.u32v(MAGIC_PLZ | (0x31 << 24));
        writer.raw(&[0xA5u8; 8]);
        let path = std::env::temp_dir().join(format!(
            "agent-core-rs-player-relics-broken-{}.sav",
            std::process::id()
        ));
        std::fs::write(&path, writer.into_bytes()).unwrap();
        let result = extract_player_relic_state(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
        assert!(!deflate(b"x").is_empty());
    }

    #[test]
    fn removes_only_numeric_property_name_suffixes() {
        assert_eq!(
            normalize_property_name("NoteObtainForInstanceFlag_0"),
            "NoteObtainForInstanceFlag"
        );
        assert_eq!(normalize_property_name("Talent_HP"), "Talent_HP");
        assert_eq!(normalize_property_name("_5"), "");
        assert_eq!(normalize_property_name("Name_"), "Name_");
        assert_eq!(normalize_property_name("Plain"), "Plain");
        assert_eq!(normalize_property_name("A_1_2"), "A_1");
        assert_eq!(normalize_property_name("12"), "12");
    }

    #[test]
    fn keeps_only_the_final_component_of_a_namespaced_relic_type() {
        assert_eq!(
            normalize_relic_type(Some("EPalRelicType::Sunreach".to_string())),
            Some("Sunreach".to_string())
        );
        assert_eq!(
            normalize_relic_type(Some("Sunreach".to_string())),
            Some("Sunreach".to_string())
        );
        assert_eq!(
            normalize_relic_type(Some("EPalRelicType::".to_string())),
            None
        );
        assert_eq!(normalize_relic_type(Some(String::new())), None);
        assert_eq!(normalize_relic_type(None), None);
    }

    #[test]
    fn accepts_only_32_digit_lowercase_hex_relic_guids() {
        assert_eq!(
            normalize_relic_guid("{AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA}"),
            Some(guid('a'))
        );
        assert_eq!(normalize_relic_guid("NotAGuid"), None);
        assert_eq!(normalize_relic_guid(""), None);
        // Thirty-two characters, but not hexadecimal.
        assert_eq!(normalize_relic_guid(&guid('z')), None);
    }

    #[test]
    fn orders_type_keys_like_locale_compare() {
        assert_eq!(compare_locale_en("Alpha", "beta"), Ordering::Less);
        assert_eq!(compare_locale_en("alpha", "Alpha"), Ordering::Less);
        assert_eq!(compare_locale_en("Alpha", "Alpha"), Ordering::Equal);
        assert_eq!(compare_locale_en("Alphabet", "Alpha"), Ordering::Greater);
    }
}
