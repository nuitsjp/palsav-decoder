// GVAS subset reader ported from tools/save-data-cli/node-save-tool/lib.mjs GvasReader.
// Scans only as far as worldSaveData.CharacterSaveParameterMap and decodes RawData.
// Error strings intentionally match the TypeScript implementation.
use crate::implementation::model::CacheCharacter;

const ERR_EOF: &str = "Unexpected end of GVAS payload.";
const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;

/// lib.mjs normalizeName: removes a trailing underscore followed only by one or more digits.
/// A leading or trailing underscore by itself is unchanged.
pub fn normalize_name(name: &str) -> &str {
    let Some(index) = name.rfind('_') else {
        return name;
    };
    if index == 0 || index == name.len() - 1 {
        return name;
    }
    if name[index + 1..].bytes().all(|byte| byte.is_ascii_digit()) {
        &name[..index]
    } else {
        name
    }
}

/// lib.mjs formatGuid byte order: [3][2][1][0]-[7][6]-[5][4]-[11][10]-[9][8][15][14][13][12].
pub fn format_guid(bytes: &[u8]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[3], bytes[2], bytes[1], bytes[0],
        bytes[7], bytes[6],
        bytes[5], bytes[4],
        bytes[11], bytes[10],
        bytes[9], bytes[8], bytes[15], bytes[14], bytes[13], bytes[12],
    )
}

/// Euclidean division that rounds negative values to the correct calendar day like JavaScript Date.
fn div_floor(value: i64, divisor: i64) -> i64 {
    let quotient = value / divisor;
    if value % divisor < 0 {
        quotient - 1
    } else {
        quotient
    }
}

/// lib.mjs formatUtcTicks: .NET ticks → "YYYY-MM-DDTHH:MM:SS.fffffff+00:00"。
/// Milliseconds use BigInt-style division toward zero; fractions use ticks % 1e7 padded to seven digits.
pub fn format_utc_ticks(ticks: i64) -> String {
    let unix_ticks = ticks - UNIX_EPOCH_TICKS;
    let milliseconds_since_unix_epoch = unix_ticks / 10_000;
    let fractional_ticks = unix_ticks % 10_000_000;
    let days = div_floor(milliseconds_since_unix_epoch, 86_400_000);
    let ms_of_day = milliseconds_since_unix_epoch - days * 86_400_000;
    let (year, month, day) = crate::implementation::timefmt::civil_from_days(days);
    let hours = ms_of_day / 3_600_000;
    let minutes = ms_of_day % 3_600_000 / 60_000;
    let seconds = ms_of_day % 60_000 / 1000;
    let fraction = format!("{:0>7}", fractional_ticks.to_string());
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{fraction}+00:00")
}

/// Subset of lib.mjs GvasReader, excluding diagnostics-only methods.
pub struct GvasReader<'a> {
    buffer: &'a [u8],
    offset: usize,
}

