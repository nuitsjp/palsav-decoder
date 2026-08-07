// Base-camp extraction ported from tools/save-data-cli/node-save-tool/base-camps.mjs.
// Scans worldSaveData and reads only BaseCampSaveData, CharacterContainerSaveData,
// and MapObjectSaveData. Error strings intentionally match the JavaScript implementation.
use super::decompress::decompress_sav;
use super::gvas::GvasReader;
use crate::model::{JsNumber, WorldPoint};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const BASE_CAMP_PROPERTY: &str = "BaseCampSaveData";
const CHARACTER_CONTAINER_PROPERTY: &str = "CharacterContainerSaveData";
const MAP_OBJECT_PROPERTY: &str = "MapObjectSaveData";
/// Base-camp core object whose placement coordinates define the camp location.
const PAL_BOX_MAP_OBJECT_ID: &str = "PalBoxV2";
/// Offset of the Pal container ID in WorkerDirector RawData.
/// Confirmed against every camp in a real save containing eight camps.
const WORKER_DIRECTOR_CONTAINER_OFFSET: usize = 98;
const GUID_BYTES: usize = 16;
/// Container slot RawData stores [player_uid(16), instance_id(16)].
const SLOT_INSTANCE_ID_OFFSET: usize = 16;
const EMPTY_GUID: &str = "00000000000000000000000000000000";

/// Layout of MapObject Model.RawData. A camp uses the placement coordinates of its PalBoxV2
/// because the camp data itself has no coordinates.
const MAP_MODEL_BASE_CAMP_OFFSET: usize = 32;
/// Offset of translation (three doubles), confirmed by comparing candidate offsets with known
/// Palbox coordinates from a real save.
const MAP_MODEL_TRANSLATION_OFFSET: usize = 104;
const MAP_MODEL_MIN_LENGTH: usize = MAP_MODEL_TRANSLATION_OFFSET + 8 * 3;

/// Equivalent to base-camp-sync.ts ExtractedBaseCamp. containerId is an intermediate local value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedBaseCamp {
    pub base_camp_id: String,
    pub container_id: Option<String>,
    pub slot_num: Option<i64>,
    pub instance_ids: Vec<String>,
    pub world: Option<WorldPoint>,
}

/// Camp ID and world coordinates read from Palbox Model.RawData.
struct MapModel {
    base_camp_id: String,
    world: WorldPoint,
}

struct BaseEntry {
    base_camp_id: String,
    container_id: Option<String>,
}

struct ContainerEntry {
    container_id: Option<String>,
    slot_num: Option<i64>,
    instance_ids: Vec<String>,
}

/// JavaScript readGuid byte order, concatenated as lowercase hexadecimal without hyphens.
/// Returns None when out of range; JavaScript checks the length before calling.
fn read_guid(bytes: &[u8], offset: usize) -> Option<String> {
    const ORDER: [usize; 16] = [3, 2, 1, 0, 7, 6, 5, 4, 11, 10, 9, 8, 15, 14, 13, 12];
    let mut value = String::with_capacity(32);
    for index in ORDER {
        let byte = bytes.get(offset.checked_add(index)?)?;
        value.push_str(&format!("{byte:02x}"));
    }
    Some(value)
}

/// JavaScript normalizeGuid: removes hyphens and converts to lowercase.
fn normalize_guid(value: &str) -> String {
    value.replace('-', "").to_lowercase()
}

fn read_f64_le(bytes: &[u8], offset: usize) -> Option<f64> {
    let end = offset.checked_add(8)?;
    let slice = bytes.get(offset..end)?;
    let mut buffer = [0u8; 8];
    buffer.copy_from_slice(slice);
    Some(f64::from_le_bytes(buffer))
}

/// Reads the camp ID and world coordinates from Palbox Model.RawData.
fn parse_map_model_raw_data(raw_data: &[u8]) -> Option<MapModel> {
    if raw_data.len() < MAP_MODEL_MIN_LENGTH {
        return None;
    }

    let translation = MAP_MODEL_TRANSLATION_OFFSET;
    Some(MapModel {
        base_camp_id: read_guid(raw_data, MAP_MODEL_BASE_CAMP_OFFSET)?,
        world: WorldPoint {
            x: JsNumber(read_f64_le(raw_data, translation)?),
            y: JsNumber(read_f64_le(raw_data, translation + 8)?),
            z: JsNumber(read_f64_le(raw_data, translation + 16)?),
        },
    })
}

