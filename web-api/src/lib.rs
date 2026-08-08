//! Web API application facade.
//!
//! HTTP transport, upload limits, authentication, and background execution
//! belong under `implementation` when they are added. Callers depend only on
//! `DecoderFacade` and the shared neutral document contracts.

mod implementation;

pub use implementation::DecoderFacade;
pub use palsav_decoder::{MetadataDocument, PlayerDocument, PlayersDocument, WorldDocument};
