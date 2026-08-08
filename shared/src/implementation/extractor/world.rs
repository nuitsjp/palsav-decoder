// Port of tools/save-data-cli/node-save-tool/world.mjs.
// Extracts map-renderable world state from Level.sav and Players/*.sav.
// Output, rounding, and ordering match the TypeScript implementation byte for byte.
use super::gvas::GvasReader;
use crate::implementation::model::{
    JsNumber, WorldCollectible, WorldEventPoint, WorldOverview, WorldPlayerPoint, WorldRaid,
};
use std::collections::HashMap;

/// Ticks per in-game day. GameDateTimeTicks uses 100 ns units.
const TICKS_PER_GAME_DAY: i64 = 864_000_000_000;

/// Minimum PalMapObjectModelSaveData.RawData length, matching world.mjs.
const MAP_MODEL_MIN_LENGTH: usize = 128;

/// Acceptance limits matching shared/src/sync.ts parseWorldOverview.
/// Contract violations return an error so the caller omits world, matching TypeScript.
const MAX_COLLECTIBLES: usize = 20_000;
const MAX_RAIDS: usize = 200;
const MAX_EVENTS: usize = 200;
const MAX_PLAYERS: usize = 200;
const MAX_RAID_REMAINING_SEC: i64 = 7 * 24 * 60 * 60;
const MAX_GAME_DAY: i64 = 1_000_000;
const MAX_DEFEATED_BOSSES: usize = 2000;

fn is_treasure(map_object_id: &str) -> bool {
    map_object_id
        .to_ascii_lowercase()
        .starts_with("treasurebox")
}

fn is_ore(map_object_id: &str) -> bool {
    let lower = map_object_id.to_ascii_lowercase();
    ["damagablerock", "damagablecoalrock", "damagablesulfurrock"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

fn treasure_kind(map_object_id: &str) -> &'static str {
    let lower = map_object_id.to_ascii_lowercase();
    if lower.contains("oilrig") {
        return "oilrig";
    }
    if lower.contains("fishing") {
        return "fishing";
    }
    "normal"
}

/// Equivalent to Math.round: .5 rounds toward positive infinity without precision-losing addition.
fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

fn round1(value: f64) -> f64 {
    js_round(value * 10.0) / 10.0
}

fn round2(value: f64) -> f64 {
    js_round(value * 100.0) / 100.0
}

/// Normalizes a canonical GUID to the contract's 32 lowercase hexadecimal digits.
fn normalize_guid(value: &str) -> String {
    value
        .chars()
        .filter(|character| *character != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

struct MapModel {
    instance_id: String,
    hp_current: i32,
    hp_max: i32,
    x: f64,
    y: f64,
    z: f64,
}

/// Decodes PalMapObjectModelSaveData.RawData using world.mjs offsets.
fn decode_map_model_raw_data(bytes: &[u8]) -> Result<Option<MapModel>, String> {
    if bytes.len() < MAP_MODEL_MIN_LENGTH {
        return Ok(None);
    }
    let mut reader = GvasReader::new(bytes);
    let instance_id = reader.read_guid_string()?;
    reader.skip(48)?; // concrete_model_instance_id + base_camp_id + group_id
    let hp_current = reader.read_i32()?;
    let hp_max = reader.read_i32()?;
    reader.skip(32)?; // initital_transform_cache.rotation (Quat)
    let x = reader.read_f64()?;
    let y = reader.read_f64()?;
    let z = reader.read_f64()?;
    Ok(Some(MapModel {
        instance_id,
        hp_current,
        hp_max,
        x,
        y,
        z,
    }))
}

fn open_world_save_data<'a>(reader: &mut GvasReader<'a>) -> Result<(), String> {
    reader.read_header()?;
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == "worldSaveData" {
            reader.read_fstring()?;
            reader.read_guid_string()?;
            reader.read_optional_guid_string()?;
            return Ok(());
        }
        reader.skip_property(&type_name, size)?;
    }
    Err("worldSaveData was not found.".to_string())
}

/// Reads a type-specific property header and returns a sub-buffer of exactly the declared size.
fn open_section_body<'a>(
    reader: &mut GvasReader<'a>,
    type_name: &str,
    size: u64,
) -> Result<GvasReader<'a>, String> {
    if type_name == "StructProperty" {
        reader.read_fstring()?;
        reader.read_guid_string()?;
        reader.read_optional_guid_string()?;
    } else if type_name == "ArrayProperty" {
        reader.read_fstring()?;
        reader.read_optional_guid_string()?;
    } else {
        reader.read_fstring()?;
        reader.read_fstring()?;
        reader.read_optional_guid_string()?;
    }
    Ok(GvasReader::new(reader.read_bytes(size as usize)?))
}