impl<'a> GvasReader<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        GvasReader { buffer, offset: 0 }
    }

    pub fn end(&self) -> bool {
        self.offset >= self.buffer.len()
    }

    pub fn remaining(&self) -> usize {
        self.buffer.len().saturating_sub(self.offset)
    }

    fn ensure(&self, count: u64) -> Result<(), String> {
        if count > self.remaining() as u64 {
            return Err(ERR_EOF.to_string());
        }
        Ok(())
    }

    fn bounded_end(&self, size: u64) -> Result<usize, String> {
        let size = usize::try_from(size).map_err(|_| ERR_EOF.to_string())?;
        let end = self
            .offset
            .checked_add(size)
            .ok_or_else(|| ERR_EOF.to_string())?;
        if end > self.buffer.len() {
            return Err(ERR_EOF.to_string());
        }
        Ok(end)
    }

    pub fn skip(&mut self, count: u64) -> Result<(), String> {
        self.ensure(count)?;
        self.offset += count as usize;
        Ok(())
    }

    pub fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], String> {
        self.ensure(count as u64)?;
        let value = &self.buffer[self.offset..self.offset + count];
        self.offset += count;
        Ok(value)
    }

    pub fn read_byte(&mut self) -> Result<u8, String> {
        self.ensure(1)?;
        let value = self.buffer[self.offset];
        self.offset += 1;
        Ok(value)
    }

    pub fn read_u16(&mut self) -> Result<u16, String> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_i32(&mut self) -> Result<i32, String> {
        let bytes = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_u32(&mut self) -> Result<u32, String> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_i64(&mut self) -> Result<i64, String> {
        let bytes = self.read_bytes(8)?;
        Ok(i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub fn read_u64(&mut self) -> Result<u64, String> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub fn read_f32(&mut self) -> Result<f32, String> {
        let bytes = self.read_bytes(4)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_f64(&mut self) -> Result<f64, String> {
        let bytes = self.read_bytes(8)?;
        Ok(f64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// FString: zero length is empty; negative length is UTF-16LE without the final two-byte NUL;
    /// positive length is ASCII with high bits masked like Node Buffer and the final NUL removed.
    pub fn read_fstring(&mut self) -> Result<String, String> {
        let length = self.read_i32()?;
        if length == 0 {
            return Ok(String::new());
        }

        if length < 0 {
            let char_count = -(length as i64);
            let byte_count = (char_count as u64) * 2;
            self.ensure(byte_count)?;
            let bytes = self.read_bytes(byte_count as usize)?;
            let units: Vec<u16> = bytes[..bytes.len() - 2]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            return Ok(String::from_utf16_lossy(&units));
        }

        let length = length as usize;
        let bytes = self.read_bytes(length)?;
        Ok(bytes[..length - 1]
            .iter()
            .map(|&byte| (byte & 0x7f) as char)
            .collect())
    }

    pub fn read_guid_string(&mut self) -> Result<String, String> {
        let bytes = self.read_bytes(16)?;
        Ok(format_guid(bytes))
    }

    pub fn read_optional_guid_string(&mut self) -> Result<Option<String>, String> {
        if self.read_byte()? == 0 {
            Ok(None)
        } else {
            Ok(Some(self.read_guid_string()?))
        }
    }

    pub fn read_guid_struct_property(
        &mut self,
        type_name: &str,
        size: u64,
    ) -> Result<Option<String>, String> {
        if type_name != "StructProperty" {
            self.skip_property(type_name, size)?;
            return Ok(None);
        }

        let struct_type = self.read_fstring()?;
        self.read_guid_string()?;
        self.read_optional_guid_string()?;
        if struct_type == "Guid" {
            return Ok(Some(self.read_guid_string()?));
        }

        self.skip(size)?;
        Ok(None)
    }

    pub fn read_datetime_struct_property(
        &mut self,
        type_name: &str,
        size: u64,
    ) -> Result<Option<String>, String> {
        if type_name != "StructProperty" {
            self.skip_property(type_name, size)?;
            return Ok(None);
        }

        let struct_type = self.read_fstring()?;
        self.read_guid_string()?;
        self.read_optional_guid_string()?;
        if struct_type == "DateTime" && size == 8 {
            return Ok(Some(format_utc_ticks(self.read_i64()?)));
        }

        self.skip(size)?;
        Ok(None)
    }

    pub fn read_string_like_property(
        &mut self,
        type_name: &str,
        size: u64,
    ) -> Result<Option<String>, String> {
        match type_name {
            "StrProperty" | "NameProperty" => {
                self.read_optional_guid_string()?;
                let value = self.read_fstring()?;
                if value.trim().is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(value))
                }
            }
            "EnumProperty" => {
                self.read_fstring()?;
                self.read_optional_guid_string()?;
                Ok(Some(self.read_fstring()?))
            }
            "ByteProperty" => {
                let enum_type = self.read_fstring()?;
                self.read_optional_guid_string()?;
                if enum_type == "None" {
                    Ok(Some(self.read_byte()?.to_string()))
                } else {
                    Ok(Some(self.read_fstring()?))
                }
            }
            _ => {
                self.skip_property(type_name, size)?;
                Ok(None)
            }
        }
    }

    pub fn read_int_like_property(
        &mut self,
        type_name: &str,
        size: u64,
    ) -> Result<Option<i64>, String> {
        match type_name {
            "IntProperty" => {
                self.read_optional_guid_string()?;
                Ok(Some(self.read_i32()? as i64))
            }
            "ByteProperty" => {
                let enum_type = self.read_fstring()?;
                self.read_optional_guid_string()?;
                if enum_type == "None" {
                    return Ok(Some(self.read_byte()? as i64));
                }

                self.read_fstring()?;
                Ok(None)
            }
            _ => {
                self.skip_property(type_name, size)?;
                Ok(None)
            }
        }
    }

    pub fn read_bool_like_property(
        &mut self,
        type_name: &str,
        size: u64,
    ) -> Result<Option<bool>, String> {
        if type_name != "BoolProperty" {
            self.skip_property(type_name, size)?;
            return Ok(None);
        }

        let value = self.read_byte()? != 0;
        self.read_optional_guid_string()?;
        Ok(Some(value))
    }

    /// Reads only Name, Str, and Enum arrays and discards whitespace-only entries while preserving None.
    /// SlotId is StructProperty(PalCharacterSlotId) containing ContainerId
    /// (StructProperty PalContainerId -> ID: Guid) and SlotIndex(IntProperty).
    /// Port of lib.mjs readSlotIdStructProperty.
    pub fn read_slot_id_struct_property(
        &mut self,
        type_name: &str,
        size: u64,
    ) -> Result<(Option<String>, Option<i64>), String> {
        if type_name != "StructProperty" {
            self.skip_property(type_name, size)?;
            return Ok((None, None));
        }

        self.read_fstring()?;
        self.read_guid_string()?;
        self.read_optional_guid_string()?;
        let end = self.bounded_end(size)?;
        let mut container_id = None;
        let mut slot_index = None;
        while self.offset < end {
            let property_name = self.read_fstring()?;
            if property_name == "None" {
                break;
            }
            let property_type = self.read_fstring()?;
            let property_size = self.read_u64()?;
            if property_name == "ContainerId" && property_type == "StructProperty" {
                self.read_fstring()?;
                self.read_guid_string()?;
                self.read_optional_guid_string()?;
                let container_end = self.bounded_end(property_size)?;
                while self.offset < container_end {
                    let inner_name = self.read_fstring()?;
                    if inner_name == "None" {
                        break;
                    }
                    let inner_type = self.read_fstring()?;
                    let inner_size = self.read_u64()?;
                    if inner_name == "ID" {
                        container_id = self.read_guid_struct_property(&inner_type, inner_size)?;
                    } else {
                        self.skip_property(&inner_type, inner_size)?;
                    }
                }
                self.offset = container_end;
            } else if property_name == "SlotIndex" {
                slot_index = self.read_int_like_property(&property_type, property_size)?;
            } else {
                self.skip_property(&property_type, property_size)?;
            }
        }

        self.offset = end;
        Ok((container_id, slot_index))
    }

    pub fn read_string_array_property(
        &mut self,
        type_name: &str,
        size: u64,
    ) -> Result<Vec<String>, String> {
        if type_name != "ArrayProperty" {
            self.skip_property(type_name, size)?;
            return Ok(Vec::new());
        }

        let array_type = self.read_fstring()?;
        self.read_optional_guid_string()?;
        if array_type != "NameProperty"
            && array_type != "StrProperty"
            && array_type != "EnumProperty"
        {
            self.skip(size)?;
            return Ok(Vec::new());
        }

        let count = self.read_u32()?;
        let mut values = Vec::new();
        for _ in 0..count {
            let value = self.read_fstring()?;
            if !value.trim().is_empty() {
                values.push(value);
            }
        }

        Ok(values)
    }

    pub fn skip_property(&mut self, type_name: &str, size: u64) -> Result<(), String> {
        match type_name {
            "StructProperty" => {
                self.read_fstring()?;
                self.read_guid_string()?;
                self.read_optional_guid_string()?;
                self.skip(size)
            }
            "ArrayProperty" => {
                self.read_fstring()?;
                self.read_optional_guid_string()?;
                self.skip(size)
            }
            "MapProperty" => {
                self.read_fstring()?;
                self.read_fstring()?;
                self.read_optional_guid_string()?;
                self.skip(size)
            }
            "EnumProperty" => {
                self.read_fstring()?;
                self.read_optional_guid_string()?;
                self.skip(size)
            }
            "BoolProperty" => {
                self.read_byte()?;
                self.read_optional_guid_string()?;
                Ok(())
            }
            "ByteProperty" => {
                let enum_type = self.read_fstring()?;
                self.read_optional_guid_string()?;
                if enum_type == "None" {
                    self.skip(1)
                } else {
                    self.read_fstring().map(|_| ())
                }
            }
            _ => {
                self.read_optional_guid_string()?;
                self.skip(size)
            }
        }
    }

    pub fn read_header(&mut self) -> Result<(), String> {
        let magic = self.read_i32()?;
        if magic != 0x5341_5647 {
            return Err("Invalid GVAS magic.".to_string());
        }

        let save_game_version = self.read_i32()?;
        if save_game_version != 3 {
            return Err(format!(
                "Unsupported save game version: {save_game_version}."
            ));
        }

        self.read_i32()?;
        self.read_i32()?;
        self.read_u16()?;
        self.read_u16()?;
        self.read_u16()?;
        self.read_u32()?;
        self.read_fstring()?;
        let custom_version_format = self.read_i32()?;
        if custom_version_format != 3 {
            return Err(format!(
                "Unsupported custom version format: {custom_version_format}."
            ));
        }

        let custom_version_count = self.read_u32()?;
        for _ in 0..custom_version_count {
            self.read_guid_string()?;
            self.read_i32()?;
        }

        self.read_fstring()?;
        Ok(())
    }
}

struct CharacterKey {
    instance_id: Option<String>,
    player_uid: Option<String>,
    debug_name: Option<String>,
}

fn read_character_key(reader: &mut GvasReader) -> Result<CharacterKey, String> {
    let mut key = CharacterKey {
        instance_id: None,
        player_uid: None,
        debug_name: None,
    };

    while !reader.end() {
        let property_name = reader.read_fstring()?;
        if property_name == "None" {
            break;
        }

        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        match property_name.as_str() {
            "InstanceId" => key.instance_id = reader.read_guid_struct_property(&type_name, size)?,
            "PlayerUId" => key.player_uid = reader.read_guid_struct_property(&type_name, size)?,
            "DebugName" => key.debug_name = reader.read_string_like_property(&type_name, size)?,
            _ => reader.skip_property(&type_name, size)?,
        }
    }

    Ok(key)
}

fn read_save_parameter(
    reader: &mut GvasReader,
    type_name: &str,
    size: u64,
    target: &mut CacheCharacter,
) -> Result<(), String> {
    if type_name != "StructProperty" {
        reader.skip_property(type_name, size)?;
        return Ok(());
    }

    reader.read_fstring()?;
    reader.read_guid_string()?;
    reader.read_optional_guid_string()?;

    while !reader.end() {
        let property_name = reader.read_fstring()?;
        if property_name == "None" {
            break;
        }

        let property_type = reader.read_fstring()?;
        let property_size = reader.read_u64()?;
        match normalize_name(&property_name) {
            "CharacterID" => {
                target.character_id =
                    reader.read_string_like_property(&property_type, property_size)?;
            }
            "NickName" => {
                target.nick_name =
                    reader.read_string_like_property(&property_type, property_size)?;
            }
            "OwnedTime" => {
                target.owned_time_utc =
                    reader.read_datetime_struct_property(&property_type, property_size)?;
            }
            "PassiveSkillList" => {
                target.passive_skills =
                    reader.read_string_array_property(&property_type, property_size)?;
            }
            "EquipWaza" => {
                target.equip_waza =
                    reader.read_string_array_property(&property_type, property_size)?;
            }
            "MasteredWaza" => {
                target.mastered_waza =
                    reader.read_string_array_property(&property_type, property_size)?;
            }
            "Level" => {
                target.level = reader.read_int_like_property(&property_type, property_size)?;
            }
            "Talent_HP" => {
                target.talent_hp = reader.read_int_like_property(&property_type, property_size)?;
            }
            "Talent_Melee" => {
                target.talent_melee =
                    reader.read_int_like_property(&property_type, property_size)?;
            }
            "Talent_Shot" => {
                target.talent_shot =
                    reader.read_int_like_property(&property_type, property_size)?;
            }
            "Talent_Defense" => {
                target.talent_defense =
                    reader.read_int_like_property(&property_type, property_size)?;
            }
            "Rank" => {
                target.rank = reader.read_int_like_property(&property_type, property_size)?;
            }
            "FriendshipPoint" => {
                target.friendship_point =
                    reader.read_int_like_property(&property_type, property_size)?;
            }
            "SlotId" => {
                let slot = reader.read_slot_id_struct_property(&property_type, property_size)?;
                target.slot_container_id = slot.0;
                target.slot_index = slot.1;
            }
            "IsPlayer" => {
                target.is_player = reader.read_bool_like_property(&property_type, property_size)?;
            }
            "Gender" => {
                target.gender = reader.read_string_like_property(&property_type, property_size)?;
            }
            "OwnerPlayerUId" => {
                target.owner_player_uid =
                    reader.read_guid_struct_property(&property_type, property_size)?;
            }
            _ => reader.skip_property(&property_type, property_size)?,
        }
    }

    Ok(())
}

fn read_raw_data(reader: &mut GvasReader, type_name: &str) -> Result<CacheCharacter, String> {
    if type_name != "ArrayProperty" {
        return Err(format!(
            "Expected ArrayProperty for RawData, got {type_name}."
        ));
    }

    let array_type = reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    if array_type != "ByteProperty" {
        return Err(format!(
            "Expected ByteProperty RawData array, got {array_type}."
        ));
    }

    let byte_count = reader.read_u32()? as usize;
    let raw_bytes = reader.read_bytes(byte_count)?;
    let mut raw_reader = GvasReader::new(raw_bytes);
    let mut value = CacheCharacter {
        raw_data_decoded: true,
        ..CacheCharacter::default()
    };

    while !raw_reader.end() {
        let property_name = raw_reader.read_fstring()?;
        if property_name == "None" {
            break;
        }

        let property_type = raw_reader.read_fstring()?;
        let property_size = raw_reader.read_u64()?;
        if normalize_name(&property_name) == "SaveParameter" {
            read_save_parameter(&mut raw_reader, &property_type, property_size, &mut value)?;
        } else {
            raw_reader.skip_property(&property_type, property_size)?;
        }
    }

    if raw_reader.remaining() >= 4 {
        raw_reader.skip(4)?;
    }
    if raw_reader.remaining() >= 16 {
        value.group_id = Some(raw_reader.read_guid_string()?);
    }

    Ok(value)
}

fn read_character_value(reader: &mut GvasReader) -> Result<CacheCharacter, String> {
    let mut value = CacheCharacter::default();

    while !reader.end() {
        let property_name = reader.read_fstring()?;
        if property_name == "None" {
            break;
        }

        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if normalize_name(&property_name) == "RawData" {
            // JavaScript Object.assign replaces every field, including arrays, with RawData results.
            value = read_raw_data(reader, &type_name)?;
            continue;
        }

        reader.skip_property(&type_name, size)?;
    }

    Ok(value)
}

fn read_character_save_parameter_map(
    reader: &mut GvasReader,
    type_name: &str,
) -> Result<Vec<CacheCharacter>, String> {
    if type_name != "MapProperty" {
        return Err(format!(
            "Expected MapProperty for CharacterSaveParameterMap, got {type_name}."
        ));
    }

    let key_type = reader.read_fstring()?;
    let value_type = reader.read_fstring()?;
    reader.read_optional_guid_string()?;
    reader.read_u32()?;
    let count = reader.read_u32()?;
    if key_type != "StructProperty" || value_type != "StructProperty" {
        return Err(format!(
            "Unexpected CharacterSaveParameterMap key/value types: {key_type}/{value_type}."
        ));
    }

    let mut characters = Vec::new();
    for _ in 0..count {
        let key = read_character_key(reader)?;
        let mut character = read_character_value(reader)?;
        character.instance_id = key.instance_id;
        character.player_uid = key.player_uid;
        character.debug_name = key.debug_name;
        characters.push(character);
    }

    Ok(characters)
}

fn extract_from_world_save(reader: &mut GvasReader) -> Result<Vec<CacheCharacter>, String> {
    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }

        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if normalize_name(&name) == "CharacterSaveParameterMap" {
            return read_character_save_parameter_map(reader, &type_name);
        }

        reader.skip_property(&type_name, size)?;
    }

    Err("CharacterSaveParameterMap was not found.".to_string())
}

/// Equivalent to lib.mjs extractCharactersFromGvas.
/// Reads PalStorageContainerId and OtomoCharacterContainerId directly below SaveData in
/// Players/<UId>.sav, ported from lib.mjs readPlayerContainerIds.
pub fn read_player_container_ids(
    gvas_payload: &[u8],
) -> Result<(Option<String>, Option<String>), String> {
    let mut reader = GvasReader::new(gvas_payload);
    reader.read_header()?;

    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == "SaveData" && type_name == "StructProperty" {
            reader.read_fstring()?;
            reader.read_guid_string()?;
            reader.read_optional_guid_string()?;
            let end = reader.bounded_end(size)?;
            return scan_player_container_ids(&mut reader, end);
        }
        reader.skip_property(&type_name, size)?;
    }

    Ok((None, None))
}