/// Reads the Pal container ID from WorkerDirector RawData, or None if unavailable.
fn parse_worker_director_container_id(raw_data: &[u8]) -> Option<String> {
    if raw_data.len() < WORKER_DIRECTOR_CONTAINER_OFFSET + GUID_BYTES {
        return None;
    }
    read_guid(raw_data, WORKER_DIRECTOR_CONTAINER_OFFSET)
}

/// Extracts camps and their assigned Pal instance IDs from worldSaveData.
///
/// Resolves each camp's Pal container through WorkerDirector and collects instance IDs from
/// CharacterContainerSaveData slots. Capacity varies by camp level, so SlotNum is returned as stored.
pub fn extract_base_camps_from_gvas(gvas_payload: &[u8]) -> Result<Vec<ExtractedBaseCamp>, String> {
    let mut reader = GvasReader::new(gvas_payload);
    reader.read_header()?;
    enter_world_save_data(&mut reader)?;

    let mut bases: Vec<BaseEntry> = Vec::new();
    let mut containers: HashMap<String, ContainerEntry> = HashMap::new();
    let mut pal_boxes: HashMap<String, WorldPoint> = HashMap::new();

    while !reader.end() {
        let property_name = reader.read_fstring()?;
        if property_name == "None" || property_name.is_empty() {
            break;
        }

        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        match property_name.as_str() {
            BASE_CAMP_PROPERTY => bases.extend(read_base_camps(&mut reader, &type_name)?),
            CHARACTER_CONTAINER_PROPERTY => {
                for container in read_character_containers(&mut reader, &type_name)? {
                    // JavaScript stores null container IDs in the map, but lookups only use non-null IDs.
                    if let Some(key) = container.container_id.clone() {
                        containers.insert(key, container);
                    }
                }
            }
            MAP_OBJECT_PROPERTY => {
                for box_model in read_pal_boxes(&mut reader, &type_name)? {
                    pal_boxes.insert(box_model.base_camp_id, box_model.world);
                }
            }
            _ => reader.skip_property(&type_name, size)?,
        }
    }

    Ok(bases
        .into_iter()
        .map(|base| {
            let container = base.container_id.as_ref().and_then(|id| containers.get(id));
            ExtractedBaseCamp {
                world: pal_boxes.get(&base.base_camp_id).cloned(),
                slot_num: container.and_then(|entry| entry.slot_num),
                instance_ids: container
                    .map(|entry| entry.instance_ids.clone())
                    .unwrap_or_default(),
                base_camp_id: base.base_camp_id,
                container_id: base.container_id,
            }
        })
        .collect())
}

/// Equivalent to base-camps.mjs extractBaseCampState; returns only camps because schemaVersion is constant.
pub fn extract_base_camp_state(level_sav_path: &str) -> Result<Vec<ExtractedBaseCamp>, String> {
    let sav_bytes = std::fs::read(level_sav_path).map_err(|error| error.to_string())?;
    let decompressed = decompress_sav(&sav_bytes)?;
    extract_base_camps_from_gvas(&decompressed.payload)
}

fn enter_world_save_data(reader: &mut GvasReader) -> Result<(), String> {
    while !reader.end() {
        let property_name = reader.read_fstring()?;
        if property_name == "None" {
            break;
        }

        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if property_name == "worldSaveData" && type_name == "StructProperty" {
            reader.read_fstring()?;
            reader.read_guid_string()?;
            reader.read_optional_guid_string()?;
            return Ok(());
        }

        reader.skip_property(&type_name, size)?;
    }

    Err("worldSaveData was not found.".to_string())
}

fn read_base_camps(reader: &mut GvasReader, type_name: &str) -> Result<Vec<BaseEntry>, String> {
    if type_name != "MapProperty" {
        return Err(format!(
            "Expected MapProperty for {BASE_CAMP_PROPERTY}, got {type_name}."
        ));
    }

    reader.read_fstring()?;
    reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    reader.read_u32()?;
    let count = reader.read_u32()?;

    let mut bases = Vec::new();
    for _ in 0..count {
        let base_camp_id = normalize_guid(&reader.read_guid_string()?);
        let mut container_id = None;
        for _ in 0..40 {
            let property_name = reader.read_fstring()?;
            if property_name == "None" || property_name.is_empty() {
                break;
            }

            let property_type = reader.read_fstring()?;
            let size = reader.read_u64()?;
            if property_name == "WorkerDirector" && property_type == "StructProperty" {
                container_id = read_worker_director(reader)?;
                continue;
            }

            reader.skip_property(&property_type, size)?;
        }
        bases.push(BaseEntry {
            base_camp_id,
            container_id,
        });
    }

    Ok(bases)
}