/// Reads the contents of ArrayProperty(ByteProperty).
fn read_byte_array<'a>(reader: &mut GvasReader<'a>) -> Result<&'a [u8], String> {
    reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    let count = reader.read_u32()?;
    reader.read_bytes(count as usize)
}

fn read_float_like_property(
    reader: &mut GvasReader,
    type_name: &str,
    size: u64,
) -> Result<Option<f64>, String> {
    if type_name != "FloatProperty" {
        reader.skip_property(type_name, size)?;
        return Ok(None);
    }
    reader.read_optional_guid_string()?;
    Ok(Some(f64::from(reader.read_f32()?)))
}

/// Scans MapObjectSaveData and builds collectible records and coordinate anchors.
fn read_map_objects(
    reader: &mut GvasReader,
    anchors: &mut HashMap<String, (f64, f64, f64)>,
) -> Result<Vec<WorldCollectible>, String> {
    let count = reader.read_u32()?;
    reader.read_fstring()?;
    reader.read_fstring()?;
    reader.read_u64()?;
    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;

    let mut collectibles = Vec::new();
    for _ in 0..count {
        let mut map_object_id = String::new();
        let mut model: Option<MapModel> = None;
        while !reader.end() {
            let name = reader.read_fstring()?;
            if name == "None" {
                break;
            }
            let type_name = reader.read_fstring()?;
            let size = reader.read_u64()?;
            if name == "MapObjectId" {
                map_object_id = reader
                    .read_string_like_property(&type_name, size)?
                    .unwrap_or_default();
                continue;
            }
            if name == "Model" && type_name == "StructProperty" {
                model = read_model_raw_data(reader)?;
                continue;
            }
            reader.skip_property(&type_name, size)?;
        }
        let Some(model) = model else { continue };
        anchors.insert(
            model.instance_id.clone(),
            (round1(model.x), round1(model.y), round1(model.z)),
        );
        let treasure = is_treasure(&map_object_id);
        if !treasure && !is_ore(&map_object_id) {
            continue;
        }
        let hp_ratio = if model.hp_max > 0 {
            round2(f64::from(model.hp_current) / f64::from(model.hp_max))
        } else {
            1.0
        };
        collectibles.push(WorldCollectible {
            kind: if treasure { "treasure" } else { "ore" }.to_string(),
            variant: if treasure {
                Some(treasure_kind(&map_object_id).to_string())
            } else {
                None
            },
            map_object_id,
            hp_ratio: JsNumber(hp_ratio),
            x: JsNumber(round1(model.x)),
            y: JsNumber(round1(model.y)),
            z: JsNumber(round1(model.z)),
        });
    }
    Ok(collectibles)
}

/// Extracts and decodes only RawData from Model(StructProperty).
fn read_model_raw_data(reader: &mut GvasReader) -> Result<Option<MapModel>, String> {
    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;

    let mut model = None;
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == "RawData" && type_name == "ArrayProperty" {
            model = decode_map_model_raw_data(read_byte_array(reader)?)?;
            continue;
        }
        reader.skip_property(&type_name, size)?;
    }
    Ok(model)
}

/// Reads InvaderSaveData(MapProperty): raid state by camp ID.
fn read_invaders(reader: &mut GvasReader) -> Result<Vec<WorldRaid>, String> {
    reader.read_u32()?;
    let count = reader.read_u32()?;
    let mut raids = Vec::new();
    for _ in 0..count {
        let base_camp_id = normalize_guid(&reader.read_guid_string()?);
        let mut invading = false;
        let mut elapsed = 0.0;
        let mut finish = 0.0;
        while !reader.end() {
            let name = reader.read_fstring()?;
            if name == "None" {
                break;
            }
            let type_name = reader.read_fstring()?;
            let size = reader.read_u64()?;
            match name.as_str() {
                "bIsInvading" => {
                    invading = reader
                        .read_bool_like_property(&type_name, size)?
                        .unwrap_or(false);
                }
                "CoolTimeElapsed" => {
                    elapsed = read_float_like_property(reader, &type_name, size)?.unwrap_or(0.0);
                }
                "CoolTimeFinish" => {
                    finish = read_float_like_property(reader, &type_name, size)?.unwrap_or(0.0);
                }
                _ => reader.skip_property(&type_name, size)?,
            }
        }
        raids.push(WorldRaid {
            base_camp_id,
            invading,
            remaining_sec: if finish > 0.0 {
                Some((js_round(finish - elapsed) as i64).max(0))
            } else {
                None
            },
        });
    }
    raids.sort_by(|left, right| left.base_camp_id.cmp(&right.base_camp_id));
    Ok(raids)
}

