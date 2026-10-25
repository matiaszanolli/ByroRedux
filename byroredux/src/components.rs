//! Application-specific marker components and resources, split by domain
//! (#5467): `render`, `terrain`, `environment`, `indices`, `input`, `audio`,
//! `tuning`, `ai_anim`, `navmesh`; every item is re-exported here so
//! `crate::components::X` paths are unchanged.

mod explicit_start;
mod projectile_muzzle;
mod game_time;

pub(crate) use explicit_start::ExplicitStartPin;
pub(crate) use projectile_muzzle::ProjectileMuzzle;
pub(crate) use game_time::GameTimeRes;

mod render;
mod vats;
mod terrain;
mod environment;
mod indices;
mod input;
mod audio;
mod tuning;
mod ai_anim;
mod navmesh;

pub(crate) use render::*;
pub(crate) use vats::*;
pub(crate) use terrain::*;
pub(crate) use environment::*;
pub(crate) use indices::*;
pub(crate) use input::*;
pub(crate) use audio::*;
pub(crate) use tuning::*;
pub(crate) use ai_anim::*;
pub(crate) use navmesh::*;

#[cfg(test)]
mod fx_hash_guard_tests {
    //! #3677 / PERF-D1-2026-08-30-01 — source-scan pin for `NameIndex.map`
    //! / `SubtreeCache.map` (`components/indices.rs`) and `build_subtree_name_map`'s
    //! return type (`anim_convert.rs`, the function that actually
    //! populates `SubtreeCache.map`'s inner map). Mirrors
    //! `crates/renderer/src/vulkan/context/mod.rs`'s
    //! `pose_dirty_crosses_the_crate_boundary_without_siphash` (#2923):
    //! text-scan, not type-check, so a revert to std's SipHash-1-3 fails
    //! at `cargo test` instead of silently regressing the hot path.

    #[test]
    fn name_index_and_subtree_cache_stay_fx_hash_map() {
        let src = include_str!("components/indices.rs");
        assert!(
            src.contains("pub(crate) map: FxHashMap<FixedString, EntityId>"),
            "NameIndex.map must stay FxHashMap (#3677)"
        );
        assert!(
            src.contains("pub(crate) map: FxHashMap<EntityId, FxHashMap<FixedString, EntityId>>"),
            "SubtreeCache.map must stay FxHashMap (#3677)"
        );
    }

    #[test]
    fn build_subtree_name_map_stays_fx_hash_map() {
        let src = include_str!("anim_convert.rs");
        assert!(
            src.contains(") -> FxHashMap<FixedString, EntityId> {"),
            "build_subtree_name_map must stay FxHashMap (#3677) — it's what \
             actually populates SubtreeCache.map's inner map"
        );
    }

    /// #5198 — `ExposureTuning::default()` must carry the renderer's own
    /// constants, not hand-typed copies: `render_one_frame` pushes this
    /// resource over the context field every frame (including frame 0), so
    /// a drifted default silently overrides the renderer's tuning — 0.5 s
    /// from 7d99ba7f0's #5158 retune never ran because the hand-typed
    /// default still said 0.2 s. The literal side pins the audited values
    /// too, so retuning the renderer constant fails here loudly instead of
    /// silently re-tuning the engine.
    #[test]
    fn exposure_tuning_defaults_match_the_renderer_constants() {
        use crate::components::ExposureTuning;
        use byroredux_renderer::vulkan::exposure::{
            DEFAULT_ADAPTATION_SECONDS, DEFAULT_EXPOSURE,
        };

        let tuning = ExposureTuning::default();
        assert_eq!(tuning.fixed_exposure, DEFAULT_EXPOSURE);
        assert_eq!(tuning.adaptation_seconds, DEFAULT_ADAPTATION_SECONDS);
        assert_eq!(DEFAULT_EXPOSURE, 0.85);
        assert_eq!(DEFAULT_ADAPTATION_SECONDS, 0.5);
    }
}