fn read_worker_director(reader: &mut GvasReader) -> Result<Option<String>, String> {
    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;

    let mut container_id = None;
    for _ in 0..20 {
        let property_name = reader.read_fstring()?;
        if property_name == "None" || property_name.is_empty() {
            break;
        }

        let property_type = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if property_name == "RawData" && property_type == "ArrayProperty" {
            let raw = read_byte_array(reader)?;
            container_id = parse_worker_director_container_id(&raw);
            continue;
        }

        reader.skip_property(&property_type, size)?;
    }

    Ok(container_id)
}

fn read_character_containers(
    reader: &mut GvasReader,
    type_name: &str,
) -> Result<Vec<ContainerEntry>, String> {
    if type_name != "MapProperty" {
        return Err(format!(
            "Expected MapProperty for {CHARACTER_CONTAINER_PROPERTY}, got {type_name}."
        ));
    }

    reader.read_fstring()?;
    reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    reader.read_u32()?;
    let count = reader.read_u32()?;

    let mut containers = Vec::new();
    for _ in 0..count {
        let container_id = read_container_key(reader)?;
        let mut slot_num = None;
        let mut instance_ids = Vec::new();
        for _ in 0..20 {
            let property_name = reader.read_fstring()?;
            if property_name == "None" || property_name.is_empty() {
                break;
            }

            let property_type = reader.read_fstring()?;
            let size = reader.read_u64()?;
            if property_name == "SlotNum" && property_type == "IntProperty" {
                slot_num = reader.read_int_like_property(&property_type, size)?;
                continue;
            }
            if property_name == "Slots" && property_type == "ArrayProperty" {
                instance_ids.extend(read_slots(reader, size)?);
                continue;
            }

            reader.skip_property(&property_type, size)?;
        }
        containers.push(ContainerEntry {
            container_id,
            slot_num,
            instance_ids,
        });
    }

    Ok(containers)
}

/// Scans MapObjectSaveData and extracts only Palbox camp ownership and coordinates.
fn read_pal_boxes(reader: &mut GvasReader, type_name: &str) -> Result<Vec<MapModel>, String> {
    if type_name != "ArrayProperty" {
        return Err(format!(
            "Expected ArrayProperty for {MAP_OBJECT_PROPERTY}, got {type_name}."
        ));
    }

    let array_type = reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    if array_type != "StructProperty" {
        return Err(format!(
            "Unexpected {MAP_OBJECT_PROPERTY} element type: {array_type}."
        ));
    }

    let count = reader.read_u32()?;
    reader.read_fstring()?;
    reader.read_fstring()?;
    reader.read_u64()?;
    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;

    let mut boxes = Vec::new();
    for _ in 0..count {
        let mut map_object_id = None;
        let mut model = None;
        for _ in 0..20 {
            let property_name = reader.read_fstring()?;
            if property_name == "None" || property_name.is_empty() {
                break;
            }

            let property_type = reader.read_fstring()?;
            let size = reader.read_u64()?;
            if property_name == "MapObjectId" {
                map_object_id = reader.read_string_like_property(&property_type, size)?;
                continue;
            }
            if property_name == "Model" && property_type == "StructProperty" {
                model = read_model_raw_data(reader)?;
                continue;
            }

            reader.skip_property(&property_type, size)?;
        }

        if map_object_id.as_deref() == Some(PAL_BOX_MAP_OBJECT_ID) {
            if let Some(model) = model {
                boxes.push(model);
            }
        }
    }

    Ok(boxes)
}

fn read_model_raw_data(reader: &mut GvasReader) -> Result<Option<MapModel>, String> {
    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;

    let mut parsed = None;
    for _ in 0..30 {
        let property_name = reader.read_fstring()?;
        if property_name == "None" || property_name.is_empty() {
            break;
        }

        let property_type = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if property_name == "RawData" && property_type == "ArrayProperty" {
            let raw = read_byte_array(reader)?;
            parsed = parse_map_model_raw_data(&raw);
            continue;
        }

        reader.skip_property(&property_type, size)?;
    }

    Ok(parsed)
}

fn read_container_key(reader: &mut GvasReader) -> Result<Option<String>, String> {
    let mut container_id = None;
    for _ in 0..10 {
        let property_name = reader.read_fstring()?;
        if property_name == "None" || property_name.is_empty() {
            break;
        }

        let property_type = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if property_type == "StructProperty" {
            let struct_type = reader.read_fstring()?;
            reader.read_guid_string()?;
            reader.read_optional_guid_string()?;
            if struct_type == "Guid" {
                container_id = Some(normalize_guid(&reader.read_guid_string()?));
            } else {
                reader.skip(size)?;
            }
            continue;
        }

        reader.skip_property(&property_type, size)?;
    }

    Ok(container_id)
}

