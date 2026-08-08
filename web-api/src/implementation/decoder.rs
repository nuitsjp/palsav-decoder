use std::path::Path;

use palsav_decoder::{
    decode_metadata, decode_player, decode_players, decode_world, MetadataDocument, PlayerDocument,
    PlayersDocument, WorldDocument,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct DecoderFacade;

impl DecoderFacade {
    pub fn new() -> Self {
        Self
    }

    pub fn world(&self, input: &Path) -> Result<WorldDocument, String> {
        decode_world(input)
    }

    pub fn players(&self, input: &Path) -> Result<PlayersDocument, String> {
        decode_players(input)
    }

    pub fn player(&self, input: &Path) -> Result<PlayerDocument, String> {
        decode_player(input)
    }

    pub fn metadata(&self, input: &Path) -> Result<MetadataDocument, String> {
        decode_metadata(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_delegates_decode_failures_without_transport_dependencies() {
        let directory = tempfile::tempdir().unwrap();

        let result = DecoderFacade::new().metadata(directory.path());

        assert!(result.is_err());
    }
}