fn scan_player_container_ids(
    reader: &mut GvasReader,
    end: usize,
) -> Result<(Option<String>, Option<String>), String> {
    let mut pal_storage = None;
    let mut otomo = None;
    while reader.offset < end {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }
        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if (name == "PalStorageContainerId" || name == "OtomoCharacterContainerId")
            && type_name == "StructProperty"
        {
            reader.read_fstring()?;
            reader.read_guid_string()?;
            reader.read_optional_guid_string()?;
            let struct_end = reader.bounded_end(size)?;
            let mut id = None;
            while reader.offset < struct_end {
                let inner_name = reader.read_fstring()?;
                if inner_name == "None" {
                    break;
                }
                let inner_type = reader.read_fstring()?;
                let inner_size = reader.read_u64()?;
                if inner_name == "ID" {
                    id = reader.read_guid_struct_property(&inner_type, inner_size)?;
                } else {
                    reader.skip_property(&inner_type, inner_size)?;
                }
            }
            reader.offset = struct_end;
            if name == "PalStorageContainerId" {
                pal_storage = id;
            } else {
                otomo = id;
            }
            continue;
        }
        reader.skip_property(&type_name, size)?;
    }
    Ok((pal_storage, otomo))
}

pub fn extract_characters_from_gvas(gvas_payload: &[u8]) -> Result<Vec<CacheCharacter>, String> {
    let mut reader = GvasReader::new(gvas_payload);
    reader.read_header()?;

    while !reader.end() {
        let name = reader.read_fstring()?;
        if name == "None" {
            break;
        }

        let type_name = reader.read_fstring()?;
        let size = reader.read_u64()?;
        if name == "worldSaveData" && type_name == "StructProperty" {
            reader.read_fstring()?;
            reader.read_guid_string()?;
            reader.read_optional_guid_string()?;
            return extract_from_world_save(&mut reader);
        }

        reader.skip_property(&type_name, size)?;
    }

    Err("worldSaveData was not found.".to_string())
}