struct SupplyInfo {
    kind: String,
    instance_id: Option<String>,
}

/// Reads SupplySaveData(StructProperty): active supply events.
fn read_supplies(reader: &mut GvasReader) -> Result<Vec<SupplyInfo>, String> {
    let mut supplies = Vec::new();
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name != "SupplyInfos" || type_name != "ArrayProperty" {
            reader.skip_property(&type_name, size)?;
            continue;
        }
        let array_type = reader.read_fstring()?;
        reader.read_optional_guid_string()?;
        if array_type != "StructProperty" {
            reader.skip(size)?;
            continue;
        }
        let count = reader.read_u32()?;
        reader.read_fstring()?;
        reader.read_fstring()?;
        reader.read_u64()?;
        reader.read_fstring()?;
        reader.read_guid_string()?;
        reader.read_optional_guid_string()?;
        for _ in 0..count {
            let mut supply_type = String::new();
            let mut instance_id = None;
            while !reader.end() {
                let inner_name = reader.read_fstring()?;
                if inner_name == "None" {
                    break;
                }
                let inner_type = reader.read_fstring()?;
                let inner_size = reader.read_u64()?;
                if inner_name == "SupplyType" {
                    supply_type = reader
                        .read_string_like_property(&inner_type, inner_size)?
                        .unwrap_or_default();
                    continue;
                }
                if inner_name == "SupplyMapObjectId" {
                    instance_id = reader.read_guid_struct_property(&inner_type, inner_size)?;
                    continue;
                }
                reader.skip_property(&inner_type, inner_size)?;
            }
            let kind = supply_type.rsplit("::").next().unwrap_or("");
            supplies.push(SupplyInfo {
                kind: if kind.is_empty() { "Unknown" } else { kind }.to_string(),
                instance_id,
            });
        }
    }
    Ok(supplies)
}

/// Reads GameTimeSaveData(StructProperty): elapsed in-game days.
fn read_game_day(reader: &mut GvasReader) -> Result<i64, String> {
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == "GameDateTimeTicks" && type_name == "Int64Property" {
            reader.read_optional_guid_string()?;
            return Ok(reader.read_i64()? / TICKS_PER_GAME_DAY);
        }
        reader.skip_property(&type_name, size)?;
    }
    Ok(0)
}

pub struct LevelWorldState {
    pub collectibles: Vec<WorldCollectible>,
    pub raids: Vec<WorldRaid>,
    pub events: Vec<WorldEventPoint>,
    pub game_day: i64,
    pub partial: bool,
}

/// Reads Level.sav world state, equivalent to world.mjs extractWorldFromLevelGvas.
pub fn extract_world_from_level_gvas(payload: &[u8]) -> Result<LevelWorldState, String> {
    let mut reader = GvasReader::new(payload);
    open_world_save_data(&mut reader)?;

    let mut collectibles = Vec::new();
    let mut raids = Vec::new();
    let mut supplies = Vec::new();
    let mut game_day = 0;
    let mut partial = false;
    let mut anchors: HashMap<String, (f64, f64, f64)> = HashMap::new();

    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        let expected_type = match name.as_str() {
            "MapObjectSaveData" => Some("ArrayProperty"),
            "InvaderSaveData" => Some("MapProperty"),
            "SupplySaveData" | "GameTimeSaveData" => Some("StructProperty"),
            _ => None,
        };
        if expected_type != Some(type_name.as_str()) {
            reader.skip_property(&type_name, size)?;
            continue;
        }
        let mut body = open_section_body(&mut reader, &type_name, size)?;
        // Abandon only the failed section and continue scanning, matching TypeScript.
        match name.as_str() {
            "MapObjectSaveData" => match read_map_objects(&mut body, &mut anchors) {
                Ok(value) => collectibles = value,
                Err(_) => partial = true,
            },
            "InvaderSaveData" => match read_invaders(&mut body) {
                Ok(value) => raids = value,
                Err(_) => partial = true,
            },
            "SupplySaveData" => match read_supplies(&mut body) {
                Ok(value) => supplies = value,
                Err(_) => partial = true,
            },
            _ => match read_game_day(&mut body) {
                Ok(value) => game_day = value,
                Err(_) => partial = true,
            },
        }
    }

    let events = supplies
        .into_iter()
        .filter_map(|supply| {
            let instance_id = supply.instance_id?;
            let (x, y, z) = anchors.get(&instance_id)?;
            Some(WorldEventPoint {
                kind: supply.kind,
                x: JsNumber(*x),
                y: JsNumber(*y),
                z: JsNumber(*z),
            })
        })
        .collect();

    Ok(LevelWorldState {
        collectibles,
        raids,
        events,
        game_day,
        partial,
    })
}

