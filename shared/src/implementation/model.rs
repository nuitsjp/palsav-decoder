use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JsNumber(pub f64);

impl Serialize for JsNumber {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.fract() == 0.0 && self.0.abs() <= 9_007_199_254_740_991.0 {
            serializer.serialize_i64(self.0 as i64)
        } else {
            serializer.serialize_f64(self.0)
        }
    }
}

impl<'de> Deserialize<'de> for JsNumber {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        f64::deserialize(deserializer).map(JsNumber)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldPoint {
    pub x: JsNumber,
    pub y: JsNumber,
    pub z: JsNumber,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldCollectible {
    pub kind: String,
    pub map_object_id: String,
    pub variant: Option<String>,
    pub hp_ratio: JsNumber,
    pub x: JsNumber,
    pub y: JsNumber,
    pub z: JsNumber,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldRaid {
    pub base_camp_id: String,
    pub invading: bool,
    pub remaining_sec: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldEventPoint {
    pub kind: String,
    pub x: JsNumber,
    pub y: JsNumber,
    pub z: JsNumber,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldPlayerPoint {
    #[serde(rename = "playerUId")]
    pub player_uid: String,
    pub x: JsNumber,
    pub y: JsNumber,
    pub z: JsNumber,
    pub defeated_boss_spawner_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldOverview {
    pub collectibles: Vec<WorldCollectible>,
    pub raids: Vec<WorldRaid>,
    pub events: Vec<WorldEventPoint>,
    pub players: Vec<WorldPlayerPoint>,
    pub game_day: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CacheCharacter {
    pub instance_id: Option<String>,
    #[serde(rename = "playerUId")]
    pub player_uid: Option<String>,
    pub debug_name: Option<String>,
    pub character_id: Option<String>,
    pub nick_name: Option<String>,
    pub owned_time_utc: Option<String>,
    pub passive_skills: Vec<String>,
    pub equip_waza: Vec<String>,
    pub mastered_waza: Vec<String>,
    pub level: Option<i64>,
    pub talent_hp: Option<i64>,
    pub talent_melee: Option<i64>,
    pub talent_shot: Option<i64>,
    pub talent_defense: Option<i64>,
    pub rank: Option<i64>,
    pub friendship_point: Option<i64>,
    pub slot_container_id: Option<String>,
    pub slot_index: Option<i64>,
    pub is_player: Option<bool>,
    pub gender: Option<String>,
    #[serde(rename = "ownerPlayerUId")]
    pub owner_player_uid: Option<String>,
    pub group_id: Option<String>,
    pub raw_data_decoded: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerContainerIndex {
    pub pal_storage_container_ids: Vec<String>,
    pub otomo_container_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedBaseCamp {
    pub base_camp_id: String,
    pub container_id: Option<String>,
    pub slot_num: Option<i64>,
    pub instance_ids: Vec<String>,
    pub world: Option<WorldPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRelicState {
    pub schema_version: u32,
    pub relics_by_type: Map<String, Value>,
    pub note_ids: Vec<String>,
    pub item_pickup_guids: Vec<String>,
    pub fast_travel_point_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedPlayerRelics {
    #[serde(rename = "playerUId")]
    pub player_uid: String,
    pub state: PlayerRelicState,
}
