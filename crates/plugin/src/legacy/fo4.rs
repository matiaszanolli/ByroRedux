//! Fallout 4 ESM/ESP parser stub.

use super::LegacyLoadOrder;
use crate::manifest::PluginManifest;
use crate::record::Record;

/// Parse a Fallout 4 ESM/ESP binary into (PluginManifest, Vec<Record>).
/// Full implementation comes in a future phase.
pub fn parse(_data: &[u8], _load_order: &LegacyLoadOrder) -> anyhow::Result<(PluginManifest, Vec<Record>)> {
    todo!("Fallout 4 ESM/ESP parser")
}