fn read_slots(reader: &mut GvasReader, size: u64) -> Result<Vec<String>, String> {
    let array_type = reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    if array_type != "StructProperty" {
        reader.skip(size)?;
        return Ok(Vec::new());
    }

    let count = reader.read_u32()?;
    reader.read_fstring()?;
    reader.read_fstring()?;
    reader.read_u64()?;
    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;

    let mut instance_ids = Vec::new();
    for _ in 0..count {
        for _ in 0..10 {
            let property_name = reader.read_fstring()?;
            if property_name == "None" || property_name.is_empty() {
                break;
            }

            let property_type = reader.read_fstring()?;
            let field_size = reader.read_u64()?;
            if property_name == "RawData" && property_type == "ArrayProperty" {
                let raw = read_byte_array(reader)?;
                if raw.len() >= SLOT_INSTANCE_ID_OFFSET + GUID_BYTES {
                    if let Some(instance_id) = read_guid(&raw, SLOT_INSTANCE_ID_OFFSET) {
                        if instance_id != EMPTY_GUID {
                            instance_ids.push(instance_id);
                        }
                    }
                }
                continue;
            }

            reader.skip_property(&property_type, field_size)?;
        }
    }

    Ok(instance_ids)
}

fn read_byte_array(reader: &mut GvasReader) -> Result<Vec<u8>, String> {
    reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    let count = reader.read_u32()?;
    // Check u32-to-usize conversion instead of casting because it can fail on 32-bit targets.
    let count =
        usize::try_from(count).map_err(|_| "Unexpected end of GVAS payload.".to_string())?;
    Ok(reader.read_bytes(count)?.to_vec())
}