/// Reads the last position from Players/<PlayerUId>.sav, equivalent to extractPlayerPointFromGvas.
pub fn extract_player_point_from_gvas(payload: &[u8]) -> Result<Option<WorldPlayerPoint>, String> {
    let mut reader = GvasReader::new(payload);
    reader.read_header()?;
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == "SaveData" {
            reader.read_fstring()?;
            reader.read_guid_string()?;
            reader.read_optional_guid_string()?;
            return read_player_save_data(&mut reader);
        }
        reader.skip_property(&type_name, size)?;
    }
    Err("SaveData was not found.".to_string())
}

fn read_player_save_data(reader: &mut GvasReader) -> Result<Option<WorldPlayerPoint>, String> {
    let mut player_uid: Option<String> = None;
    let mut translation: Option<(f64, f64, f64)> = None;
    let mut defeated_boss_spawner_ids: Vec<String> = Vec::new();
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == "PlayerUId" {
            player_uid = reader.read_guid_struct_property(&type_name, size)?;
            continue;
        }
        if name == "LastTransform" && type_name == "StructProperty" {
            translation = read_transform_translation(reader)?;
            continue;
        }
        if name == "RecordData" && type_name == "StructProperty" {
            defeated_boss_spawner_ids = read_defeated_bosses(reader)?;
            continue;
        }
        reader.skip_property(&type_name, size)?;
    }
    let (Some(player_uid), Some((x, y, z))) = (player_uid, translation) else {
        return Ok(None);
    };
    Ok(Some(WorldPlayerPoint {
        player_uid: normalize_guid(&player_uid),
        x: JsNumber(round1(x)),
        y: JsNumber(round1(y)),
        z: JsNumber(round1(z)),
        defeated_boss_spawner_ids,
    }))
}

/// Reads only true NormalBossDefeatFlag keys from RecordData(StructProperty).
fn read_defeated_bosses(reader: &mut GvasReader) -> Result<Vec<String>, String> {
    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;

    let mut ids: Vec<String> = Vec::new();
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == "NormalBossDefeatFlag" && type_name == "MapProperty" {
            reader.read_fstring()?;
            reader.read_fstring()?;
            reader.read_optional_guid_string()?;
            reader.read_u32()?;
            let count = reader.read_u32()?;
            for _ in 0..count {
                let key = reader.read_fstring()?;
                if reader.read_byte()? != 0 {
                    ids.push(key);
                }
            }
            continue;
        }
        reader.skip_property(&type_name, size)?;
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// Reads only Translation(Vector: three doubles) from Transform(StructProperty).
fn read_transform_translation(reader: &mut GvasReader) -> Result<Option<(f64, f64, f64)>, String> {
    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;

    let mut translation = None;
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == "Translation" {
            let struct_type = reader.read_fstring()?;
            reader.read_guid_string()?;
            reader.read_optional_guid_string()?;
            if struct_type != "Vector" {
                reader.skip(size)?;
                continue;
            }
            translation = Some((reader.read_f64()?, reader.read_f64()?, reader.read_f64()?));
            continue;
        }
        reader.skip_property(&type_name, size)?;
    }
    Ok(translation)
}

