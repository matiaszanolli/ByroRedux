//! Tests for the animation-import pipeline.
//!
//! Each test reaches into the sibling whose function it exercises.
//! Session 35 split the *production* code into per-phase files (`coord`,
//! `controlled_block`, `transform`, `sequence`, `keys`, `channel`,
//! `bspline`); #2199 / TD1-NEW-04 finishes the job on the test side, which
//! had accumulated back to 2002 LOC in a single file. The modules below
//! mirror the production phases they cover.
//!
//! Pure code movement — every test body is byte-identical to its pre-split
//! form; only the module it lives in changed.

use crate::blocks::controller::ControlledBlock;

mod bspline;
mod channel;
mod coord_keys;
mod sanitize;
mod sequence;
mod transform;

/// Shared fixture: a `ControlledBlock` with every ref null and every offset
/// zero, for tests that only care about one field. Used by both `transform`
/// and `sequence`, so it lives here rather than in either.
fn dummy_controlled_block() -> ControlledBlock {
    ControlledBlock {
        interpolator_ref: crate::types::BlockRef::NULL,
        controller_ref: crate::types::BlockRef::NULL,
        priority: 0,
        node_name: None,
        property_type: None,
        controller_type: None,
        controller_id: None,
        interpolator_id: None,
        string_palette_ref: crate::types::BlockRef::NULL,
        node_name_offset: 0,
        property_type_offset: 0,
        controller_type_offset: 0,
        controller_id_offset: 0,
        interpolator_id_offset: 0,
    }
}
