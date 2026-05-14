//! Animation clip import from NIF/KF files.
//!
//! Converts NiControllerSequence blocks (with their referenced interpolators
//! and keyframe data) into engine-friendly `AnimationClip` structures that
//! are decoupled from the NIF block graph.
//!
//! ## Module layout
//!
//! Split out of the 2 101-LOC monolith into per-phase submodules:
//!
//! - [`coord`] — Zup→Yup coordinate conversion helpers
//! - [`entry`] — public entry points (`import_kf`, `import_embedded_animations`)
//! - [`sequence`] — per-`NiControllerSequence` import
//! - [`controlled_block`] — string + target resolution for controlled blocks
//! - [`transform`] — TRS channel extraction from transform interpolators
//! - [`bspline`] — compressed B-spline evaluation (#155)
//! - [`channel`] — Float / Color / Bool / texture-transform channels
//! - [`keys`] — key conversion utilities + Euler ↔ quat math
//!
//! Every type from [`types`] is `pub use`-d at this module's root so
//! external callers (`crate::anim::AnimationClip` etc.) keep working
//! unchanged.

mod bspline;
mod channel;
mod controlled_block;
mod coord;
mod entry;
mod keys;
mod sequence;
mod transform;
mod types;

// Internal re-exports for the test sibling. The pre-split monolith
// brought these into anim.rs's scope via private `use` statements;
// the test module saw them transitively via `use super::*;`. Mirror
// that here so the test file can keep its concise import block.

pub use entry::{import_embedded_animations, import_kf};
pub use types::*;

// Internal cross-sibling re-exports. Each sibling does `use super::*;`
// at its top to pull in the helpers it needs from peers; the
// `pub(crate)` ceiling keeps these private to `byroredux_nif`. Tests
// see them via the same glob.
pub(crate) use bspline::*;
pub(crate) use channel::*;
pub(crate) use controlled_block::*;
pub(crate) use coord::*;
pub(crate) use keys::*;
pub(crate) use sequence::*;
pub(crate) use transform::*;


/// Sampling rate for B-spline interpolators during import.
/// 30 Hz matches the typical Bethesda animation frame rate.
pub(crate) const BSPLINE_SAMPLE_HZ: f32 = 30.0;

/// Degree of the open uniform B-spline used by Gamebryo/Creation engine.
/// Always 3 (cubic) per nif.xml / legacy Gamebryo source.
pub(crate) const BSPLINE_DEGREE: usize = 3;

#[cfg(test)]
mod tests;
