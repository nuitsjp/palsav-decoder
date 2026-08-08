use serde::{Deserialize, Serialize};
use std::io::Write;

use crate::implementation::model::{
    CacheCharacter, DecodedBaseCamp, DecodedPlayerRelics, PlayerContainerIndex, PlayerRelicState,
    WorldOverview, WorldPlayerPoint,
};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataDocument {
    pub schema_version: u32,
    pub world_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldDocument {
    pub schema_version: u32,
    pub world_name: Option<String>,
    pub characters: Vec<CacheCharacter>,
    pub player_containers: PlayerContainerIndex,
    pub base_camps: Option<Vec<DecodedBaseCamp>>,
    pub world: Option<WorldOverview>,
    pub player_relics: Vec<DecodedPlayerRelics>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerDocument {
    pub schema_version: u32,
    pub pal_storage_container_id: Option<String>,
    pub otomo_container_id: Option<String>,
    pub point: Option<WorldPlayerPoint>,
    pub relics: PlayerRelicState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayersDocument {
    pub schema_version: u32,
    pub player_relics: Vec<DecodedPlayerRelics>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Ndjson,
}

pub fn write_metadata(
    document: &MetadataDocument,
    format: OutputFormat,
    output: &mut impl Write,
) -> Result<(), String> {
    match format {
        OutputFormat::Json => write_json(document, output),
        OutputFormat::Ndjson => {
            let record = serde_json::json!({
                "type": "metadata",
                "schemaVersion": document.schema_version,
                "worldName": document.world_name,
            });
            write_json(&record, output)
        }
    }
}

pub fn write_world(
    document: &WorldDocument,
    format: OutputFormat,
    output: &mut impl Write,
) -> Result<(), String> {
    if format == OutputFormat::Json {
        return write_json(document, output);
    }

    write_json(
        &serde_json::json!({
            "type": "metadata",
            "schemaVersion": document.schema_version,
            "worldName": document.world_name,
        }),
        output,
    )?;
    for character in &document.characters {
        write_json(
            &serde_json::json!({ "type": "character", "data": character }),
            output,
        )?;
    }
    write_json(
        &serde_json::json!({ "type": "playerContainers", "data": document.player_containers }),
        output,
    )?;
    if let Some(camps) = &document.base_camps {
        for camp in camps {
            write_json(
                &serde_json::json!({ "type": "baseCamp", "data": camp }),
                output,
            )?;
        }
    }
    if let Some(world) = &document.world {
        write_json(
            &serde_json::json!({ "type": "world", "data": world }),
            output,
        )?;
    }
    for relics in &document.player_relics {
        write_json(
            &serde_json::json!({ "type": "playerRelics", "data": relics }),
            output,
        )?;
    }
    for warning in &document.warnings {
        write_json(
            &serde_json::json!({ "type": "warning", "code": warning }),
            output,
        )?;
    }
    write_json(
        &serde_json::json!({
            "type": "end",
            "characterCount": document.characters.len(),
            "playerRelicCount": document.player_relics.len(),
        }),
        output,
    )
}

pub fn write_player(
    document: &PlayerDocument,
    format: OutputFormat,
    output: &mut impl Write,
) -> Result<(), String> {
    match format {
        OutputFormat::Json => write_json(document, output),
        OutputFormat::Ndjson => write_json(
            &serde_json::json!({
                "type": "player",
                "schemaVersion": document.schema_version,
                "data": document,
            }),
            output,
        ),
    }
}

pub fn write_players(
    document: &PlayersDocument,
    format: OutputFormat,
    output: &mut impl Write,
) -> Result<(), String> {
    if format == OutputFormat::Json {
        return write_json(document, output);
    }
    for relics in &document.player_relics {
        write_json(
            &serde_json::json!({ "type": "playerRelics", "data": relics }),
            output,
        )?;
    }
    for warning in &document.warnings {
        write_json(
            &serde_json::json!({ "type": "warning", "code": warning }),
            output,
        )?;
    }
    write_json(
        &serde_json::json!({
            "type": "end",
            "playerRelicCount": document.player_relics.len(),
        }),
        output,
    )
}

fn write_json(value: &impl Serialize, output: &mut impl Write) -> Result<(), String> {
    serde_json::to_writer(&mut *output, value).map_err(|error| error.to_string())?;
    output.write_all(b"\n").map_err(|error| error.to_string())
}
