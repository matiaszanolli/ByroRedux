//! #2985 / TD9-2026-08-16-03 — pin the hasher on `skin_offsets`, the fourth
//! collection named by the hot-path hashing rule in
//! `.claude/commands/_audit-common.md`. It is probed once per static-mesh draw
//! per frame (`static_meshes.rs`'s `skin_offsets.get(&entity)`), which is what
//! makes SipHash-1-3 the wrong default here (#2985, following #1368 / #2174 /
//! #2923).
//!
//! #2923 shipped guards for the other three — every `SkinSlotPool` collection
//! and the `pose_dirty` set it hands the renderer
//! (`skin_slot_pool_maps_are_not_siphash`,
//! `pose_dirty_accessor_does_not_pin_siphash_across_the_crate_boundary`) and
//! `FrameInputs.pose_dirty` plus `record_skinned_blas_refit`'s parameter
//! (`hot_path_maps_are_fx_hashed`, a type ascription since #6687). `skin_offsets`
//! lives in the binary crate, which is where both prior rounds of this defect
//! class were reintroduced.
//!
//! #6686 — a type ascription, not a text scan. The map is one field,
//! `App.skin_offsets`; every other site that names its type (`render/mod.rs`,
//! `skinned.rs`, `static_meshes.rs`) receives that field by reference, so a
//! single site swapped to `std::collections::HashMap` already fails to compile,
//! and the consistent whole-path swap the old four-file scan existed for has to
//! change the field. The ascription below then stops this module compiling.
//! `App` is defined at the crate root, so its private field is visible here.

/// Never called: the type check is the whole guard.
#[allow(dead_code)]
fn skin_offsets_is_fx_hashed(app: &crate::App) {
    let _: &rustc_hash::FxHashMap<byroredux_core::ecs::EntityId, u32> = &app.skin_offsets;
}