// ---------------------------------------------------------------------------
// Test fixtures ported from tests/helpers/level-sav-fixture.ts.
// Defines the same standard world and expected values used by the TypeScript parity baseline.
// ---------------------------------------------------------------------------
#[cfg(test)]
pub(crate) mod test_fixture {
    use crate::implementation::model::CacheCharacter;

    /// Produces bytes that formatGuid converts back to the canonical string.
    pub fn guid_bytes_from_canonical(canonical: &str) -> [u8; 16] {
        let hex: String = canonical.chars().filter(|c| *c != '-').collect();
        assert_eq!(hex.len(), 32, "invalid guid: {canonical}");
        let part = |index: usize| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
        let mut bytes = [0u8; 16];
        bytes[3] = part(0);
        bytes[2] = part(1);
        bytes[1] = part(2);
        bytes[0] = part(3);
        bytes[7] = part(4);
        bytes[6] = part(5);
        bytes[5] = part(6);
        bytes[4] = part(7);
        bytes[11] = part(8);
        bytes[10] = part(9);
        bytes[9] = part(10);
        bytes[8] = part(11);
        bytes[15] = part(12);
        bytes[14] = part(13);
        bytes[13] = part(14);
        bytes[12] = part(15);
        bytes
    }

    #[derive(Default)]
    pub struct GvasWriter {
        buf: Vec<u8>,
    }

    impl GvasWriter {
        pub fn into_bytes(self) -> Vec<u8> {
            self.buf
        }

        pub fn raw(&mut self, bytes: &[u8]) {
            self.buf.extend_from_slice(bytes);
        }

        pub fn u8v(&mut self, value: u8) {
            self.buf.push(value);
        }

        pub fn u16v(&mut self, value: u16) {
            self.raw(&value.to_le_bytes());
        }

        pub fn i32v(&mut self, value: i32) {
            self.raw(&value.to_le_bytes());
        }

        pub fn u32v(&mut self, value: u32) {
            self.raw(&value.to_le_bytes());
        }

        pub fn u64v(&mut self, value: u64) {
            self.raw(&value.to_le_bytes());
        }

        pub fn i64v(&mut self, value: i64) {
            self.raw(&value.to_le_bytes());
        }

        pub fn f32v(&mut self, value: f32) {
            self.raw(&value.to_le_bytes());
        }

        pub fn f64v(&mut self, value: f64) {
            self.raw(&value.to_le_bytes());
        }

        /// Counterpart to GvasReader::read_fstring with automatic ASCII/UTF-16LE selection.
        pub fn fstring(&mut self, value: &str) {
            if value.is_empty() {
                self.i32v(0);
                return;
            }
            if value.is_ascii() {
                self.i32v(value.len() as i32 + 1);
                self.raw(value.as_bytes());
                self.u8v(0);
                return;
            }
            let units: Vec<u16> = value.encode_utf16().collect();
            self.i32v(-(units.len() as i32 + 1));
            for unit in units {
                self.u16v(unit);
            }
            self.u16v(0);
        }

        pub fn guid(&mut self, canonical: &str) {
            let bytes = guid_bytes_from_canonical(canonical);
            self.raw(&bytes);
        }

