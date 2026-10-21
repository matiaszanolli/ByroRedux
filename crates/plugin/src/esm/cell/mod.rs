//! Cell, placed reference, and static object extraction from ESM files.
//!
//! Walks the GRUP tree to find interior cells, exterior cells (from WRLD),
//! their placed references (REFR + ACHR), and resolves base form IDs to
//! static/object definitions for NIF paths.

// The `use` aliases below feed the sibling walkers' `use super::*` globs
// (#5805): `EsmReader` / `Result` / `HashMap` are consumed by lib code in
// `support` / `walkers` / `wrld`; `EXTERIOR_CELL_UNITS` only by test
// builds (`tests/cell_for_refr.rs`).
use super::reader::EsmReader;
use anyhow::Result;
use std::collections::HashMap;

#[cfg(test)]
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;

mod base_objects;
mod cell_data;
mod helpers;
mod index;
mod landscape;
mod lighting;
mod placed_ref;
pub(crate) mod support;
pub(crate) mod walkers;
mod worldspace;
pub(crate) mod wrld;

// Re-exported for `parse_esm_with_load_order` (#527 fused walker).
// Internal callers (`tests`, sibling cell helpers) reach the same
// helpers via `cell::support::*` directly.
pub(crate) use support::build_static_object_from_subs;

// #5805 — the per-record-family type definitions moved out of this file
// into the sibling modules above; every public path through `esm::cell::`
// is re-exported below so no caller changes.
pub use base_objects::{
    AddonData, DecalData, DestructionData, DestructionStage, LightData, StaticObject, TextureSet,
};
pub use cell_data::CellData;
pub use index::{
    CellRef, EsmCellIndex, OwnedCellRef, parse_esm_cells, parse_esm_cells_with_load_order,
};
pub use landscape::{LandscapeData, LandscapeTextureHavok, TerrainQuadrant, TerrainTextureLayer};
pub use lighting::{CellLighting, StarfieldLighting};
pub use placed_ref::{
    ActivateParent, CellOwnership, EnableParent, LinkedRef, LockData, PlacedRef, PortalLink,
    PrimitiveBounds, RagdollPoseBone, TeleportDest, TextureSlotSwap,
};
pub use worldspace::{WorldspaceRecord, worldspace_flags};

// `pub(crate)` items siblings reach via `super::` / `crate::esm::cell::`.
pub(crate) use index::canonical_interior_cell_id;
pub(crate) use lighting::decode_starfield_lighting_tail;
// Private test-build alias: `tests/merge.rs` calls this through
// `use super::super::*`, exactly as it did when the fn lived in this file.
#[cfg(test)]
use index::merge_cell_override;

#[cfg(test)]
mod tests;

/// #4075 — `docs/engine/plugin-loading.md` is the document a contributor
/// reads before touching this tier, and its `EsmCellIndex` block had fallen
/// three fields behind the struct. The missing one that mattered was
/// `worldspace_persistent_cells`: a reader working from the doc would
/// conclude persistent exterior refs have no home, when it is where quest
/// actors like Hadvar and Ralof are placed.
///
/// A field list transcribed by hand into prose drifts by default, so assert
/// it instead. Only membership is pinned, not the formatting or the trailing
/// comments — the doc is allowed to present the struct more readably than
/// `rustfmt` does.
#[cfg(test)]
mod plugin_loading_doc_pin_tests {
    const CELL_INDEX_RS: &str = include_str!("index.rs");
    const PLUGIN_LOADING_MD: &str = include_str!("../../../../../docs/engine/plugin-loading.md");

