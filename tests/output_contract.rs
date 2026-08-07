use palsav_decoder::contract::{
    write_metadata, write_player, write_players, write_world, MetadataDocument, OutputFormat,
    PlayerDocument, PlayersDocument, WorldDocument, SCHEMA_VERSION,
};
use palsav_decoder::model::{PlayerContainerIndex, PlayerRelicState};

#[test]
fn json_writes_only_the_schema_version_and_values_to_stdout() {
    let document = MetadataDocument {
        schema_version: SCHEMA_VERSION,
        world_name: Some("テストワールド".to_string()),
    };
    let mut output = Vec::new();

    write_metadata(&document, OutputFormat::Json, &mut output).unwrap();

    assert_eq!(
        String::from_utf8(output).unwrap(),
        "{\"schemaVersion\":1,\"worldName\":\"テストワールド\"}\n"
    );
}

#[test]
fn ndjson_writes_one_typed_record_per_line() {
    let document = MetadataDocument {
        schema_version: SCHEMA_VERSION,
        world_name: None,
    };
    let mut output = Vec::new();

    write_metadata(&document, OutputFormat::Ndjson, &mut output).unwrap();

    assert_eq!(
        String::from_utf8(output).unwrap(),
        "{\"type\":\"metadata\",\"schemaVersion\":1,\"worldName\":null}\n"
    );
}

#[test]
fn world_ndjson_writes_typed_metadata_data_and_end_records() {
    let document = WorldDocument {
        schema_version: SCHEMA_VERSION,
        world_name: None,
        characters: Vec::new(),
        player_containers: PlayerContainerIndex::default(),
        base_camps: Some(Vec::new()),
        world: None,
        player_relics: Vec::new(),
        warnings: vec!["levelMetaUnavailable".to_string()],
    };
    let mut output = Vec::new();

    write_world(&document, OutputFormat::Ndjson, &mut output).unwrap();

    let lines = String::from_utf8(output).unwrap();
    assert!(lines.contains("\"type\":\"metadata\""));
    assert!(lines.contains("\"type\":\"playerContainers\""));
    assert!(lines.contains("\"type\":\"warning\""));
    assert!(lines.ends_with("{\"type\":\"end\",\"characterCount\":0,\"playerRelicCount\":0}\n"));
}

#[test]
fn player_json_and_ndjson_return_the_same_neutral_data() {
    let document = PlayerDocument {
        schema_version: SCHEMA_VERSION,
        pal_storage_container_id: None,
        otomo_container_id: None,
        point: None,
        relics: PlayerRelicState {
            schema_version: 1,
            relics_by_type: serde_json::Map::new(),
            note_ids: Vec::new(),
            item_pickup_guids: Vec::new(),
        },
    };
    let mut json = Vec::new();
    let mut ndjson = Vec::new();

    write_player(&document, OutputFormat::Json, &mut json).unwrap();
    write_player(&document, OutputFormat::Ndjson, &mut ndjson).unwrap();

    assert!(String::from_utf8(json)
        .unwrap()
        .starts_with("{\"schemaVersion\":1"));
    assert!(String::from_utf8(ndjson)
        .unwrap()
        .contains("\"type\":\"player\""));
}

#[test]
fn players_ndjson_writes_warning_and_end_records() {
    let document = PlayersDocument {
        schema_version: 1,
        player_relics: vec![],
        warnings: vec!["playerDataPartiallyUnavailable".to_string()],
    };
    let mut output = Vec::new();

    write_players(&document, OutputFormat::Ndjson, &mut output).unwrap();

    assert_eq!(
        String::from_utf8(output).unwrap(),
        "{\"type\":\"warning\",\"code\":\"playerDataPartiallyUnavailable\"}\n{\"type\":\"end\",\"playerRelicCount\":0}\n"
    );
}