        pub fn zero_guid(&mut self) {
            self.raw(&[0u8; 16]);
        }

        /// Encodes the absent case for readOptionalGuidString as flag zero.
        pub fn no_guid_flag(&mut self) {
            self.u8v(0);
        }
    }

    pub fn fstring_byte_length(value: &str) -> u64 {
        if value.is_empty() {
            return 4;
        }
        if value.is_ascii() {
            return 4 + value.len() as u64 + 1;
        }
        4 + value.encode_utf16().count() as u64 * 2 + 2
    }

    pub fn write_header(writer: &mut GvasWriter) {
        writer.i32v(0x5341_5647); // "GVAS"
        writer.i32v(3); // saveGameVersion
        writer.i32v(0);
        writer.i32v(0);
        writer.u16v(5);
        writer.u16v(1);
        writer.u16v(0);
        writer.u32v(0);
        writer.fstring("main");
        writer.i32v(3); // customVersionFormat
        writer.u32v(0); // customVersionCount
        writer.fstring("/Script/Pal.PalWorldSaveGame");
    }

    fn write_name_property(writer: &mut GvasWriter, name: &str, value: &str) {
        writer.fstring(name);
        writer.fstring("NameProperty");
        writer.u64v(fstring_byte_length(value));
        writer.no_guid_flag();
        writer.fstring(value);
    }

    fn write_str_property(writer: &mut GvasWriter, name: &str, value: &str) {
        writer.fstring(name);
        writer.fstring("StrProperty");
        writer.u64v(fstring_byte_length(value));
        writer.no_guid_flag();
        writer.fstring(value);
    }