/// Applies the same rejection rules as shared/src/sync.ts parseWorldOverview so Rust does not send
/// a world value that TypeScript would omit.
pub(crate) fn validate_world_overview(overview: &WorldOverview) -> Result<(), String> {
    if overview.collectibles.len() > MAX_COLLECTIBLES
        || overview.raids.len() > MAX_RAIDS
        || overview.events.len() > MAX_EVENTS
        || overview.players.len() > MAX_PLAYERS
        || overview.game_day < 0
        || overview.game_day > MAX_GAME_DAY
    {
        return Err("world overview exceeds contract bounds".to_string());
    }
    for collectible in &overview.collectibles {
        if !is_valid_map_object_id(&collectible.map_object_id)
            || !collectible.hp_ratio.0.is_finite()
            || collectible.hp_ratio.0 < 0.0
            || collectible.hp_ratio.0 > 1.0
            || !is_finite_point(collectible.x, collectible.y, collectible.z)
        {
            return Err("world collectible violates contract".to_string());
        }
    }
    for raid in &overview.raids {
        if let Some(remaining) = raid.remaining_sec {
            if !(0..=MAX_RAID_REMAINING_SEC).contains(&remaining) {
                return Err("world raid violates contract".to_string());
            }
        }
    }
    for event in &overview.events {
        if !is_valid_event_kind(&event.kind) || !is_finite_point(event.x, event.y, event.z) {
            return Err("world event violates contract".to_string());
        }
    }
    for player in &overview.players {
        if !is_finite_point(player.x, player.y, player.z)
            || player.defeated_boss_spawner_ids.len() > MAX_DEFEATED_BOSSES
            || player
                .defeated_boss_spawner_ids
                .iter()
                .any(|id| !is_valid_boss_spawner_id(id))
        {
            return Err("world player violates contract".to_string());
        }
    }
    Ok(())
}

