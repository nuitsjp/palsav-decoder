use std::fs;
use std::path::{Path, PathBuf};

use crate::implementation::contract::{
    MetadataDocument, PlayerDocument, PlayersDocument, WorldDocument, SCHEMA_VERSION,
};
use crate::implementation::extractor;
use crate::implementation::model::{
    DecodedBaseCamp, DecodedPlayerRelics, PlayerContainerIndex, WorldOverview,
};

const WARNING_BASE_CAMPS: &str = "baseCampsUnavailable";
const WARNING_LEVEL_META: &str = "levelMetaUnavailable";
const WARNING_PLAYER_DATA: &str = "playerDataPartiallyUnavailable";
const WARNING_WORLD: &str = "worldOverviewUnavailable";

pub fn decode_metadata(path: &Path) -> Result<MetadataDocument, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let decoded = extractor::decompress::decompress_sav(&bytes)?;
    Ok(MetadataDocument {
        schema_version: SCHEMA_VERSION,
        world_name: extractor::level_meta::extract_world_name_from_gvas(&decoded.payload)?,
    })
}

pub fn decode_player(path: &Path) -> Result<PlayerDocument, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let decoded = extractor::decompress::decompress_sav(&bytes)?;
    let (pal_storage_container_id, otomo_container_id) =
        extractor::gvas::read_player_container_ids(&decoded.payload)?;
    let point = extractor::world::extract_player_point_from_gvas(&decoded.payload)?;
    let relics = extractor::player_relics::extract_player_relics_from_gvas(&decoded.payload)?;
    Ok(PlayerDocument {
        schema_version: SCHEMA_VERSION,
        pal_storage_container_id,
        otomo_container_id,
        point,
        relics: crate::implementation::model::PlayerRelicState {
            schema_version: 1,
            relics_by_type: relics.relics,
            note_ids: relics.notes,
            item_pickup_guids: relics.ruins,
        },
    })
}

pub fn decode_players(path: &Path) -> Result<PlayersDocument, String> {
    let save_dir = if path.file_name().is_some_and(|name| name == "Players") {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let (files, directory_unavailable) = player_files(save_dir);
    let mut player_relics = Vec::new();
    let mut partial = directory_unavailable;
    for player_path in files {
        let Some(player_uid) = player_uid_from_path(&player_path) else {
            partial = true;
            continue;
        };
        match decode_player(&player_path) {
            Ok(player) => player_relics.push(DecodedPlayerRelics {
                player_uid,
                state: player.relics,
            }),
            Err(_) => partial = true,
        }
    }
    player_relics.sort_by(|left, right| left.player_uid.cmp(&right.player_uid));
    Ok(PlayersDocument {
        schema_version: SCHEMA_VERSION,
        player_relics,
        warnings: if partial {
            vec![WARNING_PLAYER_DATA.to_string()]
        } else {
            Vec::new()
        },
    })
}

pub fn decode_world(path: &Path) -> Result<WorldDocument, String> {
    let save_dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .ok_or_else(|| "Could not resolve the parent directory of Level.sav.".to_string())?
            .to_path_buf()
    };
    let level_path = if path.is_dir() {
        save_dir.join("Level.sav")
    } else {
        path.to_path_buf()
    };
    let bytes = fs::read(&level_path).map_err(|error| error.to_string())?;
    let decoded = extractor::decompress::decompress_sav(&bytes)?;
    let characters = extractor::gvas::extract_characters_from_gvas(&decoded.payload)?;
    let mut warnings = Vec::new();

    let base_camps = match extractor::base_camps::extract_base_camps_from_gvas(&decoded.payload) {
        Ok(camps) => Some(
            camps
                .into_iter()
                .map(|camp| DecodedBaseCamp {
                    base_camp_id: camp.base_camp_id,
                    container_id: camp.container_id,
                    slot_num: camp.slot_num,
                    instance_ids: camp.instance_ids,
                    world: camp.world,
                })
                .collect(),
        ),
        Err(_) => {
            warnings.push(WARNING_BASE_CAMPS.to_string());
            None
        }
    };

    let mut player_containers = PlayerContainerIndex::default();
    let mut player_points = Vec::new();
    let mut player_relics = Vec::new();
    let (player_files, player_directory_unavailable) = player_files(&save_dir);
    let mut player_partial = player_directory_unavailable;
    for player_path in player_files {
        let player_uid = player_uid_from_path(&player_path);
        let Ok(bytes) = fs::read(&player_path) else {
            player_partial = true;
            continue;
        };
        let Ok(player) = extractor::decompress::decompress_sav(&bytes) else {
            player_partial = true;
            continue;
        };
        match extractor::gvas::read_player_container_ids(&player.payload) {
            Ok((storage, otomo)) => {
                if let Some(value) = storage {
                    player_containers.pal_storage_container_ids.push(value);
                }
                if let Some(value) = otomo {
                    player_containers.otomo_container_ids.push(value);
                }
            }
            Err(_) => player_partial = true,
        }
        match extractor::world::extract_player_point_from_gvas(&player.payload) {
            Ok(Some(point)) => player_points.push(point),
            Ok(None) => {}
            Err(_) => player_partial = true,
        }
        if let Some(player_uid) = player_uid {
            match extractor::player_relics::extract_player_relics_from_gvas(&player.payload) {
                Ok(relics) => player_relics.push(DecodedPlayerRelics {
                    player_uid,
                    state: crate::implementation::model::PlayerRelicState {
                        schema_version: 1,
                        relics_by_type: relics.relics,
                        note_ids: relics.notes,
                        item_pickup_guids: relics.ruins,
                    },
                }),
                Err(_) => player_partial = true,
            }
        } else {
            player_partial = true;
        }
    }
    player_containers.pal_storage_container_ids.sort();
    player_containers.pal_storage_container_ids.dedup();
    player_containers.otomo_container_ids.sort();
    player_containers.otomo_container_ids.dedup();
    player_points.sort_by(|left, right| left.player_uid.cmp(&right.player_uid));
    player_relics.sort_by(|left, right| left.player_uid.cmp(&right.player_uid));
    if player_partial {
        warnings.push(WARNING_PLAYER_DATA.to_string());
    }

    let world = match extractor::world::extract_world_from_level_gvas(&decoded.payload) {
        Ok(level) => {
            if level.partial {
                warnings.push(WARNING_WORLD.to_string());
            }
            let overview = WorldOverview {
                collectibles: level.collectibles,
                raids: level.raids,
                events: level.events,
                players: player_points,
                game_day: level.game_day,
            };
            match extractor::world::validate_world_overview(&overview) {
                Ok(()) => Some(overview),
                Err(_) => {
                    if !warnings.iter().any(|warning| warning == WARNING_WORLD) {
                        warnings.push(WARNING_WORLD.to_string());
                    }
                    None
                }
            }
        }
        Err(_) => {
            warnings.push(WARNING_WORLD.to_string());
            None
        }
    };

    let metadata_path = save_dir.join("LevelMeta.sav");
    let world_name = match decode_metadata(&metadata_path) {
        Ok(metadata) => metadata.world_name,
        Err(_) => {
            warnings.push(WARNING_LEVEL_META.to_string());
            None
        }
    };

    Ok(WorldDocument {
        schema_version: SCHEMA_VERSION,
        world_name,
        characters,
        player_containers,
        base_camps,
        world,
        player_relics,
        warnings,
    })
}

