//! Volumetric fog driver — bridges authored fog data into a uniform ECS
//! representation that the M55 volumetrics inject pass will consume in
//! later steps. Part of the "Full M55 promotion + godray emerges for
//! free" workstream tracked at issue #1277.
//!
//! Step 1 (this file): data plumbing only. The component carries the
//! authored values; no consumer reads it yet (volumetrics output is
//! gated off via `VOLUMETRIC_OUTPUT_CONSUMED = false` in
//! `crates/renderer/src/vulkan/volumetrics.rs`).
//!
//! ## Sources that will eventually feed this
//!
//! | Source            | Step | Wired today? |
//! |-------------------|------|--------------|
//! | `NiFogProperty`   | 1    | yes (rare — 1 vanilla instance) |
//! | `XCLL` cell fog   | 1    | follow-up (`CellLightingRes` already exists; cell-scope spawn deferred) |
//! | `REGN` density    | 2    | no |
//! | `WTHR` cell fog   | 3    | no |
//! | Authored mesh     | 5    | no |
//!
//! ## Storage choice
//!
//! `SparseSetStorage` — almost every entity is fog-less, and the
//! consumer (volumetric inject UBO assembly) iterates the small set
//! per frame, not every entity.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// One coherent volumetric-fog region in the scene.
///
/// `bounds == None` means cell-scope (no spatial restriction — applies
/// everywhere visible, like the XCLL depth-fog ramp). `Some` restricts
/// the volume to the bounding sphere; the inject pass will modulate
/// density to zero outside it.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FogVolume {
    /// World-space bounding sphere. `None` = cell-scope.
    pub bounds: Option<FogBounds>,
    /// Fog color, RGB. Stored as raw monitor-space floats per
    /// `feedback_color_space.md` — do NOT srgb-decode at consume time.
    pub color: [f32; 3],
    /// Distance (engine units) at which the linear ramp starts. XCLL
    /// `fog_near` for cell-scope; 0 for NiFogProperty mesh-scope.
    pub near: f32,
    /// Distance (engine units) at which the ramp reaches full
    /// extinction. XCLL `fog_far` for cell-scope; NiFogProperty
    /// `fog_depth` for mesh-scope.
    pub far: f32,
    /// FNV+ XCLL cubic-curve clip distance. When both `clip` and
    /// `power` are `Some`, the curve `pow(dist / clip, power)`
    /// replaces the linear `near..far` ramp.
    pub clip: Option<f32>,
    /// FNV+ XCLL cubic-curve falloff exponent. Paired with `clip`.
    pub power: Option<f32>,
    /// Where this volume came from — diagnostic + dispatch hint for
    /// the future consumer. Never branched on at upload time.
    pub source: FogSource,
}

/// Spatial extent for a non-cell-scope volume. Bounding-sphere
/// representation mirrors [`crate::ecs::WorldBound`].
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FogBounds {
    pub center: Vec3,
    pub radius: f32,
}

/// Provenance of a fog volume. Drives debug output and any
/// per-source modulation the consumer needs; the inject pass
/// otherwise treats every source identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum FogSource {
    /// Cell-wide depth fog from the cell's XCLL record (or LGTM
    /// template fallback) — the dominant source in practice.
    Xcll,
    /// Per-node `NiFogProperty` authored on a NIF. Rare in vanilla
    /// (1 instance across all supported games per
    /// `crates/nif/src/blocks/properties.rs`'s doc comment) but
    /// included for completeness.
    NiFog,
    /// `REGN` per-region density (Step 2, not yet wired).
    Regn,
    /// `WTHR` weather-driven cell-wide density (Step 3, not yet wired).
    Wthr,
    /// Authored fog-volume NIF mesh content — alpha-blended fog
    /// planes the cell artist placed (Step 5, not yet wired).
    AuthoredMesh,
}

impl Component for FogVolume {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_scope_fog_has_no_bounds() {
        let fog = FogVolume {
            bounds: None,
            color: [0.18, 0.14, 0.10],
            near: 64.0,
            far: 4096.0,
            clip: None,
            power: None,
            source: FogSource::Xcll,
        };
        assert!(fog.bounds.is_none());
        assert_eq!(fog.source, FogSource::Xcll);
    }

    #[test]
    fn mesh_scope_fog_carries_bounds_and_source() {
        let fog = FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::new(100.0, 0.0, 50.0),
                radius: 200.0,
            }),
            color: [0.1, 0.4, 0.1],
            near: 0.0,
            far: 200.0,
            clip: None,
            power: None,
            source: FogSource::NiFog,
        };
        assert!(fog.bounds.is_some());
        assert_eq!(fog.source, FogSource::NiFog);
    }

    #[test]
    fn cubic_curve_fields_are_optional_pair() {
        let fog = FogVolume {
            bounds: None,
            color: [0.2, 0.2, 0.25],
            near: 0.0,
            far: 8192.0,
            clip: Some(2000.0),
            power: Some(2.0),
            source: FogSource::Xcll,
        };
        // Pair semantic: consumer should treat both-Some as "use the
        // cubic curve", and either-None as "fall through to linear".
        assert_eq!(fog.clip, Some(2000.0));
        assert_eq!(fog.power, Some(2.0));
    }
}