/// ^[A-Za-z0-9][A-Za-z0-9_\-.]{0,199}$
fn is_valid_boss_spawner_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() || value.len() > 200 {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn is_finite_point(x: JsNumber, y: JsNumber, z: JsNumber) -> bool {
    x.0.is_finite() && y.0.is_finite() && z.0.is_finite()
}

/// ^[A-Za-z0-9][A-Za-z0-9_\-.]{0,99}$
fn is_valid_map_object_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() || value.len() > 100 {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// ^[A-Za-z][A-Za-z0-9_]{0,49}$
fn is_valid_event_kind(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() || value.len() > 50 {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::super::gvas::test_fixture::GvasWriter;
    use super::*;

    const CAMP_A: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    const CAMP_B: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
    const SUPPLY_OBJECT: &str = "cccccccc-cccc-cccc-cccc-cccccccccccc";

    fn write_header(writer: &mut GvasWriter) {
        super::super::gvas::test_fixture::write_header(writer);
    }

    fn write_int_property(writer: &mut GvasWriter, name: &str, value: i32) {
        writer.fstring(name);
        writer.fstring("IntProperty");
        writer.u64v(4);
        writer.no_guid_flag();
        writer.i32v(value);
    }

    fn write_bool_property(writer: &mut GvasWriter, name: &str, value: bool) {
        writer.fstring(name);
        writer.fstring("BoolProperty");
        writer.u64v(0);
        writer.u8v(u8::from(value));
        writer.no_guid_flag();
    }

    fn write_float_property(writer: &mut GvasWriter, name: &str, value: f32) {
        writer.fstring(name);
        writer.fstring("FloatProperty");
        writer.u64v(4);
        writer.no_guid_flag();
        writer.f32v(value);
    }

    fn write_int64_property(writer: &mut GvasWriter, name: &str, value: i64) {
        writer.fstring(name);
        writer.fstring("Int64Property");
        writer.u64v(8);
        writer.no_guid_flag();
        writer.i64v(value);
    }

    fn write_name_property(writer: &mut GvasWriter, name: &str, value: &str) {
        writer.fstring(name);
        writer.fstring("NameProperty");
        writer.u64v(super::super::gvas::test_fixture::fstring_byte_length(value));
        writer.no_guid_flag();
        writer.fstring(value);
    }

    fn write_enum_property(writer: &mut GvasWriter, name: &str, enum_type: &str, value: &str) {
        writer.fstring(name);
        writer.fstring("EnumProperty");
        writer.u64v(super::super::gvas::test_fixture::fstring_byte_length(value));
        writer.fstring(enum_type);
        writer.no_guid_flag();
        writer.fstring(value);
    }

    fn write_guid_struct_property(writer: &mut GvasWriter, name: &str, canonical: &str) {
        writer.fstring(name);
        writer.fstring("StructProperty");
        writer.u64v(16);
        writer.fstring("Guid");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.guid(canonical);
    }

    struct MapObjectSpec {
        map_object_id: &'static str,
        instance_id: &'static str,
        hp_current: i32,
        hp_max: i32,
        x: f64,
        y: f64,
        z: f64,
    }

    fn map_model_raw_data(spec: &MapObjectSpec) -> Vec<u8> {
        let mut writer = GvasWriter::default();
        writer.guid(spec.instance_id);
        writer.raw(&[0u8; 48]);
        writer.i32v(spec.hp_current);
        writer.i32v(spec.hp_max);
        writer.raw(&[0u8; 32]);
        writer.f64v(spec.x);
        writer.f64v(spec.y);
        writer.f64v(spec.z);
        writer.into_bytes()
    }

    fn write_map_object_section(writer: &mut GvasWriter, specs: &[MapObjectSpec]) {
        let mut body = GvasWriter::default();
        body.u32v(specs.len() as u32);
        body.fstring("MapObjectSaveData");
        body.fstring("StructProperty");
        body.u64v(0);
        body.fstring("PalMapObjectSaveData");
        body.zero_guid();
        body.no_guid_flag();
        for spec in specs {
            write_name_property(&mut body, "MapObjectId", spec.map_object_id);
            let raw = map_model_raw_data(spec);
            let mut model = GvasWriter::default();
            model.fstring("RawData");
            model.fstring("ArrayProperty");
            model.u64v(4 + raw.len() as u64);
            model.fstring("ByteProperty");
            model.no_guid_flag();
            model.u32v(raw.len() as u32);
            model.raw(&raw);
            write_int_property(&mut model, "Padding", 1);
            model.fstring("None");
            let model_body = model.into_bytes();
            body.fstring("Model");
            body.fstring("StructProperty");
            body.u64v(model_body.len() as u64);
            body.fstring("PalMapObjectModel");
            body.zero_guid();
            body.no_guid_flag();
            body.raw(&model_body);
            write_int_property(&mut body, "Padding", 0);
            body.fstring("None");
        }
        let body_bytes = body.into_bytes();
        writer.fstring("MapObjectSaveData");
        writer.fstring("ArrayProperty");
        writer.u64v(body_bytes.len() as u64);
        writer.fstring("StructProperty");
        writer.no_guid_flag();
        writer.raw(&body_bytes);
    }

    struct InvaderSpec {
        base_camp_id: &'static str,
        invading: bool,
        elapsed: f32,
        finish: f32,
    }

    fn write_invader_section(writer: &mut GvasWriter, specs: &[InvaderSpec]) {
        let mut body = GvasWriter::default();
        body.u32v(0);
        body.u32v(specs.len() as u32);
        for spec in specs {
            body.guid(spec.base_camp_id);
            write_bool_property(&mut body, "bIsInvading", spec.invading);
            write_float_property(&mut body, "CoolTimeElapsed", spec.elapsed);
            write_float_property(&mut body, "CoolTimeFinish", spec.finish);
            write_int_property(&mut body, "Padding", 0);
            body.fstring("None");
        }
        let body_bytes = body.into_bytes();
        writer.fstring("InvaderSaveData");
        writer.fstring("MapProperty");
        writer.u64v(body_bytes.len() as u64);
        writer.fstring("StructProperty");
        writer.fstring("StructProperty");
        writer.no_guid_flag();
        writer.raw(&body_bytes);
    }

    fn write_supply_section(writer: &mut GvasWriter, supplies: &[(&str, &str)]) {
        let mut infos = GvasWriter::default();
        infos.u32v(supplies.len() as u32);
        infos.fstring("SupplyInfos");
        infos.fstring("StructProperty");
        infos.u64v(0);
        infos.fstring("PalSupplyInfo");
        infos.zero_guid();
        infos.no_guid_flag();
        for (supply_type, instance_id) in supplies {
            write_enum_property(&mut infos, "SupplyType", "EPalSupplyType", supply_type);
            write_guid_struct_property(&mut infos, "SupplyMapObjectId", instance_id);
            write_int_property(&mut infos, "Padding", 0);
            infos.fstring("None");
        }
        let infos_bytes = infos.into_bytes();
        let mut body = GvasWriter::default();
        body.fstring("SupplyInfos");
        body.fstring("ArrayProperty");
        body.u64v(infos_bytes.len() as u64);
        body.fstring("StructProperty");
        body.no_guid_flag();
        body.raw(&infos_bytes);
        body.fstring("None");
        let body_bytes = body.into_bytes();
        writer.fstring("SupplySaveData");
        writer.fstring("StructProperty");
        writer.u64v(body_bytes.len() as u64);
        writer.fstring("PalSupplySaveData");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.raw(&body_bytes);
    }

    fn write_game_time_section(writer: &mut GvasWriter, ticks: i64) {
        let mut body = GvasWriter::default();
        write_int64_property(&mut body, "GameDateTimeTicks", ticks);
        body.fstring("None");
        let body_bytes = body.into_bytes();
        writer.fstring("GameTimeSaveData");
        writer.fstring("StructProperty");
        writer.u64v(body_bytes.len() as u64);
        writer.fstring("PalGameTimeSaveData");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.raw(&body_bytes);
    }

    fn level_payload(
        map_objects: &[MapObjectSpec],
        invaders: &[InvaderSpec],
        supplies: &[(&str, &str)],
        ticks: Option<i64>,
    ) -> Vec<u8> {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        let mut world = GvasWriter::default();
        write_map_object_section(&mut world, map_objects);
        write_invader_section(&mut world, invaders);
        write_supply_section(&mut world, supplies);
        if let Some(ticks) = ticks {
            write_game_time_section(&mut world, ticks);
        }
        world.fstring("None");
        let world_bytes = world.into_bytes();
        writer.fstring("worldSaveData");
        writer.fstring("StructProperty");
        writer.u64v(world_bytes.len() as u64);
        writer.fstring("PalWorldSaveData");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.raw(&world_bytes);
        writer.fstring("None");
        writer.into_bytes()
    }

    #[test]
    fn extracts_collectibles_raids_supplies_and_game_day() {
        let payload = level_payload(
            &[
                MapObjectSpec {
                    map_object_id: "TreasureBox",
                    instance_id: "11111111-1111-1111-1111-111111111111",
                    hp_current: 100,
                    hp_max: 100,
                    x: -323172.6,
                    y: 215356.2,
                    z: -760.1,
                },
                MapObjectSpec {
                    map_object_id: "DamagableRock0002",
                    instance_id: "22222222-2222-2222-2222-222222222222",
                    hp_current: 50,
                    hp_max: 100,
                    x: 10.0,
                    y: -20.0,
                    z: 0.0,
                },
                MapObjectSpec {
                    map_object_id: "PalBoxV2",
                    instance_id: SUPPLY_OBJECT,
                    hp_current: 100,
                    hp_max: 100,
                    x: 100.0,
                    y: 200.0,
                    z: 300.0,
                },
            ],
            &[
                InvaderSpec {
                    base_camp_id: CAMP_B,
                    invading: false,
                    elapsed: 100.0,
                    finish: 3700.0,
                },
                InvaderSpec {
                    base_camp_id: CAMP_A,
                    invading: true,
                    elapsed: 0.0,
                    finish: 0.0,
                },
            ],
            &[("EPalSupplyType::Capsule", SUPPLY_OBJECT)],
            Some(TICKS_PER_GAME_DAY * 12),
        );

        let state = extract_world_from_level_gvas(&payload).unwrap();
        assert_eq!(state.collectibles.len(), 2);
        assert_eq!(state.collectibles[0].kind, "treasure");
        assert_eq!(state.collectibles[0].variant.as_deref(), Some("normal"));
        assert_eq!(state.collectibles[0].x, JsNumber(-323172.6));
        assert_eq!(state.collectibles[1].kind, "ore");
        assert_eq!(state.collectibles[1].variant, None);
        assert_eq!(state.collectibles[1].hp_ratio, JsNumber(0.5));
        // Raids are ordered by camp ID.
        assert_eq!(
            state.raids,
            vec![
                WorldRaid {
                    base_camp_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                    invading: true,
                    remaining_sec: None,
                },
                WorldRaid {
                    base_camp_id: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                    invading: false,
                    remaining_sec: Some(3600),
                },
            ],
        );
        assert_eq!(state.events.len(), 1);
        assert_eq!(state.events[0].kind, "Capsule");
        assert_eq!(state.events[0].x, JsNumber(100.0));
        assert_eq!(state.game_day, 12);
    }

    #[test]
    fn returns_zero_days_when_the_game_time_section_is_absent() {
        let payload = level_payload(&[], &[], &[], None);
        let state = extract_world_from_level_gvas(&payload).unwrap();
        assert_eq!(state.game_day, 0);
        assert!(state.collectibles.is_empty());
        assert!(state.events.is_empty());
    }

    #[test]
    fn reads_and_normalizes_the_player_last_position() {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        let mut save = GvasWriter::default();
        write_guid_struct_property(
            &mut save,
            "PlayerUId",
            "AAAAAAAA-0000-0000-0000-000000000001",
        );
        let mut translation = GvasWriter::default();
        translation.fstring("Translation");
        translation.fstring("StructProperty");
        translation.u64v(24);
        translation.fstring("Vector");
        translation.zero_guid();
        translation.no_guid_flag();
        translation.f64v(1000.4);
        translation.f64v(-2000.3);
        translation.f64v(30.0);
        write_int_property(&mut translation, "Padding", 0);
        translation.fstring("None");
        let transform_body = translation.into_bytes();
        save.fstring("LastTransform");
        save.fstring("StructProperty");
        save.u64v(transform_body.len() as u64);
        save.fstring("Transform");
        save.zero_guid();
        save.no_guid_flag();
        save.raw(&transform_body);
        // RecordData > NormalBossDefeatFlag: two true, one false, one duplicate.
        let mut entries = GvasWriter::default();
        entries.u32v(0);
        entries.u32v(4);
        for (key, defeated) in [
            ("grass_FBOSS_9", 1u8),
            ("81_1_grass_FBOSS_1", 1),
            ("snow_orange_I_BOSS", 0),
            ("81_1_grass_FBOSS_1", 1),
        ] {
            entries.fstring(key);
            entries.u8v(defeated);
        }
        let entries_bytes = entries.into_bytes();
        let mut record = GvasWriter::default();
        record.fstring("NormalBossDefeatFlag");
        record.fstring("MapProperty");
        record.u64v(entries_bytes.len() as u64);
        record.fstring("NameProperty");
        record.fstring("BoolProperty");
        record.no_guid_flag();
        record.raw(&entries_bytes);
        write_int_property(&mut record, "Padding", 0);
        record.fstring("None");
        let record_bytes = record.into_bytes();
        save.fstring("RecordData");
        save.fstring("StructProperty");
        save.u64v(record_bytes.len() as u64);
        save.fstring("PalPlayerDataRecordData");
        save.zero_guid();
        save.no_guid_flag();
        save.raw(&record_bytes);
        save.fstring("None");
        let save_bytes = save.into_bytes();
        writer.fstring("SaveData");
        writer.fstring("StructProperty");
        writer.u64v(save_bytes.len() as u64);
        writer.fstring("PalPlayerDataSaveGame");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.raw(&save_bytes);
        writer.fstring("None");

        let point = extract_player_point_from_gvas(&writer.into_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(point.player_uid, "aaaaaaaa000000000000000000000001");
        assert_eq!(point.x, JsNumber(1000.4));
        assert_eq!(point.y, JsNumber(-2000.3));
        assert_eq!(point.z, JsNumber(30.0));
        // Defeat flags include only true values and are deduplicated and sorted.
        assert_eq!(
            point.defeated_boss_spawner_ids,
            vec![
                "81_1_grass_FBOSS_1".to_string(),
                "grass_FBOSS_9".to_string()
            ],
        );
    }

    #[test]
    fn js_round_rounds_negative_midpoints_toward_positive_infinity() {
        // Math.round(-7601.5) is -7601, unlike Rust f64::round.
        assert_eq!(js_round(-7601.5), -7601.0);
        assert_eq!(round1(-760.15), -760.1);
        assert_eq!(round1(0.049_999_999_999_999_996), 0.0);
    }

    #[test]
    fn rejects_extracted_results_that_exceed_contract_bounds() {
        let overview = WorldOverview {
            collectibles: vec![],
            raids: vec![WorldRaid {
                base_camp_id: "a".repeat(32),
                invading: false,
                remaining_sec: Some(MAX_RAID_REMAINING_SEC + 1),
            }],
            events: vec![],
            players: vec![],
            game_day: 0,
        };
        assert!(validate_world_overview(&overview).is_err());
        let ok = WorldOverview {
            collectibles: vec![],
            raids: vec![],
            events: vec![],
            players: vec![],
            game_day: 0,
        };
        assert!(validate_world_overview(&ok).is_ok());
    }
}
