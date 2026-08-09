//! Shared decoding facade for the CLI and Web API delivery surfaces.
//!
//! Implementation details stay private under `implementation`; consumers use
//! the stable document types and decoding functions re-exported here.

mod implementation;

pub use implementation::contract::{
    write_metadata, write_player, write_players, write_world, MetadataDocument, OutputFormat,
    PlayerDocument, PlayersDocument, WorldDocument, SCHEMA_VERSION,
};
pub use implementation::decoder::{
    assemble_world, decode_level_bytes, decode_metadata, decode_metadata_bytes, decode_player,
    decode_player_bytes, decode_players, decode_world,
};
pub use implementation::model::{
    CacheCharacter, DecodedBaseCamp, DecodedPlayerRelics, JsNumber, PlayerContainerIndex,
    PlayerRelicState, WorldCollectible, WorldEventPoint, WorldOverview, WorldPlayerPoint,
    WorldPoint, WorldRaid,
};