fn player_files(save_dir: &Path) -> (Vec<PathBuf>, bool) {
    let Ok(entries) = fs::read_dir(save_dir.join("Players")) else {
        return (Vec::new(), true);
    };
    let mut unavailable = false;
    let mut files = entries
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry.path()),
            Err(_) => {
                unavailable = true;
                None
            }
        })
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("sav"))
        })
        .collect::<Vec<_>>();
    files.sort();
    (files, unavailable)
}

fn player_uid_from_path(path: &Path) -> Option<String> {
    let value = path.file_stem()?.to_str()?.to_ascii_lowercase();
    (value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::implementation::extractor::gvas::test_fixture::{
        build_level_meta_sav, build_level_sav, expected_standard_characters,
        standard_world_characters,
    };

    #[test]
    fn accepts_only_32_digit_hexadecimal_player_uids() {
        assert_eq!(
            player_uid_from_path(Path::new("0123456789ABCDEF0123456789ABCDEF.sav")),
            Some("0123456789abcdef0123456789abcdef".to_string())
        );
        assert_eq!(player_uid_from_path(Path::new("player.sav")), None);
        assert_eq!(
            player_uid_from_path(Path::new("0123456789abcdef0123456789abcdeg.sav")),
            None
        );
    }

    #[test]
    fn decodes_a_world_into_a_schema_versioned_neutral_document() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("Level.sav"),
            build_level_sav(&standard_world_characters(), "double-zlib"),
        )
        .unwrap();
        fs::write(
            directory.path().join("LevelMeta.sav"),
            build_level_meta_sav(Some("検証ワールド"), "single-zlib"),
        )
        .unwrap();

        let document = decode_world(directory.path()).unwrap();

        assert_eq!(document.schema_version, 1);
        assert_eq!(document.world_name.as_deref(), Some("検証ワールド"));
        assert_eq!(document.characters, expected_standard_characters());
        assert!(document
            .player_containers
            .pal_storage_container_ids
            .is_empty());
        assert!(document.player_relics.is_empty());
        assert!(document.warnings.contains(&WARNING_PLAYER_DATA.to_string()));
        assert!(!serde_json::to_string(&document)
            .unwrap()
            .contains(directory.path().to_string_lossy().as_ref()));
        let json = serde_json::to_string(&document).unwrap();
        assert!(json.contains("\"playerUId\""));
        assert!(!json.contains("\"playerUid\""));
    }

    #[test]
    fn reports_a_warning_when_the_players_directory_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();

        let document = decode_players(directory.path()).unwrap();

        assert!(document.player_relics.is_empty());
        assert_eq!(document.warnings, vec![WARNING_PLAYER_DATA.to_string()]);
    }
}