    fn write_enum_property(writer: &mut GvasWriter, name: &str, enum_type: &str, value: &str) {
        writer.fstring(name);
        writer.fstring("EnumProperty");
        writer.u64v(fstring_byte_length(value));
        writer.fstring(enum_type);
        writer.no_guid_flag();
        writer.fstring(value);
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
        writer.u8v(if value { 1 } else { 0 });
        writer.no_guid_flag();
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

    fn write_datetime_struct_property(writer: &mut GvasWriter, name: &str, ticks: i64) {
        writer.fstring(name);
        writer.fstring("StructProperty");
        writer.u64v(8);
        writer.fstring("DateTime");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.i64v(ticks);
    }

    fn write_string_array_property(
        writer: &mut GvasWriter,
        name: &str,
        array_type: &str,
        values: &[&str],
    ) {
        let size: u64 = 4 + values
            .iter()
            .map(|value| fstring_byte_length(value))
            .sum::<u64>();
        writer.fstring(name);
        writer.fstring("ArrayProperty");
        writer.u64v(size);
        writer.fstring(array_type);
        writer.no_guid_flag();
        writer.u32v(values.len() as u32);
        for value in values {
            writer.fstring(value);
        }
    }

    /// Dummy FloatProperty used to exercise the unknown-property skip path.
    fn write_float_property(writer: &mut GvasWriter, name: &str, value: f32) {
        writer.fstring(name);
        writer.fstring("FloatProperty");
        writer.u64v(4);
        writer.no_guid_flag();
        writer.f32v(value);
    }

    #[derive(Default, Clone)]
    pub struct FixtureCharacter {
        pub instance_id: String,
        pub player_uid: Option<String>,
        pub debug_name: Option<String>,
        /// When false, omits RawData entirely and produces a row with raw_data_decoded=false.
        pub has_raw_data: bool,
        pub character_id: Option<String>,
        pub nick_name: Option<String>,
        pub owned_time_ticks: Option<i64>,
        pub passive_skills: Option<Vec<String>>,
        pub equip_waza: Option<Vec<String>>,
        pub mastered_waza: Option<Vec<String>>,
        pub level: Option<i32>,
        pub talent_hp: Option<i32>,
        pub talent_melee: Option<i32>,
        pub talent_shot: Option<i32>,
        pub talent_defense: Option<i32>,
        pub is_player: Option<bool>,
        pub gender: Option<String>,
        pub owner_player_uid: Option<String>,
        pub group_id: Option<String>,
    }

    impl FixtureCharacter {
        pub fn base() -> Self {
            FixtureCharacter {
                has_raw_data: true,
                ..FixtureCharacter::default()
            }
        }
    }

    fn build_raw_data(character: &FixtureCharacter) -> Vec<u8> {
        // SaveParameter structure body.
        let mut inner = GvasWriter::default();
        if let Some(value) = &character.character_id {
            write_name_property(&mut inner, "CharacterID", value);
        }
        if let Some(value) = &character.nick_name {
            write_str_property(&mut inner, "NickName", value);
        }
        if let Some(ticks) = character.owned_time_ticks {
            write_datetime_struct_property(&mut inner, "OwnedTime", ticks);
        }
        if let Some(values) = &character.passive_skills {
            let refs: Vec<&str> = values.iter().map(String::as_str).collect();
            write_string_array_property(&mut inner, "PassiveSkillList", "NameProperty", &refs);
        }
        if let Some(values) = &character.equip_waza {
            let refs: Vec<&str> = values.iter().map(String::as_str).collect();
            write_string_array_property(&mut inner, "EquipWaza", "EnumProperty", &refs);
        }
        if let Some(values) = &character.mastered_waza {
            let refs: Vec<&str> = values.iter().map(String::as_str).collect();
            write_string_array_property(&mut inner, "MasteredWaza", "EnumProperty", &refs);
        }
        if let Some(value) = character.level {
            write_int_property(&mut inner, "Level", value);
        }
        if let Some(value) = character.talent_hp {
            write_int_property(&mut inner, "Talent_HP", value);
        }
        if let Some(value) = character.talent_melee {
            write_int_property(&mut inner, "Talent_Melee", value);
        }
        if let Some(value) = character.talent_shot {
            write_int_property(&mut inner, "Talent_Shot", value);
        }
        if let Some(value) = character.talent_defense {
            write_int_property(&mut inner, "Talent_Defense", value);
        }
        if let Some(value) = character.is_player {
            write_bool_property(&mut inner, "IsPlayer", value);
        }
        if let Some(value) = &character.gender {
            write_enum_property(&mut inner, "Gender", "EPalGenderType", value);
        }
        if let Some(value) = &character.owner_player_uid {
            write_guid_struct_property(&mut inner, "OwnerPlayerUId", value);
        }
        // Reproduce skipping an unsupported property found in real data.
        write_float_property(&mut inner, "SanityValue", 100.0);
        inner.fstring("None");
        let inner_buffer = inner.into_bytes();

        let struct_header_overhead =
            fstring_byte_length("PalIndividualCharacterSaveParameter") + 16 + 1;
        let mut body = GvasWriter::default();
        body.fstring("SaveParameter");
        body.fstring("StructProperty");
        body.u64v(struct_header_overhead + inner_buffer.len() as u64);
        body.fstring("PalIndividualCharacterSaveParameter");
        body.zero_guid();
        body.no_guid_flag();
        body.raw(&inner_buffer);
        body.fstring("None");
        // Tail: four unused bytes followed by groupId.
        body.u32v(0);
        if let Some(group_id) = &character.group_id {
            body.guid(group_id);
        }
        body.into_bytes()
    }

    fn write_character_entry(writer: &mut GvasWriter, character: &FixtureCharacter) {
        // Key structure.
        write_guid_struct_property(writer, "InstanceId", &character.instance_id);
        if let Some(player_uid) = &character.player_uid {
            write_guid_struct_property(writer, "PlayerUId", player_uid);
        }
        if let Some(debug_name) = &character.debug_name {
            write_str_property(writer, "DebugName", debug_name);
        }
        writer.fstring("None");

        // Value structure.
        if character.has_raw_data {
            let raw_data = build_raw_data(character);
            writer.fstring("RawData");
            writer.fstring("ArrayProperty");
            writer.u64v(4 + raw_data.len() as u64);
            writer.fstring("ByteProperty");
            writer.no_guid_flag();
            writer.u32v(raw_data.len() as u32);
            writer.raw(&raw_data);
        } else {
            // No RawData, producing raw_data_decoded=false.
            write_int_property(writer, "Padding", 0);
        }
        writer.fstring("None");
    }

    /// Builds an uncompressed GVAS payload.
    pub fn build_gvas_payload(characters: &[FixtureCharacter]) -> Vec<u8> {
        let mut map = GvasWriter::default();
        map.fstring("StructProperty");
        map.fstring("StructProperty");
        map.no_guid_flag();
        map.u32v(0);
        map.u32v(characters.len() as u32);
        for character in characters {
            write_character_entry(&mut map, character);
        }
        let map_buffer = map.into_bytes();

        let mut world = GvasWriter::default();
        world.fstring("CharacterSaveParameterMap");
        world.fstring("MapProperty");
        world.u64v(map_buffer.len() as u64);
        world.raw(&map_buffer);
        world.fstring("None");
        let world_buffer = world.into_bytes();

        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        let struct_header_overhead = fstring_byte_length("PalWorldSaveData") + 16 + 1;
        writer.fstring("worldSaveData");
        writer.fstring("StructProperty");
        writer.u64v(struct_header_overhead + world_buffer.len() as u64);
        writer.fstring("PalWorldSaveData");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.raw(&world_buffer);
        writer.fstring("None");
        writer.into_bytes()
    }

    pub fn deflate(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    /// Wraps a GVAS payload in a Palworld .sav container with a PlZ header.
    /// compression: "zlib" | "double-zlib"
    pub fn wrap_as_level_sav(gvas_payload: &[u8], compression: &str) -> Vec<u8> {
        const MAGIC_PLZ: u32 = 0x5a6c50;
        let once = deflate(gvas_payload);
        let (payload, save_type): (Vec<u8>, u32) = if compression == "double-zlib" {
            (deflate(&once), 0x32)
        } else {
            (once, 0x31)
        };
        let mut writer = GvasWriter::default();
        writer.u32v(gvas_payload.len() as u32);
        writer.u32v(payload.len() as u32);
        writer.u32v(MAGIC_PLZ | (save_type << 24));
        writer.raw(&payload);
        writer.into_bytes()
    }

    pub fn build_level_sav(characters: &[FixtureCharacter], compression: &str) -> Vec<u8> {
        wrap_as_level_sav(&build_gvas_payload(characters), compression)
    }

    /// Players 配下へ共存する次元パルボックス保存の最小 fixture。
    /// 実データの 9,600 要素は含めず、ルートと配列メタデータだけを再現する。
    pub fn build_dimension_pal_storage_sav(compression: &str) -> Vec<u8> {
        let mut body = GvasWriter::default();
        body.u32v(0);
        body.fstring("SaveParameterArray");
        body.fstring("StructProperty");
        body.u64v(0);
        body.fstring("PalDimensionPalStorageSaveParameter");
        body.zero_guid();
        body.no_guid_flag();
        let body = body.into_bytes();

        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        writer.fstring("SaveParameterArray");
        writer.fstring("ArrayProperty");
        writer.u64v(body.len() as u64);
        writer.fstring("StructProperty");
        writer.no_guid_flag();
        writer.raw(&body);
        writer.fstring("None");
        wrap_as_level_sav(&writer.into_bytes(), compression)
    }

    /// Minimal LevelMeta.sav (PalWorldBaseInfoSaveGame) structure.
    /// Produces the same bytes as buildLevelMetaPayload in the TypeScript fixture.
    /// Passing None omits WorldName to exercise degraded input.
    pub fn build_level_meta_payload(world_name: Option<&str>) -> Vec<u8> {
        let mut body = GvasWriter::default();
        if let Some(value) = world_name {
            write_str_property(&mut body, "WorldName", value);
        }
        write_int_property(&mut body, "InGameDay", 12);
        body.fstring("None");
        let body_buffer = body.into_bytes();

        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        write_int_property(&mut writer, "Version", 1);
        writer.fstring("SaveData");
        writer.fstring("StructProperty");
        writer.u64v(body_buffer.len() as u64);
        writer.fstring("PalWorldBaseInfoSaveData");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.raw(&body_buffer);
        writer.fstring("None");
        writer.into_bytes()
    }

    pub fn build_level_meta_sav(world_name: Option<&str>, compression: &str) -> Vec<u8> {
        wrap_as_level_sav(&build_level_meta_payload(world_name), compression)
    }

    fn s(value: &str) -> Option<String> {
        Some(value.to_string())
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    /// Port of STANDARD_WORLD_CHARACTERS from tests/helpers/level-sav-fixture.ts.
    pub fn standard_world_characters() -> Vec<FixtureCharacter> {
        vec![
            // Player one, owner of a held Pal.
            FixtureCharacter {
                instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
                player_uid: s("aaaaaaaa-0000-0000-0000-000000000001"),
                debug_name: s("Player One"),
                character_id: s("Player"),
                nick_name: s("ホストさん"),
                is_player: Some(true),
                level: Some(42),
                ..FixtureCharacter::base()
            },
            // Player two, with no nickName so display falls back to debugName.
            FixtureCharacter {
                instance_id: "22222222-2222-2222-2222-222222222222".to_string(),
                player_uid: s("aaaaaaaa-0000-0000-0000-000000000002"),
                debug_name: s("Player Two"),
                character_id: s("Player"),
                is_player: Some(true),
                level: Some(13),
                ..FixtureCharacter::base()
            },
            // Held Pal with a Japanese nickname, PAL_ passive prefix, and EPalWazaID skill.
            FixtureCharacter {
                instance_id: "33333333-3333-3333-3333-333333333333".to_string(),
                character_id: s("SheepBall"),
                nick_name: s("もこもこ"),
                owned_time_ticks: Some(638_869_248_000_000_000), // 2025-07-01T00:00:00.0000000+00:00
                passive_skills: Some(strings(&["PAL_Legend", "Rare"])),
                equip_waza: Some(strings(&["EPalWazaID::WaterGun", "None"])),
                mastered_waza: Some(strings(&["EPalWazaID::WaterGun", "EPalWazaID::PowerShot"])),
                level: Some(25),
                talent_hp: Some(90),
                talent_melee: Some(80),
                talent_shot: Some(70),
                talent_defense: Some(60),
                is_player: Some(false),
                gender: s("EPalGenderType::Female"),
                owner_player_uid: s("aaaaaaaa-0000-0000-0000-000000000001"),
                group_id: s("99999999-9999-9999-9999-999999999999"),
                ..FixtureCharacter::base()
            },
            // Boss instance whose BOSS_ prefix is normalized for masterPalId.
            FixtureCharacter {
                instance_id: "44444444-4444-4444-4444-444444444444".to_string(),
                character_id: s("BOSS_Anubis"),
                passive_skills: Some(Vec::new()),
                level: Some(50),
                talent_hp: Some(100),
                is_player: Some(false),
                gender: s("EPalGenderType::Male"),
                owner_player_uid: s("aaaaaaaa-0000-0000-0000-000000000002"),
                ..FixtureCharacter::base()
            },
            // Wild Pal with no owner, excluded from held counts and notifications.
            FixtureCharacter {
                instance_id: "55555555-5555-5555-5555-555555555555".to_string(),
                character_id: s("PinkCat"),
                level: Some(3),
                is_player: Some(false),
                ..FixtureCharacter::base()
            },
            // NPC whose SalesPerson prefix maps to kind=NPC.
            FixtureCharacter {
                instance_id: "66666666-6666-6666-6666-666666666666".to_string(),
                character_id: s("SalesPerson_Wander"),
                is_player: Some(false),
                ..FixtureCharacter::base()
            },
            // Row without RawData; raw_data_decoded=false excludes it from transformation.
            FixtureCharacter {
                instance_id: "77777777-7777-7777-7777-777777777777".to_string(),
                has_raw_data: false,
                ..FixtureCharacter::default()
            },
        ]
    }

    /// Port of EXPECTED_CHARACTERS from tests/level-sav-extractor.test.ts.
    pub fn expected_standard_characters() -> Vec<CacheCharacter> {
        vec![
            CacheCharacter {
                instance_id: s("11111111-1111-1111-1111-111111111111"),
                player_uid: s("aaaaaaaa-0000-0000-0000-000000000001"),
                debug_name: s("Player One"),
                character_id: s("Player"),
                nick_name: s("ホストさん"),
                level: Some(42),
                is_player: Some(true),
                raw_data_decoded: true,
                ..CacheCharacter::default()
            },
            CacheCharacter {
                instance_id: s("22222222-2222-2222-2222-222222222222"),
                player_uid: s("aaaaaaaa-0000-0000-0000-000000000002"),
                debug_name: s("Player Two"),
                character_id: s("Player"),
                level: Some(13),
                is_player: Some(true),
                raw_data_decoded: true,
                ..CacheCharacter::default()
            },
            CacheCharacter {
                instance_id: s("33333333-3333-3333-3333-333333333333"),
                character_id: s("SheepBall"),
                nick_name: s("もこもこ"),
                owned_time_utc: s("2025-07-01T00:00:00.0000000+00:00"),
                passive_skills: strings(&["PAL_Legend", "Rare"]),
                equip_waza: strings(&["EPalWazaID::WaterGun", "None"]),
                mastered_waza: strings(&["EPalWazaID::WaterGun", "EPalWazaID::PowerShot"]),
                level: Some(25),
                talent_hp: Some(90),
                talent_melee: Some(80),
                talent_shot: Some(70),
                talent_defense: Some(60),
                is_player: Some(false),
                gender: s("EPalGenderType::Female"),
                owner_player_uid: s("aaaaaaaa-0000-0000-0000-000000000001"),
                group_id: s("99999999-9999-9999-9999-999999999999"),
                raw_data_decoded: true,
                ..CacheCharacter::default()
            },
            CacheCharacter {
                instance_id: s("44444444-4444-4444-4444-444444444444"),
                character_id: s("BOSS_Anubis"),
                level: Some(50),
                talent_hp: Some(100),
                is_player: Some(false),
                gender: s("EPalGenderType::Male"),
                owner_player_uid: s("aaaaaaaa-0000-0000-0000-000000000002"),
                raw_data_decoded: true,
                ..CacheCharacter::default()
            },
            CacheCharacter {
                instance_id: s("55555555-5555-5555-5555-555555555555"),
                character_id: s("PinkCat"),
                level: Some(3),
                is_player: Some(false),
                raw_data_decoded: true,
                ..CacheCharacter::default()
            },
            CacheCharacter {
                instance_id: s("66666666-6666-6666-6666-666666666666"),
                character_id: s("SalesPerson_Wander"),
                is_player: Some(false),
                raw_data_decoded: true,
                ..CacheCharacter::default()
            },
            CacheCharacter {
                instance_id: s("77777777-7777-7777-7777-777777777777"),
                raw_data_decoded: false,
                ..CacheCharacter::default()
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixture::{
        build_gvas_payload, expected_standard_characters, guid_bytes_from_canonical,
        standard_world_characters, write_header, GvasWriter,
    };
    use super::*;

    #[test]
    fn extracts_all_character_fields_from_a_gvas_payload() {
        let payload = build_gvas_payload(&standard_world_characters());
        let characters = extract_characters_from_gvas(&payload).unwrap();
        assert_eq!(characters, expected_standard_characters());
    }

    #[test]
    fn format_guid_uses_the_same_byte_order_as_typescript() {
        let canonical = "12345678-9abc-def0-1122-334455667788";
        let bytes = guid_bytes_from_canonical(canonical);
        assert_eq!(format_guid(&bytes), canonical);
    }

    #[test]
    fn format_utc_ticks_uses_seven_fractional_digits() {
        assert_eq!(
            format_utc_ticks(638_869_248_000_000_000),
            "2025-07-01T00:00:00.0000000+00:00"
        );
        assert_eq!(
            format_utc_ticks(638_869_248_001_234_567),
            "2025-07-01T00:00:00.1234567+00:00"
        );
    }

    #[test]
    fn normalize_name_removes_only_trailing_numeric_suffixes() {
        assert_eq!(normalize_name("SaveParameter_2"), "SaveParameter");
        assert_eq!(normalize_name("RawData_12"), "RawData");
        assert_eq!(normalize_name("Talent_HP"), "Talent_HP");
        assert_eq!(normalize_name("SalesPerson_Wander"), "SalesPerson_Wander");
        assert_eq!(normalize_name("_5"), "_5");
        assert_eq!(normalize_name("Name_"), "Name_");
        assert_eq!(normalize_name("NoUnderscore"), "NoUnderscore");
    }

    #[test]
    fn ascii_fstring_masks_high_bits_like_node() {
        // Length three and bytes [0xC1, 0x41, 0x00] produce 0xC1 & 0x7F = 0x41 = 'A'.
        let buffer = [3i32.to_le_bytes().to_vec(), vec![0xC1, 0x41, 0x00]].concat();
        let mut reader = GvasReader::new(&buffer);
        assert_eq!(reader.read_fstring().unwrap(), "AA");
    }

    #[test]
    fn byte_property_with_none_enum_returns_the_byte_as_a_string() {
        let mut writer = GvasWriter::default();
        writer.fstring("None");
        writer.no_guid_flag();
        writer.u8v(7);
        let buffer = writer.into_bytes();
        let mut reader = GvasReader::new(&buffer);
        assert_eq!(
            reader.read_string_like_property("ByteProperty", 1).unwrap(),
            Some("7".to_string())
        );
    }

    #[test]
    fn byte_property_with_an_enum_returns_none_as_an_integer() {
        let mut writer = GvasWriter::default();
        writer.fstring("SomeEnum");
        writer.no_guid_flag();
        writer.fstring("SomeEnum::Value");
        let buffer = writer.into_bytes();
        let mut reader = GvasReader::new(&buffer);
        assert_eq!(
            reader.read_int_like_property("ByteProperty", 1).unwrap(),
            None
        );
    }

    #[test]
    fn oversized_struct_property_returns_error_without_panicking() {
        let mut writer = GvasWriter::default();
        writer.fstring("PalCharacterSlotId");
        writer.zero_guid();
        writer.no_guid_flag();
        let buffer = writer.into_bytes();
        let mut reader = GvasReader::new(&buffer);

        assert_eq!(
            reader
                .read_slot_id_struct_property("StructProperty", u64::MAX)
                .unwrap_err(),
            ERR_EOF
        );
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut writer = GvasWriter::default();
        writer.i32v(0x1234_5678);
        let buffer = writer.into_bytes();
        assert_eq!(
            extract_characters_from_gvas(&buffer).unwrap_err(),
            "Invalid GVAS magic."
        );
    }

    #[test]
    fn rejects_an_unsupported_save_version() {
        let mut writer = GvasWriter::default();
        writer.i32v(0x5341_5647);
        writer.i32v(4);
        let buffer = writer.into_bytes();
        assert_eq!(
            extract_characters_from_gvas(&buffer).unwrap_err(),
            "Unsupported save game version: 4."
        );
    }

    #[test]
    fn rejects_an_unsupported_custom_version_format() {
        let mut writer = GvasWriter::default();
        writer.i32v(0x5341_5647);
        writer.i32v(3);
        writer.i32v(0);
        writer.i32v(0);
        writer.u16v(5);
        writer.u16v(1);
        writer.u16v(0);
        writer.u32v(0);
        writer.fstring("main");
        writer.i32v(2);
        let buffer = writer.into_bytes();
        assert_eq!(
            extract_characters_from_gvas(&buffer).unwrap_err(),
            "Unsupported custom version format: 2."
        );
    }

    #[test]
    fn rejects_a_truncated_payload() {
        assert_eq!(
            extract_characters_from_gvas(&[]).unwrap_err(),
            "Unexpected end of GVAS payload."
        );

        // The final 18 bytes are unused after the map count is consumed, so truncate inside the last entry.
        let payload = build_gvas_payload(&standard_world_characters());
        assert_eq!(
            extract_characters_from_gvas(&payload[..payload.len() - 19]).unwrap_err(),
            "Unexpected end of GVAS payload."
        );
    }

    #[test]
    fn returns_an_error_when_world_save_data_is_absent() {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        writer.fstring("None");
        let buffer = writer.into_bytes();
        assert_eq!(
            extract_characters_from_gvas(&buffer).unwrap_err(),
            "worldSaveData was not found."
        );
    }

    #[test]
    fn returns_an_error_when_character_save_parameter_map_is_absent() {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        let overhead = super::test_fixture::fstring_byte_length("PalWorldSaveData") + 16 + 1;
        writer.fstring("worldSaveData");
        writer.fstring("StructProperty");
        writer.u64v(overhead + 9); // Body contains only None.
        writer.fstring("PalWorldSaveData");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.fstring("None");
        let buffer = writer.into_bytes();
        assert_eq!(
            extract_characters_from_gvas(&buffer).unwrap_err(),
            "CharacterSaveParameterMap was not found."
        );
    }

    #[test]
    fn rejects_character_save_parameter_map_with_the_wrong_type() {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        let overhead = super::test_fixture::fstring_byte_length("PalWorldSaveData") + 16 + 1;
        writer.fstring("worldSaveData");
        writer.fstring("StructProperty");
        writer.u64v(overhead);
        writer.fstring("PalWorldSaveData");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.fstring("CharacterSaveParameterMap");
        writer.fstring("ArrayProperty");
        writer.u64v(0);
        let buffer = writer.into_bytes();
        assert_eq!(
            extract_characters_from_gvas(&buffer).unwrap_err(),
            "Expected MapProperty for CharacterSaveParameterMap, got ArrayProperty."
        );
    }

    #[test]
    fn rejects_character_save_parameter_map_with_wrong_key_or_value_types() {
        let mut writer = GvasWriter::default();
        write_header(&mut writer);
        let overhead = super::test_fixture::fstring_byte_length("PalWorldSaveData") + 16 + 1;
        writer.fstring("worldSaveData");
        writer.fstring("StructProperty");
        writer.u64v(overhead);
        writer.fstring("PalWorldSaveData");
        writer.zero_guid();
        writer.no_guid_flag();
        writer.fstring("CharacterSaveParameterMap");
        writer.fstring("MapProperty");
        writer.u64v(0);
        writer.fstring("IntProperty");
        writer.fstring("StructProperty");
        writer.no_guid_flag();
        writer.u32v(0);
        writer.u32v(0);
        let buffer = writer.into_bytes();
        assert_eq!(
            extract_characters_from_gvas(&buffer).unwrap_err(),
            "Unexpected CharacterSaveParameterMap key/value types: IntProperty/StructProperty."
        );
    }
}