    /// Every `pub <name>:` declared in the `EsmCellIndex` struct body.
    fn declared_fields() -> Vec<String> {
        let start = CELL_INDEX_RS
            .find("pub struct EsmCellIndex {")
            .expect("EsmCellIndex must still be declared here");
        let body = &CELL_INDEX_RS[start..];
        let body = &body[..body.find("\n}").expect("the struct body must close")];
        // Assembled at run time so this module's own text cannot be matched.
        let decl = format!("{} ", "pub");
        body.lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let rest = line.strip_prefix(decl.as_str())?;
                let name = rest.split(':').next()?;
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_')
                    .then(|| name.to_string())
            })
            .collect()
    }

    #[test]
    fn the_documented_esm_cell_index_lists_every_field() {
        let fields = declared_fields();
        assert!(
            fields.len() >= 13,
            "the field scan found only {fields:?} — the extraction broke, not the doc",
        );

        let block_start = PLUGIN_LOADING_MD.find("pub struct EsmCellIndex {").expect(
            "plugin-loading.md must still carry the EsmCellIndex block — it is the \
                 structured output of cell parsing and the doc's reason to exist for \
                 this tier (#4075)",
        );
        let block = &PLUGIN_LOADING_MD[block_start..];
        let block = &block[..block.find("\n}").expect("the documented block must close")];

        for field in &fields {
            assert!(
                block.contains(field.as_str()),
                "`EsmCellIndex::{field}` is missing from plugin-loading.md's struct \
                 block. A contributor reads that block instead of this file, so a \
                 field absent there is a field they will conclude does not exist \
                 (#4075).",
            );
        }
    }

    /// #5689 — the `CellData` block in esm-records.md is a *partial*
    /// field list ("covers, among others"), so the exhaustive direction
    /// the `EsmCellIndex` pin takes cannot apply. The drift that bit was
    /// the reverse: the block named `absorbed_refs`, a field that no
    /// longer exists (#5484 reshaped it into `absorbed_ref_bakes`), and a
    /// reader following the doc re-derives the wrong bake gate. Pin that
    /// every field the block names is a real `CellData` field.
    #[test]
    fn the_documented_cell_data_block_names_only_real_fields() {
        const ESM_RECORDS_MD: &str = include_str!("../../../../../docs/engine/esm-records.md");

        let scan = |source: &str, struct_name: &str| -> Vec<String> {
            let start = source
                .find(&format!("pub struct {struct_name} {{"))
                .unwrap_or_else(|| panic!("{struct_name} must still be declared/carried"));
            let body = &source[start..];
            let body = &body[..body.find("\n}").expect("the block must close")];
            body.lines()
                .filter_map(|line| {
                    let rest = line.trim_start().strip_prefix("pub ")?;
                    let name = rest.split(':').next()?;
                    name.chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_')
                        .then(|| name.to_string())
                })
                .collect()
        };

        const CELL_DATA_RS: &str = include_str!("cell_data.rs");

        let real = scan(CELL_DATA_RS, "CellData");
        assert!(
            real.len() >= 13,
            "the CellData scan found only {real:?} — the extraction broke, not the doc",
        );
        let documented = scan(ESM_RECORDS_MD, "CellData");
        assert!(
            documented.len() >= 13,
            "the doc scan found only {documented:?} — the extraction broke, not the doc",
        );
        for field in &documented {
            assert!(
                real.contains(field),
                "`CellData::{field}` is documented in esm-records.md but no longer \
                 exists — the doc block names a field a reader will look for (#5689).",
            );
        }
    }

    /// The audio line said eight record types were unparsed when three of
    /// them had typed dispatch arms — half-right, which sends an
    /// audio-subsystem author to write parsers that already exist.
    #[test]
    fn the_audio_record_split_matches_the_dispatcher() {
        const DISPATCH_RS: &str = include_str!("../records/dispatch_misc_stub.rs");
        let quote = '"';
        for parsed in ["SOUN", "MUSC", "ASPC"] {
            assert!(
                DISPATCH_RS.contains(&format!("b{quote}{parsed}{quote} =>")),
                "{parsed} lost its dispatch arm — plugin-loading.md lists it as parsed",
            );
        }
        for unparsed in ["SNCT", "SOPM", "MUST", "REVB", "AECH"] {
            assert!(
                !DISPATCH_RS.contains(&format!("b{quote}{unparsed}{quote} =>")),
                "{unparsed} gained a dispatch arm — plugin-loading.md still lists it \
                 under \"Not yet parsed\" (#4075)",
            );
        }
    }
}