// ---------------------------------------------------------------------------
// Test fixtures for constructing synthetic GVAS payloads.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod fixture {
    use crate::extractor::gvas::test_fixture::{
        fstring_byte_length, guid_bytes_from_canonical, write_header, GvasWriter,
    };

    pub const CAMP_A: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    pub const CAMP_B: &str = "bbbbbbbb-0000-0000-0000-000000000002";
    pub const CAMP_C: &str = "cccccccc-0000-0000-0000-000000000003";
    pub const CONTAINER_A: &str = "11111111-1111-1111-1111-111111111111";
    pub const CONTAINER_B: &str = "22222222-2222-2222-2222-222222222222";
    pub const PAL_1: &str = "33333333-3333-3333-3333-333333333333";
    pub const PAL_2: &str = "44444444-4444-4444-4444-444444444444";
    pub const PAL_3: &str = "55555555-5555-5555-5555-555555555555";

    pub fn hex(canonical: &str) -> String {
        canonical.replace('-', "").to_lowercase()
    }

    /// Unknown property used to exercise the skip path. Size is four bytes: optGuid plus f32.
    pub fn write_float_property(writer: &mut GvasWriter, name: &str, value: f32) {
        writer.fstring(name);
        writer.fstring("FloatProperty");
        writer.u64v(4);
        writer.no_guid_flag();
        writer.f32v(value);
    }

    pub fn write_int_property(writer: &mut GvasWriter, name: &str, value: i32) {
        writer.fstring(name);
        writer.fstring("IntProperty");
        writer.u64v(4);
        writer.no_guid_flag();
        writer.i32v(value);
    }

    pub fn write_name_property(writer: &mut GvasWriter, name: &str, value: &str) {
        writer.fstring(name);
        writer.fstring("NameProperty");
        writer.u64v(fstring_byte_length(value));
        writer.no_guid_flag();
        writer.fstring(value);
    }

    pub fn write_guid_struct_property(writer: &mut GvasWriter, name: &str, canonical: &str) {
        writer.fstring(name);
        writer.fstring("StructProperty");
        writer.u64v(16);
        writer.fstring("Guid");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.guid(canonical);
    }

    pub fn write_byte_array_property(writer: &mut GvasWriter, name: &str, bytes: &[u8]) {
        writer.fstring(name);
        writer.fstring("ArrayProperty");
        writer.u64v(4 + bytes.len() as u64);
        writer.fstring("ByteProperty");
        writer.no_guid_flag();
        writer.u32v(bytes.len() as u32);
        writer.raw(bytes);
    }

    /// StructProperty header where size is the body length.
    pub fn write_struct_header(
        writer: &mut GvasWriter,
        name: &str,
        struct_type: &str,
        body_len: u64,
    ) {
        writer.fstring(name);
        writer.fstring("StructProperty");
        writer.u64v(body_len);
        writer.fstring(struct_type);
        writer.zero_guid();
        writer.no_guid_flag();
    }

    pub fn write_map_header(writer: &mut GvasWriter, key_type: &str, value_type: &str, count: u32) {
        writer.fstring(key_type);
        writer.fstring(value_type);
        writer.no_guid_flag();
        writer.u32v(0);
        writer.u32v(count);
    }

    /// Struct array header: arrayType/optGuid/count followed by name/type/size/structType/guid/optGuid.
    pub fn write_struct_array_header(writer: &mut GvasWriter, name: &str, count: u32) {
        writer.fstring("StructProperty");
        writer.no_guid_flag();
        writer.u32v(count);
        writer.fstring(name);
        writer.fstring("StructProperty");
        writer.u64v(0);
        writer.fstring(name);
        writer.zero_guid();
        writer.no_guid_flag();
    }

    /// WorkerDirector.RawData with the container GUID at offset 98.
    pub fn worker_director_raw(container: &str, length: usize) -> Vec<u8> {
        let mut bytes = vec![0x11u8; length];
        let offset = super::WORKER_DIRECTOR_CONTAINER_OFFSET;
        if length >= offset + 16 {
            bytes[offset..offset + 16].copy_from_slice(&guid_bytes_from_canonical(container));
        }
        bytes
    }

    /// Model.RawData with camp GUID at offset 32 and translation as three doubles from offset 104.
    pub fn map_model_raw(base_camp: &str, world: (f64, f64, f64), length: usize) -> Vec<u8> {
        let mut bytes = vec![0x22u8; length];
        if length >= 48 {
            bytes[32..48].copy_from_slice(&guid_bytes_from_canonical(base_camp));
        }
        if length >= 128 {
            bytes[104..112].copy_from_slice(&world.0.to_le_bytes());
            bytes[112..120].copy_from_slice(&world.1.to_le_bytes());
            bytes[120..128].copy_from_slice(&world.2.to_le_bytes());
        }
        bytes
    }

    /// Slot RawData ([player_uid(16), instance_id(16)]). None is encoded as an empty GUID.
    pub fn slot_raw(instance_id: Option<&str>) -> Vec<u8> {
        let mut bytes = vec![0u8; 32];
        if let Some(instance_id) = instance_id {
            bytes[16..32].copy_from_slice(&guid_bytes_from_canonical(instance_id));
        }
        bytes
    }

    pub struct CampFixture {
        pub base_camp_id: &'static str,
        pub container: Option<&'static str>,
        /// WorkerDirector.RawData length; containerId is unavailable below 98 + 16 bytes.
        pub raw_len: usize,
    }

    pub fn write_base_camp_map(camps: &[CampFixture]) -> Vec<u8> {
        let mut writer = GvasWriter::default();
        write_map_header(
            &mut writer,
            "StructProperty",
            "StructProperty",
            camps.len() as u32,
        );
        for camp in camps {
            writer.raw(&guid_bytes_from_canonical(camp.base_camp_id));
            write_float_property(&mut writer, "AreaRange", 1.5);
            if let Some(container) = camp.container {
                let mut body = GvasWriter::default();
                write_byte_array_property(
                    &mut body,
                    "RawData",
                    &worker_director_raw(container, camp.raw_len),
                );
                body.fstring("None");
                let body = body.into_bytes();
                write_struct_header(
                    &mut writer,
                    "WorkerDirector",
                    "PalBaseCampWorkerDirectorSaveData",
                    body.len() as u64,
                );
                writer.raw(&body);
            }
            writer.fstring("None");
        }
        writer.into_bytes()
    }

    pub struct ContainerFixture {
        pub container_id: &'static str,
        pub slot_num: Option<i32>,
        pub slots: Vec<Option<&'static str>>,
    }

    pub fn write_container_map(containers: &[ContainerFixture]) -> Vec<u8> {
        let mut writer = GvasWriter::default();
        write_map_header(
            &mut writer,
            "StructProperty",
            "StructProperty",
            containers.len() as u32,
        );
        for container in containers {
            // Key structure.
            write_guid_struct_property(&mut writer, "ID", container.container_id);
            writer.fstring("None");

            // Value structure.
            if let Some(slot_num) = container.slot_num {
                write_int_property(&mut writer, "SlotNum", slot_num);
            }
            let mut slots = GvasWriter::default();
            write_struct_array_header(&mut slots, "Slots", container.slots.len() as u32);
            for slot in &container.slots {
                write_byte_array_property(&mut slots, "RawData", &slot_raw(*slot));
                slots.fstring("None");
            }
            let slots = slots.into_bytes();
            writer.fstring("Slots");
            writer.fstring("ArrayProperty");
            writer.u64v(slots.len() as u64);
            writer.raw(&slots);
            writer.fstring("None");
        }
        writer.into_bytes()
    }

    pub struct MapObjectFixture {
        pub map_object_id: &'static str,
        /// (camp GUID, coordinates, RawData length)
        pub model: Option<(&'static str, (f64, f64, f64), usize)>,
    }

    pub fn write_map_object_array(objects: &[MapObjectFixture]) -> Vec<u8> {
        let mut writer = GvasWriter::default();
        write_struct_array_header(&mut writer, "MapObjectSaveData", objects.len() as u32);
        for object in objects {
            write_name_property(&mut writer, "MapObjectId", object.map_object_id);
            if let Some((base_camp, world, length)) = object.model {
                let mut body = GvasWriter::default();
                write_byte_array_property(
                    &mut body,
                    "RawData",
                    &map_model_raw(base_camp, world, length),
                );
                body.fstring("None");
                let body = body.into_bytes();
                write_struct_header(
                    &mut writer,
                    "Model",
                    "PalMapObjectModelSaveData",
                    body.len() as u64,
                );
                writer.raw(&body);
            }
            write_float_property(&mut writer, "Hp", 100.0);
            writer.fstring("None");
        }
        writer.into_bytes()
    }

    /// Writes properties directly below worldSaveData and terminates them with None.
    pub fn world_body(properties: &[(&str, &str, u64, Vec<u8>)]) -> Vec<u8> {
        let mut writer = GvasWriter::default();
        for (name, type_name, size, body) in properties {
            writer.fstring(name);
            writer.fstring(type_name);
            writer.u64v(*size);
            writer.raw(body);
        }
        writer.fstring("None");
        writer.into_bytes()
    }

    /// Unknown FloatProperty that should be skipped, represented as (size, body).
    pub fn float_property_body(value: f32) -> (u64, Vec<u8>) {
        let mut writer = GvasWriter::default();
        writer.no_guid_flag();
        writer.f32v(value);
        (4, writer.into_bytes())
    }

    /// Wraps worldSaveData contents in a GVAS payload.
    pub fn wrap_world_save_data(body: &[u8]) -> Vec<u8> {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        // Add one property before worldSaveData to exercise the skip path.
        write_int_property(&mut writer, "Version", 3);
        let overhead = fstring_byte_length("PalWorldSaveData") + 16 + 1;
        writer.fstring("worldSaveData");
        writer.fstring("StructProperty");
        writer.u64v(overhead + body.len() as u64);
        writer.fstring("PalWorldSaveData");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.raw(body);
        writer.fstring("None");
        writer.into_bytes()
    }

    /// Standard world with three camps: A complete, B short Palbox RawData, C no WorkerDirector.
    pub fn standard_payload() -> Vec<u8> {
        let bases = write_base_camp_map(&[
            CampFixture {
                base_camp_id: CAMP_A,
                container: Some(CONTAINER_A),
                raw_len: 114,
            },
            CampFixture {
                base_camp_id: CAMP_B,
                container: Some(CONTAINER_B),
                raw_len: 114,
            },
            CampFixture {
                base_camp_id: CAMP_C,
                container: None,
                raw_len: 0,
            },
        ]);
        let containers = write_container_map(&[
            ContainerFixture {
                container_id: CONTAINER_A,
                slot_num: Some(20),
                slots: vec![Some(PAL_1), None, Some(PAL_2)],
            },
            ContainerFixture {
                container_id: CONTAINER_B,
                slot_num: Some(15),
                slots: vec![Some(PAL_3)],
            },
        ]);
        let objects = write_map_object_array(&[
            MapObjectFixture {
                map_object_id: "WoodChest",
                model: None,
            },
            MapObjectFixture {
                map_object_id: "PalBoxV2",
                model: Some((CAMP_A, (-123.5, 456.25, 78.125), 200)),
            },
            // A Palbox with short RawData is ignored.
            MapObjectFixture {
                map_object_id: "PalBoxV2",
                model: Some((CAMP_B, (1.0, 2.0, 3.0), 64)),
            },
        ]);

        let (float_size, float_body) = float_property_body(1.0);
        wrap_world_save_data(&world_body(&[
            ("GameTimeSaveData", "FloatProperty", float_size, float_body),
            ("BaseCampSaveData", "MapProperty", bases.len() as u64, bases),
            (
                "CharacterContainerSaveData",
                "MapProperty",
                containers.len() as u64,
                containers,
            ),
            (
                "MapObjectSaveData",
                "ArrayProperty",
                objects.len() as u64,
                objects,
            ),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::*;
    use super::*;
    use crate::extractor::gvas::test_fixture::{
        guid_bytes_from_canonical, wrap_as_level_sav, write_header, GvasWriter,
    };

    fn point(x: f64, y: f64, z: f64) -> Option<WorldPoint> {
        Some(WorldPoint {
            x: JsNumber(x),
            y: JsNumber(y),
            z: JsNumber(z),
        })
    }

    #[test]
    fn extracts_ids_slots_instances_and_coordinates_for_multiple_camps() {
        let camps = extract_base_camps_from_gvas(&standard_payload()).unwrap();
        assert_eq!(
            camps,
            vec![
                ExtractedBaseCamp {
                    base_camp_id: hex(CAMP_A),
                    container_id: Some(hex(CONTAINER_A)),
                    slot_num: Some(20),
                    instance_ids: vec![hex(PAL_1), hex(PAL_2)],
                    world: point(-123.5, 456.25, 78.125),
                },
                ExtractedBaseCamp {
                    base_camp_id: hex(CAMP_B),
                    container_id: Some(hex(CONTAINER_B)),
                    slot_num: Some(15),
                    instance_ids: vec![hex(PAL_3)],
                    world: None,
                },
                ExtractedBaseCamp {
                    base_camp_id: hex(CAMP_C),
                    container_id: None,
                    slot_num: None,
                    instance_ids: Vec::new(),
                    world: None,
                },
            ]
        );
    }

    #[test]
    fn excludes_instance_ids_from_empty_slots() {
        let camps = extract_base_camps_from_gvas(&standard_payload()).unwrap();
        assert!(!camps[0].instance_ids.contains(&EMPTY_GUID.to_string()));
        assert_eq!(camps[0].instance_ids.len(), 2);
    }

    #[test]
    fn returns_no_coordinates_or_slots_for_a_camp_without_palbox_or_container() {
        let bases = write_base_camp_map(&[CampFixture {
            base_camp_id: CAMP_A,
            container: Some(CONTAINER_A),
            raw_len: 114,
        }]);
        let payload = wrap_world_save_data(&world_body(&[(
            "BaseCampSaveData",
            "MapProperty",
            bases.len() as u64,
            bases,
        )]));
        let camps = extract_base_camps_from_gvas(&payload).unwrap();
        assert_eq!(camps.len(), 1);
        assert_eq!(camps[0].container_id, Some(hex(CONTAINER_A)));
        assert_eq!(camps[0].world, None);
        assert_eq!(camps[0].slot_num, None);
        assert!(camps[0].instance_ids.is_empty());
    }

    #[test]
    fn does_not_resolve_a_container_id_from_short_worker_director_raw_data() {
        let bases = write_base_camp_map(&[CampFixture {
            base_camp_id: CAMP_A,
            container: Some(CONTAINER_A),
            raw_len: 50,
        }]);
        let containers = write_container_map(&[ContainerFixture {
            container_id: CONTAINER_A,
            slot_num: Some(20),
            slots: vec![Some(PAL_1)],
        }]);
        let payload = wrap_world_save_data(&world_body(&[
            ("BaseCampSaveData", "MapProperty", bases.len() as u64, bases),
            (
                "CharacterContainerSaveData",
                "MapProperty",
                containers.len() as u64,
                containers,
            ),
        ]));
        let camps = extract_base_camps_from_gvas(&payload).unwrap();
        assert_eq!(camps[0].container_id, None);
        assert_eq!(camps[0].slot_num, None);
        assert!(camps[0].instance_ids.is_empty());
    }

    #[test]
    fn returns_an_empty_array_for_an_empty_camp_map() {
        let mut bases = GvasWriter::default();
        write_map_header(&mut bases, "StructProperty", "StructProperty", 0);
        let bases = bases.into_bytes();
        let payload = wrap_world_save_data(&world_body(&[(
            "BaseCampSaveData",
            "MapProperty",
            bases.len() as u64,
            bases,
        )]));
        assert_eq!(extract_base_camps_from_gvas(&payload).unwrap(), Vec::new());
    }

    #[test]
    fn returns_an_empty_array_when_the_camp_map_is_absent() {
        let payload = wrap_world_save_data(&world_body(&[]));
        assert_eq!(extract_base_camps_from_gvas(&payload).unwrap(), Vec::new());
    }

    #[test]
    fn rejects_a_camp_map_with_the_wrong_type() {
        let payload = wrap_world_save_data(&world_body(&[(
            "BaseCampSaveData",
            "ArrayProperty",
            0,
            Vec::new(),
        )]));
        assert_eq!(
            extract_base_camps_from_gvas(&payload).unwrap_err(),
            "Expected MapProperty for BaseCampSaveData, got ArrayProperty."
        );
    }

    #[test]
    fn rejects_a_container_map_with_the_wrong_type() {
        let payload = wrap_world_save_data(&world_body(&[(
            "CharacterContainerSaveData",
            "ArrayProperty",
            0,
            Vec::new(),
        )]));
        assert_eq!(
            extract_base_camps_from_gvas(&payload).unwrap_err(),
            "Expected MapProperty for CharacterContainerSaveData, got ArrayProperty."
        );
    }

    #[test]
    fn rejects_map_objects_with_the_wrong_type() {
        let payload = wrap_world_save_data(&world_body(&[(
            "MapObjectSaveData",
            "MapProperty",
            0,
            Vec::new(),
        )]));
        assert_eq!(
            extract_base_camps_from_gvas(&payload).unwrap_err(),
            "Expected ArrayProperty for MapObjectSaveData, got MapProperty."
        );
    }

    #[test]
    fn rejects_map_objects_whose_element_type_is_not_a_struct() {
        let mut objects = GvasWriter::default();
        objects.fstring("ByteProperty");
        objects.no_guid_flag();
        let objects = objects.into_bytes();
        let payload = wrap_world_save_data(&world_body(&[(
            "MapObjectSaveData",
            "ArrayProperty",
            objects.len() as u64,
            objects,
        )]));
        assert_eq!(
            extract_base_camps_from_gvas(&payload).unwrap_err(),
            "Unexpected MapObjectSaveData element type: ByteProperty."
        );
    }

    #[test]
    fn returns_an_error_when_world_save_data_is_absent() {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        writer.fstring("None");
        assert_eq!(
            extract_base_camps_from_gvas(&writer.into_bytes()).unwrap_err(),
            "worldSaveData was not found."
        );
    }

    #[test]
    fn returns_an_error_without_panicking_for_a_truncated_payload() {
        assert_eq!(
            extract_base_camps_from_gvas(&[]).unwrap_err(),
            "Unexpected end of GVAS payload."
        );

        let payload = standard_payload();
        assert_eq!(
            extract_base_camps_from_gvas(&payload[..payload.len() - 40]).unwrap_err(),
            "Unexpected end of GVAS payload."
        );
    }

    #[test]
    fn rejects_invalid_gvas_magic() {
        let mut writer = GvasWriter::default();
        writer.i32v(0x1234_5678);
        assert_eq!(
            extract_base_camps_from_gvas(&writer.into_bytes()).unwrap_err(),
            "Invalid GVAS magic."
        );
    }

    #[test]
    fn read_guid_returns_32_lowercase_hex_digits_or_none_when_out_of_range() {
        let bytes = guid_bytes_from_canonical(CAMP_A);
        assert_eq!(read_guid(&bytes, 0), Some(hex(CAMP_A)));
        assert_eq!(read_guid(&bytes, 1), None);
    }

    #[test]
    fn normalize_guid_removes_hyphens_and_lowercases() {
        assert_eq!(
            normalize_guid("AAAAAAAA-0000-0000-0000-00000000000B"),
            "aaaaaaaa00000000000000000000000b"
        );
    }

    #[test]
    fn short_raw_data_contains_neither_a_camp_model_nor_a_container_id() {
        assert!(parse_map_model_raw_data(&[0u8; 127]).is_none());
        assert!(parse_map_model_raw_data(&[0u8; 128]).is_some());
        assert!(parse_worker_director_container_id(&[0u8; 113]).is_none());
        assert!(parse_worker_director_container_id(&[0u8; 114]).is_some());
    }

    #[test]
    fn decompresses_a_level_save_and_extracts_camps_from_a_file() {
        let path = std::env::temp_dir().join(format!(
            "agent-core-rs-base-camps-test-{}.sav",
            std::process::id()
        ));
        std::fs::write(&path, wrap_as_level_sav(&standard_payload(), "double-zlib")).unwrap();
        let result = extract_base_camp_state(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        let camps = result.unwrap();
        assert_eq!(camps.len(), 3);
        assert_eq!(camps[0].base_camp_id, hex(CAMP_A));
        assert_eq!(camps[0].world, point(-123.5, 456.25, 78.125));
    }

    #[test]
    fn returns_an_error_for_a_missing_file() {
        assert!(extract_base_camp_state("Z:\\no\\such\\Level.sav").is_err());
    }
}
