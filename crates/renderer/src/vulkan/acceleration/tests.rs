//! Unit tests for the acceleration submodules.
//!
//! Lifted from the monolithic `acceleration.rs::tests` block — every
//! test exercises a pure predicate (no live Vulkan context).

use super::predicates::*;
use super::*;
use crate::vulkan::context::DrawCommand;

/// Minimal `DrawCommand` builder for the TLAS-eligibility unit
/// tests. Only `in_tlas` and `is_water` are read by
/// [`draw_command_eligible_for_tlas`]; every other field gets a
/// zero/default value. Same pattern as the `cmd` builder in
/// `context::draw::is_caustic_source_tests`.
fn make_draw_command(in_tlas: bool, is_water: bool) -> DrawCommand {
    DrawCommand {
        mesh_handle: 0,
        texture_handle: 0,
        model_matrix: [0.0; 16],
        alpha_blend: false,
        src_blend: 6,
        dst_blend: 7,
        two_sided: false,
        wireframe: false,
        flat_shading: false,
        is_decal: false,
        render_layer: byroredux_core::ecs::components::RenderLayer::Architecture,
        bone_offset: 0,
        normal_map_index: 0,
        dark_map_index: 0,
        glow_map_index: 0,
        detail_map_index: 0,
        gloss_map_index: 0,
        parallax_map_index: 0,
        parallax_height_scale: 0.0,
        parallax_max_passes: 0.0,
        env_map_index: 0,
        env_mask_index: 0,
        alpha_threshold: 0.0,
        alpha_test_func: 0,
        roughness: 0.5,
        metalness: 0.0,
        ior: 1.5,        // #1248
        subsurface: 0.0, // #1249
        sheen: 0.0,
        sheen_tint: 0.0,
        anisotropic: 0.0, // #1250
        emissive_mult: 0.0,
        emissive_color: [0.0; 3],
        specular_strength: 0.0,
        specular_color: [0.0; 3],
        diffuse_color: [1.0; 3],
        ambient_color: [1.0; 3],
        vertex_offset: 0,
        index_offset: 0,
        vertex_count: 0,
        sort_depth: 0,
        in_tlas,
        in_raster: true,
        avg_albedo: [0.0; 3],
        material_kind: 0,
        z_test: true,
        z_write: true,
        z_function: 3,
        terrain_tile_index: None,
        entity_id: 0,
        uv_offset: [0.0; 2],
        uv_scale: [1.0; 2],
        material_alpha: 1.0,
        skin_tint_rgba: [0.0; 4],
        hair_tint_rgb: [0.0; 3],
        multi_layer_envmap_strength: 0.0,
        eye_left_center: [0.0; 3],
        eye_cubemap_scale: 0.0,
        eye_right_center: [0.0; 3],
        multi_layer_inner_thickness: 0.0,
        multi_layer_refraction_scale: 0.0,
        multi_layer_inner_scale: [0.0; 2],
        sparkle_rgba: [0.0; 4],
        effect_falloff: [0.0; 5],
        material_id: 0,
        vertex_color_emissive: false,
        effect_shader_flags: 0,
        greyscale_lut_index: 0,
        supplemental_texture_indices: [0; 12],
        translucency_subsurface_color: [0.0; 3],
        translucency_transmissive_scale: 0.0,
        translucency_turbulence: 0.0,
        is_water,
    }
}

// ── #1024 / F-WAT-03 — water TLAS-exclusion contract ──────────

/// The hot path: a regular opaque draw with `in_tlas=true` and
/// `is_water=false` is eligible for TLAS instancing.
#[test]
fn regular_opaque_draw_is_tlas_eligible() {
    let cmd = make_draw_command(true, false);
    assert!(draw_command_eligible_for_tlas(&cmd));
}

/// Particles / UI quads opt out via `in_tlas=false` — already
/// pinned by the SSBO-builder contract (#516) but exercised here
/// alongside the new water gate so a future refactor of
/// `draw_command_eligible_for_tlas` keeps both axes load-bearing.
#[test]
fn non_tlas_draw_is_excluded() {
    let cmd = make_draw_command(false, false);
    assert!(!draw_command_eligible_for_tlas(&cmd));
}

/// Core regression. Water surfaces must be excluded from the
/// TLAS even if `in_tlas=true`. Pre-#1024 this case relied on
/// the cell loader's `for_rt=false` mesh upload to keep the
/// water mesh out of `blas_entries`; any future code path that
/// adds water to the BLAS pool (e.g. caustic-source meshes
/// sharing a handle) would silently reintroduce ray self-hits.
/// This predicate makes `is_water` the load-bearing gate.
#[test]
fn water_draw_excluded_even_with_in_tlas_set() {
    let cmd = make_draw_command(true, true);
    assert!(
        !draw_command_eligible_for_tlas(&cmd),
        "is_water=true must exclude the draw from the TLAS regardless of in_tlas"
    );
}

/// Both opt-outs at once — degenerate case but pinned so a
/// future short-circuit refactor (e.g. early-return on `is_water`)
/// doesn't accidentally invert the `in_tlas` branch.
#[test]
fn water_and_non_tlas_both_excluded() {
    let cmd = make_draw_command(false, true);
    assert!(!draw_command_eligible_for_tlas(&cmd));
}

/// BSEffectShader surfaces need TLAS presence for optical rays. Their
/// dedicated visibility mask—not global TLAS deletion—keeps them from
/// becoming opaque shadow casters. Skyrim's alchemy-workbench glass depends
/// on this: its outer shell refracts authored InnerHaze effect geometry.
#[test]
fn effect_shader_surface_is_tlas_eligible_for_optical_rays() {
    let mut cmd = make_draw_command(true, false);
    cmd.material_kind = crate::MATERIAL_KIND_EFFECT_SHADER;
    assert!(draw_command_eligible_for_tlas(&cmd));
}

/// Regression for #2297 / MAT-D1-NEW-02. `MATERIAL_KIND_FIRE_REFRACTION`
/// is documented raster-only (`scene_buffer::constants`: "must not cast
/// shadows, receive GI hits, enter reflections, or synthesize a physics
/// collider") — must be excluded from the TLAS even with `in_tlas=true`,
/// mirroring the `is_water` precedent. Today `render::static_meshes`
/// already computes `in_tlas=false` for this kind, so this is
/// defense-in-depth against a future producer that forgets to.
#[test]
fn fire_refraction_surface_excluded_even_with_in_tlas_set() {
    let mut cmd = make_draw_command(true, false);
    cmd.material_kind = crate::MATERIAL_KIND_FIRE_REFRACTION;
    assert!(
        !draw_command_eligible_for_tlas(&cmd),
        "MATERIAL_KIND_FIRE_REFRACTION must exclude the draw from the \
         TLAS regardless of in_tlas (raster-only per its own doc contract)"
    );
}

/// Regression for #679 / AS-8-9. The skinned-BLAS rebuild
/// predicate must fire only when the in-place refit chain has
/// reached the configured threshold; below the threshold the
/// BLAS keeps refitting cheaply.
#[test]
fn skinned_blas_rebuild_predicate_thresholds() {
    // Below threshold — keep refitting.
    assert!(!should_rebuild_skinned_blas_after(0));
    assert!(!should_rebuild_skinned_blas_after(1));
    assert!(!should_rebuild_skinned_blas_after(
        SKINNED_BLAS_REFIT_THRESHOLD - 1
    ));
    // At threshold — fire.
    assert!(should_rebuild_skinned_blas_after(
        SKINNED_BLAS_REFIT_THRESHOLD
    ));
    // Above threshold — fire (caller missed a frame; still rebuild).
    assert!(should_rebuild_skinned_blas_after(
        SKINNED_BLAS_REFIT_THRESHOLD + 1
    ));
    assert!(should_rebuild_skinned_blas_after(u32::MAX));
}

// ── #907 / REN-D12-NEW-01 — refit-counts VUID guard ────────────

/// Identity case: same counts at BUILD and refit → no error. Pins
/// the happy path so a future refactor that breaks the check
/// (e.g. inverts the equality test) fails this test immediately
/// instead of falling through to a real Vulkan refit.
#[test]
fn validate_refit_counts_accepts_matching_counts() {
    assert!(validate_refit_counts(100, 300, 100, 300).is_ok());
    assert!(validate_refit_counts(0, 0, 0, 0).is_ok());
    assert!(validate_refit_counts(u32::MAX, u32::MAX, u32::MAX, u32::MAX).is_ok());
}

/// Vertex-count drift only — typical for a LOD-down swap (same
/// triangle count but fewer unique verts after merging). Vulkan
/// VUID 03667 is strict on `primitiveCount` but we also pin
/// vertex_count to catch this earlier than the
/// max_vertex-based VUIDs.
#[test]
fn validate_refit_counts_rejects_vertex_only_drift() {
    let err =
        validate_refit_counts(100, 300, 80, 300).expect_err("vertex-count drift must be rejected");
    assert!(err.contains("v=100") && err.contains("v=80"));
    assert!(err.contains("03667"));
}

/// Index-count drift — the spec-strict case. UPDATE-mode at a
/// different `primitiveCount` is undefined behaviour on every
/// driver; silent BVH corruption on NVIDIA per the issue body.
#[test]
fn validate_refit_counts_rejects_index_only_drift() {
    let err = validate_refit_counts(100, 300, 100, 240)
        .expect_err("index-count drift must be rejected (primitiveCount mismatch)");
    assert!(err.contains("i=300") && err.contains("i=240"));
}

/// Both axes drift — full mesh swap. Same rejection path.
#[test]
fn validate_refit_counts_rejects_full_mesh_swap() {
    assert!(validate_refit_counts(100, 300, 80, 240).is_err());
}

// ── #1145 / SAFE-D6-NEW-01 — flag-set half of VUID-03667 ────────────

/// Identity case: same flag-set at BUILD and refit → no error.
#[test]
fn validate_refit_flags_accepts_matching_flags() {
    use ash::vk::BuildAccelerationStructureFlagsKHR as F;
    assert!(validate_refit_flags(
        F::PREFER_FAST_BUILD | F::ALLOW_UPDATE,
        F::PREFER_FAST_BUILD | F::ALLOW_UPDATE
    )
    .is_ok());
    assert!(validate_refit_flags(F::empty(), F::empty()).is_ok());
}

/// SKINNED_BLAS_FLAGS vs UPDATABLE_AS_FLAGS — the realistic future
/// drift the audit calls out. A BUILD site mistakenly using the TLAS
/// flag constant (PREFER_FAST_TRACE) where the matching UPDATE uses
/// the skinned constant (PREFER_FAST_BUILD) trips VUID-03667.
#[test]
fn validate_refit_flags_rejects_skinned_vs_updatable_drift() {
    let err = validate_refit_flags(
        super::constants::UPDATABLE_AS_FLAGS,
        super::constants::SKINNED_BLAS_FLAGS,
    )
    .expect_err("FAST_TRACE vs FAST_BUILD drift must be rejected");
    assert!(err.contains("03667"));
}

/// ALLOW_COMPACTION accidentally added on one side — VUID-03667
/// fires on UPDATE because the bit set changed.
#[test]
fn validate_refit_flags_rejects_allow_compaction_drift() {
    use ash::vk::BuildAccelerationStructureFlagsKHR as F;
    let with_compaction = F::PREFER_FAST_BUILD | F::ALLOW_UPDATE | F::ALLOW_COMPACTION;
    let without = F::PREFER_FAST_BUILD | F::ALLOW_UPDATE;
    assert!(validate_refit_flags(without, with_compaction).is_err());
    assert!(validate_refit_flags(with_compaction, without).is_err());
}

/// Sibling check: the threshold must be a sane number of frames.
/// At 60 FPS the issue suggested ~10 s = 600 frames — too low
/// would thrash the rebuild path, too high defeats the bug fix.
#[test]
fn skinned_blas_threshold_is_in_sane_range() {
    // 5 s ≤ threshold ≤ 30 s at 60 FPS.
    const {
        assert!(SKINNED_BLAS_REFIT_THRESHOLD >= 300);
        assert!(SKINNED_BLAS_REFIT_THRESHOLD <= 1800);
    }
}

/// Regression for #504: the scratch-shrink helper must reclaim
/// capacity after a past peak frame while leaving small working
/// sets alone. Exercised on a plain `Vec<u8>` — the algorithm is
/// size-agnostic, so `Vec<vk::AccelerationStructureInstanceKHR>`
/// (the real caller) follows the same math.
#[test]
fn shrink_scratch_reclaims_capacity_after_peak() {
    // Target = 2 × max(working_set, floor) = 2 × max(50, 512) = 1024.
    // The literal "1024" in the asserts below is this product, not
    // the `BINDLESS_CEILING = 65535` constant or any other in-tree
    // 1024-shaped value; bumping the floor will move both.
    const FLOOR: usize = 512;
    const TARGET: usize = 2 * FLOOR;
    // 10 000-entry peak, then a tiny steady-state restore.
    let mut v: Vec<u8> = Vec::with_capacity(10_000);
    shrink_scratch_if_oversized(&mut v, 50, FLOOR);
    assert!(
        v.capacity() <= TARGET,
        "expected capacity <= {TARGET}, got {}",
        v.capacity()
    );
    // Floor honoured — NOT shrunk to `working_set` alone (50).
    assert!(
        v.capacity() >= FLOOR,
        "floor must keep capacity above working-set for small frames"
    );
}

/// Near-steady state: capacity just over the 2× band must not
/// trigger a shrink (avoids thrashing when the working set
/// oscillates around the peak).
#[test]
fn shrink_scratch_preserves_hysteresis_band() {
    // Same target-derivation note as above: TARGET = 2 × FLOOR; not
    // BINDLESS_CEILING.
    const FLOOR: usize = 512;
    const TARGET: usize = 2 * FLOOR;
    // Working set 500, floor 512, target = 2 × max(500, 512) = 1024.
    // Capacity 1500 > target → shrink.
    let mut over: Vec<u8> = Vec::with_capacity(1500);
    shrink_scratch_if_oversized(&mut over, 500, FLOOR);
    assert!(over.capacity() <= TARGET);

    // Capacity == target → NO shrink (equality falls into the
    // "leave alone" branch).
    let mut at: Vec<u8> = Vec::with_capacity(TARGET);
    shrink_scratch_if_oversized(&mut at, 500, FLOOR);
    assert_eq!(
        at.capacity(),
        TARGET,
        "at-target capacity must not be touched"
    );

    // Capacity below 2× — leave alone, we're already efficient.
    let mut under: Vec<u8> = Vec::with_capacity(800);
    shrink_scratch_if_oversized(&mut under, 500, FLOOR);
    assert_eq!(under.capacity(), 800);
}

/// #2486 / D5-01 — the map variant of the same policy, used for the two
/// rigid-motion history maps. `HashMap::shrink_to` is documented as a lower
/// bound (the table rounds up to its own capacity policy), so the peak case
/// asserts "reclaimed something and can still hold the floor" rather than an
/// exact capacity the way the `Vec` test can.
#[test]
fn shrink_map_scratch_reclaims_capacity_after_peak() {
    use std::collections::HashMap;
    const FLOOR: usize = 512;

    let mut peak: HashMap<u32, [f32; 16]> = HashMap::with_capacity(10_000);
    let peak_capacity = peak.capacity();
    shrink_map_scratch_if_oversized(&mut peak, 50, FLOOR);
    assert!(
        peak.capacity() < peak_capacity,
        "a 10k peak with a 50-entry working set must give capacity back, \
         still had {}",
        peak.capacity()
    );
    assert!(
        peak.capacity() >= FLOOR,
        "the floor must survive the shrink so small frames don't realloc, \
         got {}",
        peak.capacity()
    );

    // Inside the 2× hysteresis band — left alone, exactly like the Vec.
    let mut under: HashMap<u32, [f32; 16]> = HashMap::with_capacity(800);
    let under_capacity = under.capacity();
    shrink_map_scratch_if_oversized(&mut under, 500, FLOOR);
    assert_eq!(
        under.capacity(),
        under_capacity,
        "capacity within the 2× band must not be touched"
    );

    // Live entries survive: this runs on `previous_rigid_models` while it
    // holds the frame's history, so a shrink that dropped entries would
    // silently zero out motion vectors.
    let mut live: HashMap<u32, [f32; 16]> = HashMap::with_capacity(10_000);
    for id in 0..40u32 {
        live.insert(id, [id as f32; 16]);
    }
    let live_working = live.len();
    shrink_map_scratch_if_oversized(&mut live, live_working, FLOOR);
    assert_eq!(live.len(), 40);
    assert_eq!(live.get(&7), Some(&[7.0; 16]));
}

/// Regression for #645 / MEM-2-3: the TLAS-instance-buffer shrink
/// predicate must fire when a past peak (e.g. 32 K-instance
/// exterior cell) has settled back into a small working set, but
/// must NOT thrash when the working set is close to the current
/// capacity. SLACK is 1 MB (≈16 K instances).
#[test]
fn tlas_instance_should_shrink_fires_after_exterior_peak() {
    const STRIDE: vk::DeviceSize = 64;
    // 32 K-instance peak (= 2 MB) settling into an 8 K-instance
    // small interior (= 512 KB working). Capacity is 4× working
    // and 1.5 MB > 1 MB SLACK → shrink.
    let current = 32_768 * STRIDE;
    let working = 8_192 * STRIDE;
    assert!(tlas_instance_should_shrink(current, working));
}

#[test]
fn tlas_instance_should_shrink_holds_inside_2x_band() {
    const STRIDE: vk::DeviceSize = 64;
    // Capacity 16 K instances (= 1 MB), working 12 K instances
    // (= 768 KB). Capacity is < 2 × working → don't shrink (the
    // 2× hysteresis still holds even before the slack check).
    let current = 16_384 * STRIDE;
    let working = 12_288 * STRIDE;
    assert!(!tlas_instance_should_shrink(current, working));
}

#[test]
fn tlas_instance_should_shrink_holds_below_slack() {
    const STRIDE: vk::DeviceSize = 64;
    // Capacity 16 K (= 1 MB), working 4 K (= 256 KB). Ratio is
    // 4× (above 2×) but `current - working = 768 KB` is below
    // the 1 MB SLACK → leave alone, we're already small enough
    // that a destroy-and-recreate would burn more than it saves.
    let current = 16_384 * STRIDE;
    let working = 4_096 * STRIDE;
    assert!(!tlas_instance_should_shrink(current, working));
}

#[test]
fn tlas_instance_should_shrink_zero_working_set_with_big_peak() {
    const STRIDE: vk::DeviceSize = 64;
    // 32 K-instance peak with zero working — far above the 2×
    // band and 2 MB > 1 MB SLACK → shrink. (The
    // `shrink_tlas_to_fit` wrapper imposes a `WORKING_SET_FLOOR`
    // of 8 192 on its caller-passed working count, so the
    // raw-zero case here is for the helper's algebraic
    // contract; the wrapper's floor is what callers see.)
    let current = 32_768 * STRIDE;
    let working = 0;
    assert!(tlas_instance_should_shrink(current, working));
}

/// Zero working set must still honour the floor — don't shrink
/// to zero just because the current frame emitted no draws.
#[test]
fn shrink_scratch_zero_working_set_keeps_floor() {
    // Same derivation as above tests — TARGET = 2 × FLOOR.
    const FLOOR: usize = 512;
    const TARGET: usize = 2 * FLOOR;
    let mut v: Vec<u8> = Vec::with_capacity(5000);
    shrink_scratch_if_oversized(&mut v, 0, FLOOR);
    assert!(v.capacity() >= FLOOR, "floor must survive zero working set");
    assert!(
        v.capacity() <= TARGET,
        "shrink must still fire above 2 × floor"
    );
}

/// Regression for #510: the mid-batch eviction predicate must
/// fire at ≥ 90% of the configured budget and stay quiet below.
/// Uses integer-only arithmetic so the threshold is consistent
/// between 32- and 64-bit `DeviceSize` builds.
#[test]
fn should_evict_mid_batch_fires_at_ninety_percent() {
    let budget: vk::DeviceSize = 1_000_000_000; // 1 GB

    // Exactly 90%: projected == budget * 9 / 10 → fires.
    assert!(should_evict_mid_batch(700_000_000, 200_000_000, budget));

    // Exactly at the boundary: 900 MB projected, 900 MB trigger.
    assert!(should_evict_mid_batch(600_000_000, 300_000_000, budget));

    // One byte under 90%: must NOT fire.
    assert!(!should_evict_mid_batch(500_000_000, 399_999_999, budget));

    // Well under: empty live + small pending.
    assert!(!should_evict_mid_batch(0, 10_000_000, budget));

    // Saturating-add guards against overflow when a bogus caller
    // passes near-u64::MAX for pending. Must not panic.
    let _ = should_evict_mid_batch(u64::MAX / 2, u64::MAX / 2, budget);

    // Zero budget — eviction always fires (degenerate
    // configuration; `compute_blas_budget` floors at 256 MB so
    // this path can't hit in practice, but the predicate must
    // not panic or treat zero budget as "under").
    assert!(should_evict_mid_batch(1, 0, 0));
    assert!(should_evict_mid_batch(0, 0, 0));
}

/// Regression for #920 (REN-D12-NEW-03). The mid-batch + LRU
/// eviction predicates must compare *static* BLAS bytes against the
/// budget, not *total* BLAS bytes. Without the split, an NPC-heavy
/// scene whose skinned BLAS push `total_blas_bytes` over budget
/// would LRU-thrash static BLAS every frame even though no static
/// eviction actually frees the over-budget skinned residency.
///
/// This pins the predicate's input contract: the same static
/// footprint must be reported as "under budget" when that's the
/// truth, regardless of whatever skinned-BLAS-driven `total_bytes`
/// happens to be — because skinned bytes can't be freed via
/// eviction.
#[test]
fn evict_predicate_uses_static_bytes_not_total_post_920() {
    let budget: vk::DeviceSize = 1_000_000_000; // 1 GB
                                                // Realistic post-M41 NPC-heavy scene:
                                                // - Static interior-cell BLAS resident: 700 MB (under 90%).
                                                // - 50 skinned NPCs at ~10 MB each: 500 MB skinned residency.
                                                // - Total: 1200 MB (over budget!).
    let static_bytes: vk::DeviceSize = 700_000_000;
    let pending_static_bytes: vk::DeviceSize = 0;
    // Pre-#920 the caller passed (static + skinned). Verify that
    // the FIXED inputs do NOT trip the predicate — even though the
    // total residency *would* exceed 90% of budget.
    assert!(
        !should_evict_mid_batch(static_bytes, pending_static_bytes, budget),
        "static @ 70% must not trigger eviction even with skinned residency \
         pushing total over 90% — eviction can only free static BLAS",
    );

    // Cross-check: if static itself climbs past 90%, the predicate
    // *should* still fire — the fix preserves the threshold for
    // legitimate static pressure.
    let static_at_threshold: vk::DeviceSize = 900_000_000;
    assert!(
        should_evict_mid_batch(static_at_threshold, 0, budget),
        "static @ 90% must trigger eviction (threshold preserved)",
    );
}

/// #1792 / PERF-D3-NEW-01 — `evict_unused_blas`'s actual reclaim gate
/// must account for a mid-batch caller's `pending_bytes`, not just the
/// pre-batch committed `static_blas_bytes`. Pre-fix, on a fresh cell
/// load (`static_blas_bytes == 0`) a batch could grow arbitrarily large
/// mid-loop and this gate would still read "under budget" — the fix
/// this predicate backs is what makes `should_evict_mid_batch`'s
/// trigger (tested above) actually consequential.
#[test]
fn blas_over_budget_accounts_for_pending_bytes() {
    let budget: vk::DeviceSize = 1_000_000_000; // 1 GB

    // The exact failure mode: fresh load, nothing committed yet, but a
    // huge batch has already sized 1.2 GB of result buffers this batch.
    assert!(
        blas_over_budget(0, 1_200_000_000, budget),
        "zero committed but 1.2 GB pending must read over-budget — this \
         is the case the pre-fix gate (static_blas_bytes alone) missed \
         entirely on a fresh load"
    );

    // Committed alone already over budget (the pre-existing, always-
    // worked case) — pending_bytes = 0 must not mask it.
    assert!(blas_over_budget(1_200_000_000, 0, budget));

    // Committed + pending combine to cross the line even though neither
    // alone would.
    assert!(blas_over_budget(600_000_000, 500_000_000, budget));

    // Under budget on both counts — must not fire.
    assert!(!blas_over_budget(400_000_000, 100_000_000, budget));

    // Exactly at the 100% line (not over) — the loop break / gate use
    // `>`, matching the pre-fix `static_blas_bytes <= budget` early
    // return, so exactly-at-budget must NOT be "over".
    assert!(!blas_over_budget(budget, 0, budget));
    assert!(blas_over_budget(budget, 1, budget));

    // Saturating-add guards against overflow from a bogus caller.
    let _ = blas_over_budget(u64::MAX / 2, u64::MAX / 2, budget);
}

/// Regression: #300 — when `needs_full_rebuild` is set, the
/// per-instance address zip-compare must be skipped (the call is
/// going to BUILD regardless, so paying O(N) is pure waste).
#[test]
fn decide_skips_zip_when_needs_full_rebuild() {
    // Even with identical address lists, needs_full_rebuild forces
    // BUILD and the zip is not run.
    let cached = vec![1u64, 2, 3];
    let current = vec![1u64, 2, 3];
    let (use_update, did_zip) = decide_use_update(true, 0, 0, &cached, &current);
    assert!(!use_update, "needs_full_rebuild forces BUILD");
    assert!(!did_zip, "comparison must be skipped — short-circuit");
}

/// Regression: #300 — when the BLAS map generation has bumped
/// since the last build (cell load / unload / eviction frame),
/// the per-instance address compare is also skipped because
/// addresses might have shifted and we're going to BUILD anyway.
#[test]
fn decide_skips_zip_when_blas_map_dirty() {
    let cached = vec![1u64, 2, 3];
    let current = vec![1u64, 2, 3];
    // last_gen=5, current=7 → BLAS map changed since last build.
    let (use_update, did_zip) = decide_use_update(false, 5, 7, &cached, &current);
    assert!(!use_update, "blas_map_dirty forces BUILD");
    assert!(!did_zip, "comparison must be skipped — short-circuit");
}

/// Steady state — no rebuild needed, BLAS map unchanged. The zip
/// runs to detect frustum / draw-list composition changes (which
/// are invisible to the dirty flag).
#[test]
fn decide_runs_zip_when_steady_state_layout_matches() {
    let cached = vec![1u64, 2, 3];
    let current = vec![1u64, 2, 3];
    let (use_update, did_zip) = decide_use_update(false, 7, 7, &cached, &current);
    assert!(use_update, "matching steady state must use UPDATE");
    assert!(did_zip, "comparison must run to verify per-slot match");
}

/// Steady state but composition shifted (frustum culling brought
/// a different mesh into a slot). The zip catches the mismatch
/// and forces BUILD.
#[test]
fn decide_forces_build_when_layout_diverges() {
    let cached = vec![1u64, 2, 3];
    let current = vec![1u64, 2, 99]; // slot 2 now refers to a different BLAS
    let (use_update, did_zip) = decide_use_update(false, 7, 7, &cached, &current);
    assert!(!use_update, "diverging slot forces BUILD");
    assert!(did_zip, "comparison must run — that's how we noticed");
}

/// Length mismatch (entity spawned/despawned without the BLAS map
/// noticing — e.g. an entity with an existing-mesh handle joined
/// the in_tlas set). The zip-compare's length check catches this.
#[test]
fn decide_forces_build_when_lengths_differ() {
    let cached = vec![1u64, 2, 3];
    let current = vec![1u64, 2, 3, 4];
    let (use_update, did_zip) = decide_use_update(false, 7, 7, &cached, &current);
    assert!(!use_update);
    assert!(did_zip);
}

/// Sentinel from the freshly-created TlasState (`u64::MAX`) must
/// never accidentally match a real generation. Forces BUILD on
/// the very first frame after creation regardless of input
/// identity.
#[test]
fn decide_first_frame_after_tlas_creation_builds() {
    let cached: Vec<u64> = Vec::new();
    let current = vec![1u64, 2, 3];
    let (use_update, did_zip) = decide_use_update(true, u64::MAX, 0, &cached, &current);
    assert!(!use_update);
    assert!(!did_zip);
}

/// Regression: #657. Two empty address lists must NOT zip-match
/// into UPDATE — the helper has to force BUILD when this frame
/// has no instances, regardless of the dirty / generation flags.
/// Pre-fix `(false, last_gen, last_gen, &[], &[])` returned
/// `(true, true)`; the call site was masked only by
/// `needs_full_rebuild = true` at TLAS creation.
#[test]
fn decide_empty_current_forces_build() {
    let cached: Vec<u64> = Vec::new();
    let current: Vec<u64> = Vec::new();
    let (use_update, did_zip) = decide_use_update(false, 7, 7, &cached, &current);
    assert!(!use_update, "empty instance list must force BUILD");
    assert!(!did_zip, "must short-circuit before zip");

    // And with a non-empty cached prior frame too — the previous
    // frame had instances, this one does not.
    let cached_nonempty = vec![1u64, 2, 3];
    let (use_update, did_zip) = decide_use_update(false, 7, 7, &cached_nonempty, &current);
    assert!(!use_update);
    assert!(!did_zip);
}

/// #1096 / REN-D8-002 — pin the skip→add round-trip for
/// `last_blas_addresses` decay. A draw command can be skipped on frame N
/// because its BLAS is still under construction (`tlas_handle()` returned
/// None); the next frame the BLAS is built and the draw is re-emitted.
/// The `decide_use_update` zip-compare must catch this address-set
/// change and force a BUILD rather than a stale-source UPDATE.
///
/// Scenario:
///   Frame N:   `current = [a, _, c]` (b skipped — missing BLAS), but the
///              caller's `build_instance_map` filters out the skipped
///              draw so the *actual* address slice fed to `decide` is
///              `[a, c]`. `last_blas_addresses` from the previous BUILD
///              had `[a, b, c]`.
///   Frame N+1: BLAS for `b` finishes; `current = [a, b, c]`. The
///              `last_blas_addresses` after frame N's BUILD is now
///              `[a, c]`. The address-set differs → must BUILD.
#[test]
fn decide_use_update_skip_then_add_round_trip_forces_build() {
    // Frame N: post-skip state, address sequence has shrunk by one.
    let cached_after_skip = vec![1u64, 3]; // pre-frame-N had [1,2,3]; b=2 was skipped
    let current_full = vec![1u64, 2, 3]; // frame N+1: BLAS for b is back

    // Same generation across both frames (no BLAS-map mutation), no
    // forced full rebuild — the address-zip is the only signal.
    let (use_update, did_zip) = decide_use_update(false, 7, 7, &cached_after_skip, &current_full);
    assert!(
        !use_update,
        "skip→add transition (address-set change) must force BUILD, \
         not UPDATE the stale source"
    );
    assert!(did_zip, "address-mismatch path must run the zip-compare");

    // Reverse direction: an entry was newly missing this frame (BLAS
    // evicted). Same expectation — address-set change → BUILD.
    let cached_full = vec![1u64, 2, 3];
    let current_after_evict = vec![1u64, 3];
    let (use_update, _) = decide_use_update(false, 7, 7, &cached_full, &current_after_evict);
    assert!(
        !use_update,
        "BLAS eviction (entry disappearing from address sequence) \
         must force BUILD"
    );
}

// ── build_instance_map (#419) ──────────────────────────────────
//
// The shared `draw_idx → ssbo_idx` mapping is the single source of
// truth the TLAS `instance_custom_index` and SSBO position must
// agree on. Before #419 the TLAS used the raw enumerate index and
// the SSBO used `gpu_instances.len()` (compacted) — identical only
// when the filter in `draw.rs` never rejected a draw_cmd. A single
// mesh eviction shifted every subsequent SSBO entry by one while
// TLAS custom indices stayed put, silently corrupting material /
// transform reads on every RT hit downstream.

/// Effectively-unbounded cap used by the legacy tests that pre-date
/// the `max_kept` parameter. usize::MAX guarantees the cap never
/// bites for any realistic input, preserving the pre-Option-B
/// semantics under test.
const NO_CAP: usize = usize::MAX;

#[test]
fn instance_map_empty_list_produces_empty_map() {
    let map = build_instance_map(0, NO_CAP, |_| true);
    assert!(map.is_empty());
}

#[test]
fn instance_map_all_kept_matches_iota() {
    // Happy path: every draw_cmd survives the filter. compacted
    // index equals the enumerate index, which is exactly the pre-fix
    // behaviour — so the mapping must be a no-op in this case.
    let map = build_instance_map(4, NO_CAP, |_| true);
    assert_eq!(map, vec![Some(0), Some(1), Some(2), Some(3)]);
}

#[test]
fn instance_map_all_dropped_produces_all_none() {
    let map = build_instance_map(3, NO_CAP, |_| false);
    assert_eq!(map, vec![None, None, None]);
}

#[test]
fn instance_map_skips_compact_subsequent_indices() {
    // The failure mode from the audit: draw_cmds = [A, B, C, D, E]
    // where B and D are filtered out. Before #419 the TLAS would
    // encode custom_index = 2 for C but the SSBO compacted to
    // [A, C, E] at positions 0, 1, 2 — so the shader's ray hit on
    // C would read gpu_instances[2] = E. After #419 C's
    // custom_index is the compacted 1, which matches gpu_instances[1].
    let map = build_instance_map(5, NO_CAP, |i| i != 1 && i != 3);
    assert_eq!(map, vec![Some(0), None, Some(1), None, Some(2)]);
}

#[test]
fn instance_map_only_first_kept() {
    let map = build_instance_map(4, NO_CAP, |i| i == 0);
    assert_eq!(map, vec![Some(0), None, None, None]);
}

#[test]
fn instance_map_next_idx_never_overlaps_a_dropped_slot() {
    // Every `Some(x)` value must be unique and strictly increasing.
    // A regression that decremented or double-assigned `next` would
    // pass the "count matches" check but break SSBO indexing.
    let map = build_instance_map(10, NO_CAP, |i| i % 2 == 0);
    let kept: Vec<u32> = map.iter().filter_map(|x| *x).collect();
    assert_eq!(kept, vec![0, 1, 2, 3, 4]);
    assert!(
        kept.windows(2).all(|w| w[0] < w[1]),
        "compacted indices must be strictly increasing"
    );
}

/// Regression: Option B (`MAX_INSTANCES` cap in lockstep with the
/// SSBO upload). When the kept count would exceed `max_kept`, the
/// trailing entries flip to `None` so the TLAS doesn't emit
/// instances whose `instance_custom_index` would point past the
/// uploaded SSBO range — that would produce garbage reads on every
/// shadow / reflection / GI ray hit against an over-cap instance.
#[test]
fn instance_map_caps_at_max_kept() {
    // 10 draw commands all eligible; cap at 4.
    let map = build_instance_map(10, 4, |_| true);
    // First 4 land at compacted positions 0..3; the trailing 6
    // get None because they would have indices >= 4.
    assert_eq!(
        map,
        vec![
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            None,
            None,
            None,
            None,
            None,
            None,
        ]
    );
}

/// Sibling cap-check: when the filter drops some entries AND the
/// cap bites, the cap counts only the kept (compacted) ones. A
/// dropped entry doesn't consume a cap slot.
#[test]
fn instance_map_cap_counts_kept_only_not_filtered() {
    // 8 draw commands; filter drops every odd index (4 dropped, 4 kept);
    // cap at 3 → keeps the first 3 of the surviving 4.
    let map = build_instance_map(8, 3, |i| i % 2 == 0);
    // Surviving indices in order: 0, 2, 4, 6 → first 3 (0, 2, 4)
    // get compacted 0, 1, 2; index 6 flips to None because the
    // cap is full.
    assert_eq!(
        map,
        vec![Some(0), None, Some(1), None, Some(2), None, None, None,]
    );
}

/// Cap-equal-to-len edge case: when `max_kept >= len` AND every
/// entry is kept, the map is identical to the uncapped iota — the
/// cap doesn't introduce any None entries.
#[test]
fn instance_map_cap_at_or_above_len_is_no_op() {
    assert_eq!(
        build_instance_map(3, 3, |_| true),
        vec![Some(0), Some(1), Some(2)]
    );
    assert_eq!(
        build_instance_map(3, 100, |_| true),
        vec![Some(0), Some(1), Some(2)]
    );
}

/// Regression: #60 + #424 SIBLING. Scratch pool growth policy is a
/// pure `Option<size> + required -> bool` decision shared by both
/// BLAS paths and the TLAS full-rebuild path. Must:
///   - grow on first use (no buffer yet)
///   - grow when the required size exceeds current capacity
///   - reuse when the existing buffer meets or exceeds the need
///     (including equality — the edge where pre-#424 TLAS code
///     would still destroy+recreate)
#[test]
fn scratch_pool_growth_policy() {
    // First use — no existing buffer.
    assert!(scratch_needs_growth(None, 1024));

    // Existing buffer too small — grow.
    assert!(scratch_needs_growth(Some(1024), 2048));

    // Existing buffer exactly the required size — REUSE.
    assert!(!scratch_needs_growth(Some(2048), 2048));

    // Existing buffer larger than required — REUSE (high-water mark).
    assert!(!scratch_needs_growth(Some(1 << 20), 1024));

    // Zero required (empty TLAS) — REUSE whatever's there.
    assert!(!scratch_needs_growth(Some(1), 0));
}

// ── scratch_should_shrink (#495) ─────────────────────────────────
//
// Shrink policy: current > 2× peak AND excess > 16 MB slack. Four
// boundary cases pinned here so a future rewrite can't relax the
// thresholds silently.
const MB: vk::DeviceSize = 1024 * 1024;

#[test]
fn scratch_shrink_triggers_when_excess_is_large() {
    // Current = 100 MB, peak = 2 MB. Ratio = 50×, excess = 98 MB.
    // Both thresholds exceeded → shrink.
    assert!(scratch_should_shrink(100 * MB, 2 * MB));
}

#[test]
fn scratch_shrink_skipped_below_2x_ratio() {
    // Current = 40 MB, peak = 30 MB. Ratio = 1.33×. Excess 10 MB.
    // Ratio check fails → don't shrink.
    assert!(!scratch_should_shrink(40 * MB, 30 * MB));
}

#[test]
fn scratch_shrink_skipped_when_excess_under_slack() {
    // Current = 15 MB, peak = 2 MB. Ratio = 7.5×, but excess = 13 MB
    // < 16 MB slack → don't shrink (not worth the realloc churn).
    assert!(!scratch_should_shrink(15 * MB, 2 * MB));
}

#[test]
fn scratch_shrink_triggers_at_zero_peak_with_large_current() {
    // No BLAS survives — peak = 0, current = 80 MB. Ratio check is
    // `current > 0 * 2 = 0` → true; excess = 80 MB > 16 MB → true.
    // Shrink (the caller's method drops the buffer entirely on zero
    // peak).
    assert!(scratch_should_shrink(80 * MB, 0));
}

#[test]
fn scratch_shrink_skipped_at_zero_peak_under_slack() {
    // peak = 0 but current is tiny (8 MB) — excess 8 MB < 16 MB
    // slack → don't churn.
    assert!(!scratch_should_shrink(8 * MB, 0));
}

#[test]
fn scratch_shrink_skipped_on_exactly_2x_ratio() {
    // current = 2× peak exactly — ratio check is strict `>`, so
    // equality does NOT trigger.
    assert!(!scratch_should_shrink(64 * MB, 32 * MB));
}

// ── shared_blas_scratch_peak (#2460 / AS-D1-NEW-01) ──────────────
//
// `blas_scratch_buffer` is ONE allocation shared by the static
// (mesh-keyed) builders and the per-entity skinned builder/refitter.
// The shrink target must therefore be the max over both maps: the
// skinned refit re-queries no sizes and grows nothing, so a peak
// walked over `blas_entries` alone reallocates the buffer below what
// a live NPC's next `mode = UPDATE` writes into it.

#[test]
fn shared_scratch_peak_takes_the_max_across_both_blas_maps() {
    // A live skinned entity out-scratching every static survivor is
    // the reachable failure shape: interior cell whose static peak is
    // ~1 MB, NPCs from the outgoing cell still resident at 40 MB.
    let static_sizes = [MB, 512 * 1024];
    let skinned_sizes = [40 * MB, 3 * MB];
    assert_eq!(
        shared_blas_scratch_peak(static_sizes, skinned_sizes),
        40 * MB,
        "skinned entries must not be ignored — they share the buffer"
    );

    // …and symmetrically, a large static survivor still wins over a
    // small skinned set.
    assert_eq!(shared_blas_scratch_peak([80 * MB], [2 * MB, MB]), 80 * MB,);
}

#[test]
fn shared_scratch_peak_is_zero_only_when_both_maps_are_empty() {
    // The `peak == 0` arm drops the buffer outright, so it must not
    // fire while a skinned BLAS is still resident — pre-#2460 that
    // failed every refit with "blas_scratch_buffer absent" until a
    // first-sight rebuild.
    assert_eq!(shared_blas_scratch_peak([], [7 * MB]), 7 * MB);
    assert_eq!(shared_blas_scratch_peak([7 * MB], []), 7 * MB);
    assert_eq!(shared_blas_scratch_peak([], []), 0);
}

#[test]
fn shrink_decision_uses_the_union_peak_not_the_static_one() {
    // The #2460 scenario end-to-end at the predicate level: 40 MB
    // buffer, 1 MB static survivors, 30 MB skinned survivor. On the
    // static-only peak the hysteresis fires (40 > 2 MB and excess
    // 39 MB > 16 MB slack) and the buffer is reallocated at 1 MB —
    // beneath the skinned entry's build scratch. On the union peak it
    // correctly declines.
    let static_only = shared_blas_scratch_peak([MB], []);
    assert!(scratch_should_shrink(40 * MB, static_only));

    let union = shared_blas_scratch_peak([MB], [30 * MB]);
    assert!(!scratch_should_shrink(40 * MB, union));
}

// ── tlas_scratch_should_shrink (#1226) ────────────────────────────
//
// Pre-#1226 the TLAS scratch shrink path called `scratch_should_shrink`
// with its 16 MB BLAS-scale slack; TLAS scratch lives at tens of KB to
// <1 MB so the entire mechanism was dead code. The new predicate uses
// a 256 KB TLAS-calibrated slack. Tests below pin the threshold math
// against realistic TLAS scratch scales.
const KB: vk::DeviceSize = 1024;

#[test]
fn tlas_scratch_shrink_fires_at_realistic_excess() {
    // Big exterior frame settles into a small interior: 4 MB peak now,
    // 256 KB working. Ratio = 16× and excess = 3.75 MB
    // > 256 KB slack → shrink.
    let exterior_capacity = 4 * MB;
    let interior_steady = 256 * KB;
    assert!(tlas_scratch_should_shrink(
        exterior_capacity,
        interior_steady
    ));
}

#[test]
fn tlas_scratch_shrink_skipped_below_2x_ratio() {
    // Capacity 1.5× the new peak — no churn.
    assert!(!tlas_scratch_should_shrink(900 * KB, 600 * KB));
}

#[test]
fn tlas_scratch_shrink_skipped_when_excess_under_tlas_slack() {
    // Ratio is huge (50×) but excess is only 196 KB — below the
    // 256 KB slack. Don't churn through a free+create for a tiny win.
    assert!(!tlas_scratch_should_shrink(200 * KB, 4 * KB));
}

#[test]
fn tlas_scratch_shrink_fires_at_zero_peak_when_over_slack() {
    // Slot drained between frames — peak == 0; 512 KB current,
    // excess 512 KB > 256 KB slack → shrink.
    assert!(tlas_scratch_should_shrink(512 * KB, 0));
}

#[test]
fn tlas_scratch_shrink_skipped_on_exactly_2x_ratio() {
    // Strict `>` ratio check — equality doesn't trigger.
    assert!(!tlas_scratch_should_shrink(1024 * KB, 512 * KB));
}

/// Regression: pre-#1226 the TLAS scratch shrink path called
/// `scratch_should_shrink` (BLAS-scale slack), which permanently
/// disabled shrink at realistic TLAS scales. Pin both predicates
/// against the same realistic input so the slack-scale mismatch
/// surfaces in the diff if the call site ever drifts back.
#[test]
fn blas_scale_slack_disables_shrink_at_tlas_scale() {
    // 4 MB current, 256 KB peak — exactly the canonical TLAS-scale
    // scenario the new predicate fires on. The BLAS-scale predicate
    // refuses to shrink (excess = 3.75 MB < 16 MB slack).
    let capacity = 4 * MB;
    let peak = 256 * KB;
    assert!(tlas_scratch_should_shrink(capacity, peak));
    assert!(!scratch_should_shrink(capacity, peak));
}

/// #659 — `is_scratch_aligned` enforces the AS-spec
/// `minAccelerationStructureScratchOffsetAlignment` requirement at
/// every `cmd_build_acceleration_structures` call site. The pure
/// helper keeps the math testable without a Vulkan device; the
/// debug_assert wrapper inside `AccelerationManager` adds the live
/// firing path.
#[test]
fn scratch_alignment_check_matches_modulo() {
    // Trivial-align fast paths.
    assert!(is_scratch_aligned(0, 0));
    assert!(is_scratch_aligned(0xDEAD_BEEF, 0));
    assert!(is_scratch_aligned(0xDEAD_BEEF, 1));

    // 256-byte alignment (typical desktop driver).
    assert!(is_scratch_aligned(0x0000_1000, 256));
    assert!(is_scratch_aligned(0x0000_1100, 256));
    assert!(!is_scratch_aligned(0x0000_1001, 256));
    assert!(!is_scratch_aligned(0x0000_10FF, 256));

    // 128-byte alignment.
    assert!(is_scratch_aligned(0x0000_0080, 128));
    assert!(!is_scratch_aligned(0x0000_0081, 128));

    // 1024 — hypothetical mobile GPU with a stricter requirement.
    assert!(is_scratch_aligned(0x0010_0000, 1024));
    assert!(!is_scratch_aligned(0x0010_0001, 1024));
}

/// #1386 — `align_scratch_address` rounds a raw scratch device address
/// up to the alignment so the value handed to
/// `cmd_build_acceleration_structures` is always a multiple of
/// `minAccelerationStructureScratchOffsetAlignment`, even in release
/// builds where the old `debug_assert!` guard compiled out. The
/// rounded result must (a) be aligned, (b) never move below `raw`, and
/// (c) move by strictly less than `align` — so `scratch_alignment_padding`
/// headroom always covers it.
#[test]
fn align_scratch_address_rounds_up_to_alignment() {
    // Trivial-align no-op paths return the address untouched.
    assert_eq!(align_scratch_address(0xDEAD_BEEF, 0), 0xDEAD_BEEF);
    assert_eq!(align_scratch_address(0xDEAD_BEEF, 1), 0xDEAD_BEEF);

    // Already-aligned addresses are unchanged (the common case on every
    // desktop driver — gpu-allocator returns >= 256 B-aligned GpuOnly).
    assert_eq!(align_scratch_address(0x0000_1000, 256), 0x0000_1000);
    assert_eq!(align_scratch_address(0x0000_1100, 128), 0x0000_1100);

    // Misaligned addresses round UP to the next multiple, never down.
    assert_eq!(align_scratch_address(0x0000_1001, 256), 0x0000_1100);
    assert_eq!(align_scratch_address(0x0000_10FF, 256), 0x0000_1100);
    assert_eq!(align_scratch_address(0x0000_0081, 128), 0x0000_0100);
    assert_eq!(align_scratch_address(0x0010_0001, 1024), 0x0010_0400);

    // Invariants over a sweep of (raw, align) pairs: the rounded value
    // is aligned, >= raw, and within `align - 1` of raw (so the padding
    // headroom always covers the shift).
    for &align in &[128u32, 256, 512, 1024] {
        for raw in (0x4000u64..0x4000 + 4 * align as u64).step_by(7) {
            let aligned = align_scratch_address(raw, align);
            assert!(
                is_scratch_aligned(aligned, align),
                "not aligned: {raw:#x} align {align}"
            );
            assert!(aligned >= raw);
            assert!(aligned - raw <= scratch_alignment_padding(align));
        }
    }
}

/// #1386 — `scratch_alignment_padding` is exactly `align - 1`: the
/// worst-case round-up distance, so a scratch buffer padded by this
/// amount can always satisfy `align_scratch_address` without the build
/// overrunning the allocation. `align <= 1` needs no padding.
#[test]
fn scratch_alignment_padding_is_align_minus_one() {
    assert_eq!(scratch_alignment_padding(0), 0);
    assert_eq!(scratch_alignment_padding(1), 0);
    assert_eq!(scratch_alignment_padding(128), 127);
    assert_eq!(scratch_alignment_padding(256), 255);
    assert_eq!(scratch_alignment_padding(1024), 1023);
}

/// #926 / REN-D8-NEW-11 — `column_major_to_vk_transform` converts
/// glam's column-major `[f32; 16]` storage into the row-major
/// 3×4 layout Vulkan expects. Pre-#926 this conversion was
/// inline-spelt at the TLAS rebuild site with no unit test —
/// any silent re-transpose would corrupt every BLAS instance
/// orientation. Pin the layout against a hand-built rotation +
/// translation matrix.
#[test]
fn column_major_to_vk_transform_pins_row_major_3x4_output() {
    // Affine: 90° rotation about +Y followed by translation (3, 4, 5).
    // Row-major view:
    //   [  0  0  1  3 ]
    //   [  0  1  0  4 ]
    //   [ -1  0  0  5 ]
    //   [  0  0  0  1 ]  (dropped — Vulkan TLAS instance struct
    //                    has no bottom row)
    // glam stores this column-major as 16 floats in column order.
    let column_major: [f32; 16] = [
        0.0, 0.0, -1.0, 0.0, // column 0
        0.0, 1.0, 0.0, 0.0, // column 1
        1.0, 0.0, 0.0, 0.0, // column 2
        3.0, 4.0, 5.0, 1.0, // column 3 (translation)
    ];
    let t = column_major_to_vk_transform(&column_major);
    // Row 0: x-row = (m00, m01, m02, m03).
    assert_eq!(t.matrix[0..4], [0.0, 0.0, 1.0, 3.0]);
    // Row 1: y-row = (m10, m11, m12, m13).
    assert_eq!(t.matrix[4..8], [0.0, 1.0, 0.0, 4.0]);
    // Row 2: z-row = (m20, m21, m22, m23).
    assert_eq!(t.matrix[8..12], [-1.0, 0.0, 0.0, 5.0]);
}

/// #1487 / REN2-02 — `tlas_instance_transform` must emit IDENTITY for
/// skinned draws (`bone_offset != 0`) and the entity's `model_matrix`
/// for rigid draws. Skinned BLAS geometry already bakes the world
/// placement through the bone palette, so re-applying `model_matrix`
/// at the TLAS instance double-transforms the actor's RT presence
/// (shadow caster / reflection / GI subject), placing it at `R·w + t`
/// instead of `w`. Pre-fix every placed actor (since M29 Phase 2) cast
/// no shadow at its visual location and a phantom occluder sat
/// elsewhere.
#[test]
fn skinned_tlas_instance_uses_identity_transform() {
    // A non-trivial placement: 90° about +Y then translate (3, 4, 5),
    // the same affine the row-major pin above exercises. If the skinned
    // path leaked `model_matrix` through, the asserted identity below
    // would pick up this rotation + translation.
    let placed_model: [f32; 16] = [
        0.0, 0.0, -1.0, 0.0, // column 0
        0.0, 1.0, 0.0, 0.0, // column 1
        1.0, 0.0, 0.0, 0.0, // column 2
        3.0, 4.0, 5.0, 1.0, // column 3 (translation)
    ];

    // Skinned: bone_offset != 0 → identity, regardless of model_matrix.
    let mut skinned = make_draw_command(true, false);
    skinned.bone_offset = 128; // any non-zero palette base
    skinned.model_matrix = placed_model;
    let t = tlas_instance_transform(&skinned);
    assert_eq!(
        t.matrix,
        [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
        ],
        "skinned TLAS instance must be identity — its BLAS is already \
         absolute-world; model_matrix here would double-transform it"
    );

    // Rigid: bone_offset == 0 → the model_matrix passes through exactly
    // as `column_major_to_vk_transform` would convert it.
    let mut rigid = make_draw_command(true, false);
    rigid.bone_offset = 0;
    rigid.model_matrix = placed_model;
    assert_eq!(
        tlas_instance_transform(&rigid).matrix,
        column_major_to_vk_transform(&placed_model).matrix,
        "rigid TLAS instance must carry the entity's absolute model_matrix"
    );
}

// ── #1123 / REN-D8-NEW-02 — built_primitive_count invariant ────
//
// The TLAS UPDATE path at `tlas.rs:753` runtime-asserts
// `built_primitive_count == instance_count`. That assert is fed
// by a bookkeeping chain inside `build_tlas` (decide_use_update
// short-circuits + the `instance_count > built_primitive_count`
// guard + the BUILD-mode `built_primitive_count = instance_count`
// store). Pin the chain from outside the live Vulkan path by
// replaying the same sequence on a `TlasBookkeeping` stand-in and
// asserting the invariant after every "submit".
//
// Paired with REN-D8-NEW-01 (#1121) — the runtime assert covers
// the failure at the firing site; this test pins the contract so
// a refactor that breaks it before the assert ever runs fails in
// `cargo test`.

/// Minimal stand-in for the slice of `TlasState` the BUILD/UPDATE
/// decision touches each frame. Captures the same fields production
/// code carries on the per-FIF `TlasState`. Initialised to match a
/// freshly-allocated TLAS (`needs_full_rebuild = true`,
/// `last_blas_map_gen = u64::MAX`, `built_primitive_count = 0`).
struct TlasBookkeeping {
    needs_full_rebuild: bool,
    last_blas_map_gen: u64,
    last_blas_addresses: Vec<vk::DeviceAddress>,
    built_primitive_count: u32,
    /// Number of BUILDs / UPDATEs we've ever submitted from this
    /// stand-in — used by the tests below to assert the right mode
    /// fired for each frame.
    builds: u32,
    updates: u32,
}

impl TlasBookkeeping {
    fn new() -> Self {
        Self {
            needs_full_rebuild: true,
            last_blas_map_gen: u64::MAX,
            last_blas_addresses: Vec::new(),
            built_primitive_count: 0,
            builds: 0,
            updates: 0,
        }
    }

    /// Replay one frame of `build_tlas`'s bookkeeping. Mirrors the
    /// production sequence in `tlas.rs::build_tlas`:
    ///
    /// 1. Call `decide_use_update(needs_full_rebuild, last_gen,
    ///    map_gen, cached, current)`.
    /// 2. Apply the `instance_count > built_primitive_count` guard
    ///    that forces BUILD when an UPDATE would exceed the source
    ///    BUILD's primitive count (VUID-…-pInfos-03708).
    /// 3. Swap `last_blas_addresses` and `current_addresses`.
    /// 4. On BUILD, set `built_primitive_count = instance_count`.
    ///    On UPDATE, leave it untouched.
    /// 5. Clear `needs_full_rebuild` and remember `map_gen`.
    fn submit_frame(&mut self, map_gen: u64, mut current_addresses: Vec<u64>) {
        let instance_count = current_addresses.len() as u32;
        let (mut use_update, _did_zip) = decide_use_update(
            self.needs_full_rebuild,
            self.last_blas_map_gen,
            map_gen,
            &self.last_blas_addresses,
            &current_addresses,
        );
        if use_update && instance_count > self.built_primitive_count {
            use_update = false;
        }
        std::mem::swap(&mut self.last_blas_addresses, &mut current_addresses);
        if use_update {
            self.updates += 1;
        } else {
            self.builds += 1;
            self.built_primitive_count = instance_count;
        }
        self.needs_full_rebuild = false;
        self.last_blas_map_gen = map_gen;
    }

    /// The invariant pinned by [`tlas.rs:753`]'s `debug_assert_eq!`.
    /// Holds at every frame boundary so the next-frame UPDATE path
    /// finds a consistent count and address-list pair.
    fn assert_invariant(&self) {
        assert_eq!(
            self.built_primitive_count as usize,
            self.last_blas_addresses.len(),
            "built_primitive_count ({}) must equal last_blas_addresses.len() ({}) — \
             see #1121 / REN-D8-NEW-01 runtime assert at tlas.rs:753",
            self.built_primitive_count,
            self.last_blas_addresses.len(),
        );
    }
}

/// The headline scenario from the issue: BUILD → UPDATE → shrink →
/// UPDATE. Every transition must preserve the invariant, and the
/// "shrink" frame (instance_count drops below `built_primitive_count`)
/// must force a BUILD because the address-set length changed — without
/// which the next UPDATE submit would feed stale tail data into the
/// BVH on the difference range.
#[test]
fn tlas_built_primitive_count_invariant_holds_across_build_update_cycles() {
    let mut state = TlasBookkeeping::new();
    state.assert_invariant();

    // Frame 0: BUILD (`needs_full_rebuild = true`). 3 instances.
    state.submit_frame(7, vec![1, 2, 3]);
    state.assert_invariant();
    assert_eq!(state.builds, 1);
    assert_eq!(state.updates, 0);
    assert_eq!(state.built_primitive_count, 3);

    // Frame 1: identical address-set, same map_gen → UPDATE. 3 instances.
    state.submit_frame(7, vec![1, 2, 3]);
    state.assert_invariant();
    assert_eq!(state.builds, 1);
    assert_eq!(state.updates, 1);
    assert_eq!(state.built_primitive_count, 3);

    // Frame 2: shrink to 2 instances. `cached.len() != current.len()`
    // so `decide_use_update` forces BUILD. Without this transition's
    // BUILD, the next UPDATE would read past the device buffer end.
    state.submit_frame(7, vec![1, 2]);
    state.assert_invariant();
    assert_eq!(state.builds, 2);
    assert_eq!(state.updates, 1);
    assert_eq!(state.built_primitive_count, 2);

    // Frame 3: same 2 instances, same map_gen → UPDATE. Now
    // `last_blas_addresses.len() == built_primitive_count == 2`
    // (post-shrink invariant); the UPDATE submits exactly 2 instances.
    state.submit_frame(7, vec![1, 2]);
    state.assert_invariant();
    assert_eq!(state.builds, 2);
    assert_eq!(state.updates, 2);
    assert_eq!(state.built_primitive_count, 2);
}

/// Grow case: instance_count grows beyond `built_primitive_count`
/// while the cached address sequence is shorter. `decide_use_update`
/// already forces BUILD on the length mismatch, but the
/// `instance_count > built_primitive_count` guard at `tlas.rs:547`
/// is the second line of defence. Pin both work together.
#[test]
fn tlas_invariant_holds_when_instance_count_grows() {
    let mut state = TlasBookkeeping::new();
    state.submit_frame(7, vec![1, 2]);
    state.assert_invariant();
    assert_eq!(state.built_primitive_count, 2);

    // Grow from 2 → 4 instances. cached.len() != current.len() →
    // decide_use_update forces BUILD. Invariant after BUILD:
    // built_primitive_count == 4 == last_blas_addresses.len().
    state.submit_frame(7, vec![1, 2, 3, 4]);
    state.assert_invariant();
    assert_eq!(state.built_primitive_count, 4);
    assert_eq!(state.builds, 2);
    assert_eq!(state.updates, 0);
}

/// Map-gen mutation (cell load / unload / BLAS eviction frame)
/// short-circuits `decide_use_update` to BUILD even when the address
/// sequence is identical. Invariant must still hold after the
/// dirty-flag-driven BUILD.
#[test]
fn tlas_invariant_holds_across_blas_map_generation_bumps() {
    let mut state = TlasBookkeeping::new();
    state.submit_frame(7, vec![1, 2, 3]);
    state.submit_frame(7, vec![1, 2, 3]); // UPDATE
    state.assert_invariant();
    assert_eq!(state.updates, 1);

    // Cell load bumped the BLAS map generation. Even though the
    // address sequence happens to be unchanged this frame, the
    // short-circuit in `decide_use_update` forces BUILD because
    // addresses might have shifted.
    state.submit_frame(8, vec![1, 2, 3]);
    state.assert_invariant();
    assert_eq!(state.builds, 2, "map_gen bump must force BUILD");
}

/// Empty → non-empty → empty round trip. Empty frames force BUILD
/// via `decide_use_update`'s empty-current short-circuit. The
/// invariant must survive `built_primitive_count = 0` on the empty
/// BUILD and pick up the non-empty count on the next BUILD without
/// any UPDATE accidentally reading stale `built_primitive_count`.
#[test]
fn tlas_invariant_holds_across_empty_frames() {
    let mut state = TlasBookkeeping::new();

    // Empty first frame — BUILD with primitive_count = 0.
    state.submit_frame(7, vec![]);
    state.assert_invariant();
    assert_eq!(state.built_primitive_count, 0);
    assert_eq!(state.builds, 1);

    // Non-empty next frame — length mismatch from cached (0 → 3)
    // forces BUILD. Invariant: built_primitive_count == 3 == len.
    state.submit_frame(7, vec![1, 2, 3]);
    state.assert_invariant();
    assert_eq!(state.built_primitive_count, 3);
    assert_eq!(state.builds, 2);

    // Empty again — short-circuit forces BUILD with count = 0.
    state.submit_frame(7, vec![]);
    state.assert_invariant();
    assert_eq!(state.built_primitive_count, 0);
    assert_eq!(state.builds, 3);
}

/// Identity round-trip: the column-major identity matrix must
/// emit the row-major identity 3×4 (with zero translation).
/// Catches an accidental sign flip / index swap in the helper.
#[test]
fn column_major_to_vk_transform_identity_maps_to_3x4_identity() {
    let identity: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, // col 0
        0.0, 1.0, 0.0, 0.0, // col 1
        0.0, 0.0, 1.0, 0.0, // col 2
        0.0, 0.0, 0.0, 1.0, // col 3
    ];
    let t = column_major_to_vk_transform(&identity);
    assert_eq!(
        t.matrix,
        [
            1.0, 0.0, 0.0, 0.0, // row 0
            0.0, 1.0, 0.0, 0.0, // row 1
            0.0, 0.0, 1.0, 0.0, // row 2
        ]
    );
}

// ── #1140 / CONC-D5-NEW-01 — scratch-serialize barrier invariant ─────
//
// These tests pin `requires_scratch_serialize_barrier_before` against
// the four `ScratchUser` variants. Production sites unconditionally
// self-emit the barrier; the predicate exists to document the rule
// and pin the cross-submission case so a future refactor that drops
// the self-emit "because validation layers don't flag it" is caught
// at `cargo test` time. See `AUDIT_CONCURRENCY_2026-05-16.md` Dim 5.

#[test]
fn scratch_barrier_unneeded_for_first_op_of_frame() {
    assert!(
        !requires_scratch_serialize_barrier_before(ScratchUser::None),
        "first AS build/refit of the frame has no prior writer — \
         no AS_WRITE → AS_WRITE barrier should be required"
    );
}

#[test]
fn scratch_barrier_required_after_same_submission_build() {
    assert!(
        requires_scratch_serialize_barrier_before(ScratchUser::SameSubmissionBuild),
        "BUILD-batch → refit / next-BUILD on the same cmd must \
         serialise on the shared scratch"
    );
}

#[test]
fn scratch_barrier_required_between_refits() {
    assert!(
        requires_scratch_serialize_barrier_before(ScratchUser::SameSubmissionRefit),
        "refit → refit on the same cmd must serialise on the shared \
         scratch (per-iteration emit in the draw_frame refit loop)"
    );
}

/// **Load-bearing case for #983 / REN-D8-NEW-15 _and_ #1300 / D12B-1.**
/// Vulkan host fence-wait after `submit_one_time` establishes a
/// *host*-side dependency only; the next submission's commands still need
/// a device-side AS_WRITE → AS_WRITE barrier when they reuse the shared
/// scratch. Validation layers reason per-submission and do NOT flag this
/// case, so the only safety net is the callee-side self-emit. Two sites
/// rely on this: `refit_skinned_blas` (#983) and the FIRST (`i == 0`)
/// build in `build_skinned_blas_batched_on_cmd` (#1300 — previously the
/// build path only self-emitted between its own builds via `i > 0`,
/// leaving the cross-submission i==0 case unguarded). If a future
/// refactor drops either self-emit ("optimization noticed via
/// emit-count", assuming same-submission semantics), this case silently
/// regresses on cell-load-then-render frames.
///
/// The predicate result here is the contract that pins the rule.
#[test]
fn scratch_barrier_required_across_submission_despite_fence_wait() {
    assert!(
        requires_scratch_serialize_barrier_before(ScratchUser::CrossSubmissionBuildWithFenceWait),
        "Host fence-wait establishes host-side dependency only — \
         device-side AS_WRITE → AS_WRITE barrier is still required \
         when the next submission reuses the shared scratch buffer \
         (see #983 / REN-D8-NEW-15 + #1140 / CONC-D5-NEW-01)"
    );
}

// ── #1790 / SAFE-2026-07-02-01 — scratch-serialize barrier must carry
// AS_READ, not just AS_WRITE, on its dst mask ─────────────────────────
//
// `requires_scratch_serialize_barrier_before` above pins WHETHER a
// barrier is required; it says nothing about which access bits the
// real `record_scratch_serialize_barrier` emits. `refit_skinned_blas`
// records an UPDATE build (`src == dst == entry.accel`), which per spec
// READS `srcAccelerationStructure`. On a first-sight frame the same
// command buffer records a fresh BUILD (WRITE) immediately before the
// refit loop, with only this barrier between them — a dst mask of
// AS_WRITE alone never makes that BUILD's write visible to the refit's
// READ, a same-command-buffer RAW hazard confirmed by the validation
// layer on real hardware (10 occurrences / first-sight skinned NPC on
// an FNV interior-cell run before this fix).
//
// A live call-through test needs a real `ash::Device` + recording
// command buffer (no safe mock exists for `vkCmdPipelineBarrier2`), so
// — mirroring the `draw_frame` early-return guard tests in
// `context/draw.rs` — a static source assertion pins the actual emitted
// mask instead.
#[test]
fn scratch_serialize_barrier_dst_mask_includes_as_read() {
    let src = include_str!("blas_skinned.rs");

    let fn_start = src
        .find("pub fn record_scratch_serialize_barrier(")
        .expect("record_scratch_serialize_barrier must exist");
    // Slice to just this function's body (next `pub fn` at the same
    // indent level, or EOF) so the assertion can't accidentally match
    // an unrelated barrier call elsewhere in the file.
    let fn_body_start = src[fn_start..]
        .find('{')
        .map(|i| fn_start + i)
        .expect("function must have a body");
    let fn_end = src[fn_body_start..]
        .find("\n    }")
        .map(|i| fn_body_start + i)
        .expect("function body must close");
    let fn_body = &src[fn_body_start..fn_end];

    assert!(
        fn_body.contains("vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR"),
        "record_scratch_serialize_barrier must still carry AS_WRITE (the \
         original scratch-WAW requirement, #642 / #1140)"
    );
    assert!(
        fn_body.contains("vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR"),
        "record_scratch_serialize_barrier's dst access mask must ALSO carry \
         AS_READ — a same-cmd first-sight BUILD → UPDATE-refit sequence \
         needs the BUILD's write made visible to the refit's \
         srcAccelerationStructure read, or it's a RAW hazard \
         (#1790 / SAFE-2026-07-02-01)"
    );
}

// ── #1144 / SAFE-D1-NEW-02 — BUILD flag composition pins ─────────────
//
// `UPDATABLE_AS_FLAGS` and `SKINNED_BLAS_FLAGS` are bit-set composites
// assembled via `from_raw(... .as_raw() | ...)`. Without a pinned
// composition test, a typo on a future edit — e.g.
// `PREFER_FAST_BUILD` → `PREFER_FAST_TRACE` on the skinned arm, or
// accidentally adding `ALLOW_COMPACTION` — would compile and run
// silently. Failure modes:
//
//   * `PREFER_FAST_TRACE` regression on the skinned arm: silent ~18%
//     FPS regression on FNV Prospector (R6a-prospector-regress at
//     1775a7e6 already lived through this once).
//   * `ALLOW_COMPACTION` on an updatable BLAS:
//     VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667 violation
//     at the next UPDATE call — validation layer catches it in debug;
//     release builds silently mis-render the skinned mesh.

#[test]
fn updatable_as_flags_is_fast_trace_plus_allow_update() {
    use ash::vk::BuildAccelerationStructureFlagsKHR as F;
    assert_eq!(
        super::constants::UPDATABLE_AS_FLAGS,
        F::PREFER_FAST_TRACE | F::ALLOW_UPDATE,
        "UPDATABLE_AS_FLAGS must be exactly PREFER_FAST_TRACE | ALLOW_UPDATE — \
         no ALLOW_COMPACTION (VUID-03667 would fire on UPDATE), no PREFER_FAST_BUILD \
         (TLAS budget is FAST_TRACE per #958)."
    );
}

#[test]
fn skinned_blas_flags_is_fast_build_plus_allow_update() {
    use ash::vk::BuildAccelerationStructureFlagsKHR as F;
    assert_eq!(
        super::constants::SKINNED_BLAS_FLAGS,
        F::PREFER_FAST_BUILD | F::ALLOW_UPDATE,
        "SKINNED_BLAS_FLAGS must be exactly PREFER_FAST_BUILD | ALLOW_UPDATE — \
         FAST_BUILD beats FAST_TRACE empirically on the skinned-BLAS path (see \
         R6a-prospector-regress 2026-05-16 at 1775a7e6 — flipping to FAST_TRACE \
         cost ~18% FPS on FNV Prospector). No ALLOW_COMPACTION (VUID-03667)."
    );
}

#[test]
fn static_blas_flags_is_fast_trace_plus_allow_compaction() {
    use ash::vk::BuildAccelerationStructureFlagsKHR as F;
    assert_eq!(
        super::constants::STATIC_BLAS_FLAGS,
        F::PREFER_FAST_TRACE | F::ALLOW_COMPACTION,
        "STATIC_BLAS_FLAGS must be exactly PREFER_FAST_TRACE | ALLOW_COMPACTION — \
         FAST_TRACE because static geometry is traced far more than rebuilt; \
         ALLOW_COMPACTION kept in lockstep across all static-BLAS sites so the \
         compact pass lights up without a flag-drift bisect. No ALLOW_UPDATE — \
         static BLAS is rebuilt, never refit."
    );
}

/// #1913 — pin the shadow-mask bucket assignment fed into the TLAS
/// instance's 8-bit `Packed24_8` mask. Every geometry family must land in its
/// explicit visibility layer, and the complete mask must remain 8-bit (the const-assert in
/// `shader_constants_data.rs` enforces the ceiling at build time; this pins
/// the runtime selection + invariant so a value edit or an inverted branch
/// is caught by `cargo test`, not by a silent RT dropout in-engine).
#[test]
fn shadow_mask_bucket_selection_is_pinned() {
    use crate::shader_constants::{
        VISIBILITY_LAYER_ARCHITECTURE, VISIBILITY_LAYER_DYNAMIC_ACTOR, VISIBILITY_LAYER_EFFECT,
        VISIBILITY_LAYER_FOLIAGE, VISIBILITY_LAYER_GLASS, VISIBILITY_LAYER_STATIC_PROP,
        VISIBILITY_MASK_FULL,
    };
    use crate::vulkan::scene_buffer::{
        MATERIAL_KIND_EFFECT_SHADER, MATERIAL_KIND_FIRE_REFRACTION, MATERIAL_KIND_GLASS,
        MATERIAL_KIND_NO_LIGHTING,
    };
    use byroredux_core::ecs::components::RenderLayer;

    // Glass → glass bucket regardless of layer.
    assert_eq!(
        shadow_mask_for_instance(MATERIAL_KIND_GLASS, RenderLayer::Architecture, true, 0.0),
        VISIBILITY_LAYER_GLASS as u8,
        "glass material must select the glass shadow bucket",
    );

    // #2238 — a real two-layer-refractive MultiLayerParallax surface (kind
    // 11, non-zero refraction scale) is a caustic source per the CPU gate
    // (`draw::is_refractive_glass`) and must land in the same glass bucket,
    // not opaque — else it self-shadows its own caustic.
    const MATERIAL_KIND_MULTI_LAYER_PARALLAX: u32 = 11;
    assert_eq!(
        shadow_mask_for_instance(
            MATERIAL_KIND_MULTI_LAYER_PARALLAX,
            RenderLayer::Architecture,
            false,
            0.3,
        ),
        VISIBILITY_LAYER_GLASS as u8,
        "refractive MultiLayerParallax must select the glass shadow bucket",
    );

    // A MultiLayerParallax draw with a zero (unauthored) refraction scale is
    // not a real refractor and stays in the ordinary opaque bucket, matching
    // `is_refractive_glass`'s rejection of the same case.
    assert_eq!(
        shadow_mask_for_instance(
            MATERIAL_KIND_MULTI_LAYER_PARALLAX,
            RenderLayer::Architecture,
            false,
            0.0,
        ),
        VISIBILITY_LAYER_ARCHITECTURE as u8,
    );

    // Solid architecture has exactly the architecture category.
    assert_eq!(
        shadow_mask_for_instance(0, RenderLayer::Architecture, false, 0.0),
        VISIBILITY_LAYER_ARCHITECTURE as u8,
    );

    assert_eq!(
        shadow_mask_for_instance(0, RenderLayer::Clutter, false, 0.0),
        VISIBILITY_LAYER_STATIC_PROP as u8,
    );
    assert_eq!(
        shadow_mask_for_instance(0, RenderLayer::Actor, false, 0.0),
        VISIBILITY_LAYER_DYNAMIC_ACTOR as u8,
    );
    assert_eq!(
        shadow_mask_for_instance(0, RenderLayer::Decal, false, 0.0),
        VISIBILITY_LAYER_FOLIAGE as u8,
    );

    // Alpha/effect proxy geometry must not become a structural wall.
    assert_eq!(
        shadow_mask_for_instance(0, RenderLayer::Architecture, true, 0.0),
        VISIBILITY_LAYER_EFFECT as u8,
    );
    for kind in [MATERIAL_KIND_EFFECT_SHADER, MATERIAL_KIND_FIRE_REFRACTION] {
        assert_eq!(
            shadow_mask_for_instance(kind, RenderLayer::Architecture, false, 0.0),
            VISIBILITY_LAYER_EFFECT as u8,
            "effect proxy kind {kind} must select the effect layer",
        );
    }
    assert_eq!(
        shadow_mask_for_instance(
            MATERIAL_KIND_NO_LIGHTING,
            RenderLayer::Architecture,
            false,
            0.0,
        ),
        VISIBILITY_LAYER_ARCHITECTURE as u8,
    );

    const {
        assert!(VISIBILITY_MASK_FULL <= 0xFF);
    }
}

// #2481 / AS-D1-NEW-02 — BLAS registration must release any BLAS already
// occupying the target slot/key before overwriting it, or the previous
// `vk::AccelerationStructureKHR` leaks (no `Drop` impl) and the byte
// budget counters drift upward. Building a real BLAS needs a live Vulkan
// device, so — matching this crate's convention for logic that can only
// be exercised end-to-end with a GPU (e.g. `context/mod.rs`'s
// `rigid_history_hasher_tests`, `context/skinned_blas_refit.rs`'s
// `skin_built_this_frame_skip_tests`) — this pins the fix at the source
// level: the release call must appear, and must appear strictly before
// the registration it guards, at all three sites.
#[cfg(test)]
mod blas_registration_releases_occupied_slot_tests {
    const BLAS_STATIC_RS: &str = include_str!("blas_static.rs");
    const BLAS_SKINNED_RS: &str = include_str!("blas_skinned.rs");

    #[test]
    fn build_blas_releases_before_overwriting() {
        let guard_pos = BLAS_STATIC_RS
            .find("self.drop_blas(mesh_handle);")
            .expect("build_blas must release any occupied handle before overwriting it (#2481)");
        let assign_pos = BLAS_STATIC_RS
            .find("self.blas_entries[handle] = Some(BlasEntry {\n                accel,")
            .expect("build_blas's registration assignment must still exist");
        assert!(
            guard_pos < assign_pos,
            "the release must run BEFORE the overwrite, or the entry being \
             replaced is still live when it's dropped as plain memory"
        );
    }

    #[test]
    fn build_blas_batched_releases_before_overwriting() {
        let guard_pos = BLAS_STATIC_RS
            .find("self.drop_blas(mesh_handle);")
            .expect("a drop_blas guard must exist");
        // Two call sites share the same needle text (`build_blas` and
        // `build_blas_batched`'s Phase 7); confirm a SECOND occurrence
        // exists for the batched path specifically.
        let second_guard_pos = BLAS_STATIC_RS[guard_pos + 1..]
            .find("self.drop_blas(mesh_handle);")
            .map(|p| p + guard_pos + 1)
            .expect(
                "build_blas_batched's Phase 7 registration must ALSO release \
                 any occupied handle before overwriting it (#2481) — the two \
                 static registration sites must both carry the guard",
            );
        let assign_pos = BLAS_STATIC_RS[second_guard_pos..]
            .find("self.blas_entries[handle] = Some(BlasEntry {")
            .map(|p| p + second_guard_pos)
            .expect("build_blas_batched's registration assignment must still exist");
        assert!(second_guard_pos < assign_pos);
    }

    #[test]
    fn skinned_blas_batch_releases_before_overwriting() {
        let guard_pos = BLAS_SKINNED_RS
            .find("self.drop_skinned_blas(p.entity_id);")
            .expect(
                "build_skinned_blas_batched_on_cmd's Phase 4 registration must \
                 release any existing entity entry before overwriting it (#2481)",
            );
        let assign_pos = BLAS_SKINNED_RS
            .find("self.skinned_blas.insert(")
            .expect("the skinned_blas registration insert must still exist");
        assert!(
            guard_pos < assign_pos,
            "the release must run BEFORE the insert, or the entry being \
             replaced is still live when it's dropped as plain memory"
        );
    }
}

// ── TLAS allocate-then-swap + post-build commit ordering ────────────
//
// Both invariants are ordering properties of `tlas.rs` that only show
// up on a fallible path a headless test can't drive (an allocation
// failure at TLAS grow time, a `write_mapped` flush failure), and the
// consequences are validation-layer / GPU-fault visible only. Same
// source-position pinning approach as
// `blas_registration_releases_occupied_slot_tests` above and
// `context/skinned_blas_refit.rs`'s `skin_built_this_frame_skip_tests`.
#[cfg(test)]
mod tlas_commit_ordering_tests {
    const TLAS_RS: &str = include_str!("tlas.rs");

    /// #2673 / CONC-D1-NEW-01 — `ensure_tlas_state` must allocate the
    /// replacement buffers + acceleration structure BEFORE destroying
    /// the slot's existing `TlasState`. Destroy-first meant any `?` in
    /// the allocation window left `self.tlas[frame] == None` while
    /// scene descriptor binding 2 still named the destroyed AS, with
    /// `rt_flag` latched at 1.0 — a use-after-free read by every RT
    /// shading path.
    #[test]
    fn ensure_tlas_state_allocates_before_destroying_the_old_slot() {
        let destroy_pos = TLAS_RS
            .find("if let Some(mut old) = self.tlas[frame_index].take()")
            .expect("ensure_tlas_state must still retire the old slot");
        for (needle, what) in [
            (
                "GpuBuffer::create_host_visible(",
                "the instance staging buffer",
            ),
            (
                "let mut instance_buffer_device = GpuBuffer::create_device_local_uninit(",
                "the device-local instance buffer",
            ),
            (
                "let mut tlas_buffer = GpuBuffer::create_device_local_uninit(",
                "the AS backing buffer",
            ),
            (
                ".create_acceleration_structure(&accel_info, None)",
                "the acceleration structure",
            ),
        ] {
            let alloc_pos = TLAS_RS
                .find(needle)
                .unwrap_or_else(|| panic!("ensure_tlas_state must still allocate {what}"));
            assert!(
                alloc_pos < destroy_pos,
                "{what} must be allocated BEFORE the old TlasState is destroyed \
                 (#2673) — otherwise a failure of that allocation leaves \
                 descriptor binding 2 naming a destroyed VkAccelerationStructureKHR \
                 while `tlas_written` keeps rt_flag latched at 1.0"
            );
        }
    }

    /// #2673 SIBLING — the scratch regrow follows the same discipline:
    /// build into a local, retire the old buffer only past the commit
    /// point. A destroy-first regrow could leave the slot with a live
    /// TLAS and no scratch buffer, which `build_tlas` unwraps.
    #[test]
    fn scratch_regrow_allocates_before_destroying_the_old_buffer() {
        let alloc_pos = TLAS_RS
            .find("let new_scratch = if needs_new_scratch {")
            .expect("the scratch regrow must build into a local first (#2673)");
        let destroy_pos = TLAS_RS
            .find("if let Some(mut old_scratch) = self.scratch_buffers[frame_index].take()")
            .expect("the scratch regrow must still retire the old buffer");
        assert!(
            alloc_pos < destroy_pos,
            "the replacement scratch buffer must be allocated BEFORE the old one \
             is destroyed (#2673)"
        );
    }

    /// #2674 / CONC-D1-NEW-02 — the BUILD-vs-UPDATE bookkeeping that
    /// next frame's `decide_use_update` consults must be committed only
    /// after the build has been recorded. Committing before the
    /// fallible `instance_buffer.write_mapped(..)?` let a failed frame
    /// claim a BUILD that never happened, so the next frame could pick
    /// UPDATE with changed BLAS references (VUID-…-pInfos-03707).
    #[test]
    fn build_tlas_commits_bookkeeping_after_recording_the_build() {
        let record_pos = TLAS_RS
            .find("self.accel_loader.cmd_build_acceleration_structures(")
            .expect("build_tlas must still record the build");
        let write_pos = TLAS_RS
            .find("tlas.instance_buffer.write_mapped(device, &instances)?;")
            .expect("build_tlas must still stage the instances");
        assert!(
            write_pos < record_pos,
            "sanity: the fallible host write precedes the build record"
        );
        for (needle, what) in [
            (
                "std::mem::swap(\n            &mut tlas.last_blas_addresses,",
                "the last_blas_addresses promotion",
            ),
            (
                "tlas.needs_full_rebuild = false;",
                "the needs_full_rebuild clear",
            ),
            (
                "tlas.last_blas_map_gen = map_gen;",
                "the map-generation stamp",
            ),
        ] {
            let commit_pos = TLAS_RS
                .find(needle)
                .unwrap_or_else(|| panic!("build_tlas must still perform {what}"));
            assert!(
                commit_pos > record_pos,
                "{what} must run AFTER cmd_build_acceleration_structures (#2674) — \
                 committing it before the fallible write_mapped lets a failed frame \
                 assert a BUILD that never landed, and the next frame then refits \
                 with stale acceleration_structure_reference values"
            );
        }
    }
}

// ── AS↔SSBO compaction-count contract (#2913 / REN-D1-01) ──────
//
// `build_instance_map` is the documented single source of truth for
// the TLAS `instance_custom_index` ↔ compacted-SSBO-position
// agreement, but only `build_tlas_instances` reads it — the SSBO
// builder in `draw.rs` re-derives the same compaction from
// `gpu_instances.len()`. `draw_frame` now pins the two with a
// `debug_assert_eq!` against `instance_map.iter().flatten().count()`.
// These tests pin the counting rule that assert depends on, so a
// change to the map's representation can't quietly make the guard
// vacuous (e.g. always-0, or counting rejected slots too).

#[test]
fn instance_map_kept_count_equals_accepted_predicate_count() {
    // The property `draw_frame`'s debug_assert relies on: the number of
    // Some entries is exactly the number of draw commands the predicate
    // accepted — which is what the SSBO loop's `gpu_instances.len()`
    // independently arrives at.
    for (total, reject) in [
        (0_usize, vec![]),
        (5, vec![]),
        (5, vec![0_usize]),
        (5, vec![1, 3]),
        (5, vec![0, 1, 2, 3, 4]),
        (9, vec![4]),
    ] {
        let map = build_instance_map(total, NO_CAP, |i| !reject.contains(&i));
        let kept = map.iter().flatten().count();
        assert_eq!(
            kept,
            total - reject.len(),
            "flatten().count() must equal the accepted-predicate count \
             (total={total}, rejected={reject:?})"
        );
        // And the compacted indices must be exactly 0..kept with no gaps —
        // otherwise a matching COUNT could still hide a mismatched
        // ASSIGNMENT, and the debug_assert would pass on corrupt indices.
        let mut seen: Vec<u32> = map.iter().flatten().copied().collect();
        seen.sort_unstable();
        assert_eq!(
            seen,
            (0..kept as u32).collect::<Vec<_>>(),
            "kept entries must compact to a dense 0..N range"
        );
    }
}

#[test]
fn instance_map_kept_count_is_capped_the_same_way_the_ssbo_is() {
    // The cap arm matters to the same contract: if the map stops mapping
    // at `max_kept` but the SSBO loop kept pushing, the counts diverge and
    // every TLAS custom index past the cap addresses the wrong instance.
    let map = build_instance_map(8, 3, |_| true);
    assert_eq!(
        map.iter().flatten().count(),
        3,
        "the map must stop issuing indices at max_kept"
    );
    assert_eq!(
        map,
        vec![Some(0), Some(1), Some(2), None, None, None, None, None],
        "over-cap draw commands must map to None, not to a wrapped index"
    );
}

// ── TLAS shrink must not publish a dangling handle (#2929 / CON-D1-01) ──
//
// `shrink_tlas_to_fit` used to destroy the slot's AS outright and rely on
// the next `build_tlas` to recreate it. Scene descriptor set-1 binding 2
// keeps naming the destroyed handle until a *successful* build re-points
// it, and `draw_frame`'s failure arm can only re-point at an AS the
// manager still owns — after a teardown it owns nothing. Binding 2 is not
// PARTIALLY_BOUND and `triangle.frag` statically uses `topLevelAS`, so the
// geometry pass would run against an invalid statically-used descriptor.
// The shrink fires under VRAM pressure, which is exactly when the
// replacement allocation is likeliest to fail — the two correlate.
//
// The fix routes the shrink through `ensure_tlas_state`'s allocate-then-
// swap (#2673). These are source assertions because the hazard needs a
// live device plus a failing allocation to reproduce.

#[test]
fn tlas_shrink_records_intent_instead_of_destroying_the_live_slot() {
    let src = include_str!("memory.rs");
    let shrink = src
        .split("pub unsafe fn shrink_tlas_to_fit")
        .nth(1)
        .and_then(|rest| rest.split("\n    pub ").next())
        .expect("shrink_tlas_to_fit must still exist");

    assert!(
        shrink.contains("self.tlas_shrink_pending[slot_index] = true;"),
        "shrink_tlas_to_fit must RECORD the shrink for ensure_tlas_state to \
         perform via allocate-then-swap (#2929)"
    );
    assert!(
        !shrink.contains("destroy_acceleration_structure"),
        "shrink_tlas_to_fit must not destroy the slot's acceleration \
         structure itself — that leaves scene binding 2 naming a dead \
         handle until a SUCCESSFUL rebuild, and the rebuild is likeliest to \
         fail under the very VRAM pressure that triggered the shrink (#2929)"
    );
    assert!(
        !shrink.contains("self.tlas[slot_index].take()"),
        "the slot must stay live until its replacement is committed (#2929)"
    );
}

#[test]
fn ensure_tlas_state_consumes_the_shrink_request_past_the_commit_point() {
    let src = include_str!("tlas.rs");

    assert!(
        src.contains("|| self.tlas_shrink_pending[frame_index];"),
        "a pending shrink must force the rebuild path, otherwise the request \
         is recorded and never acted on (#2929)"
    );

    let clear = src
        .find("self.tlas_shrink_pending[frame_index] = false;")
        .expect("ensure_tlas_state must clear the shrink request (#2929)");
    let commit = src
        .find("self.tlas[frame_index] = Some(TlasState {")
        .expect("ensure_tlas_state must still commit the new slot");
    assert!(
        clear > commit,
        "the shrink request must be cleared only AFTER the replacement is \
         committed — clearing it earlier drops the retry when a fallible \
         step fails, stranding the oversized TLAS forever (#2929)"
    );
}

// ── The frame's only AS_WRITE→AS_READ barrier is unconditional (#2931) ──

/// Regression for #2931 (CON-D2-01). The `AS_BUILD → FRAGMENT|COMPUTE`
/// barrier in `draw_frame` used to sit inside the `build_tlas` SUCCESS arm
/// only. It does not merely publish the TLAS build: `record_skinned_blas_refit`
/// runs earlier in the same command buffer and this is the frame's ONLY
/// `ACCELERATION_STRUCTURE_WRITE → ACCELERATION_STRUCTURE_READ` barrier, so
/// it is what makes those refits visible too.
///
/// Clearing `rt_flag` on the failure arm does not cover it: `rt_flag` gates
/// the FRAGMENT consumers, while the volumetrics inject dispatch gates on
/// `accel.tlas_handle(frame)` (`post_passes.rs`), and post-#2673 a failed
/// build deliberately keeps the previous AS alive — so `tlas_handle` is
/// still `Some`, volumetrics still ray-queries from COMPUTE, and the
/// skinned refits reach it unpublished.
#[test]
fn as_build_to_ray_query_barrier_runs_on_both_build_tlas_arms() {
    let src = include_str!("../context/draw.rs");

    let barrier = src
        .find("vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR")
        .expect("draw_frame must still emit the AS_BUILD -> ray-query barrier");
    let failure_arm = src
        .find("log::warn!(\"TLAS build failed: {e}\");")
        .expect("the build_tlas failure arm must still exist");
    let rt_flag_clear = src
        .find("Failed to clear rt_flag after TLAS build failure")
        .expect("the failure arm must still clear rt_flag");

    // The barrier must sit AFTER the whole if/else, not nested in the
    // success arm — i.e. past the failure arm's last statement.
    assert!(
        barrier > failure_arm && barrier > rt_flag_clear,
        "the AS_WRITE -> AS_READ barrier must run on both arms of the \
         build_tlas result; nesting it in the success arm leaves that \
         frame's skinned-BLAS refits unpublished to the volumetrics \
         compute ray query on a failed build (#2931)"
    );

    assert!(
        src.contains("if !tlas_build_failed {"),
        "the post-build descriptor write must stay gated on build success \
         while the barrier itself does not (#2931)"
    );
}

// ── BLAS compaction rollback + peak accounting ───────────────────────
//
// Both invariants live on `build_blas_batched`'s compaction phase, whose
// only trigger is an allocator OOM part-way through a batch — a live
// device plus a genuinely exhausted pool. Same source-position pinning
// approach the file already uses for `blas_registration_releases_
// occupied_slot_tests` and `tlas_commit_ordering_tests`.
#[cfg(test)]
mod blas_compaction_rollback_tests {
    const BLAS_STATIC_RS: &str = include_str!("blas_static.rs");

    /// #2926 / PERF-D3-02 — `alloc_compact`'s two early exits
    /// (`create_device_local_uninit`'s `?` and the
    /// `create_acceleration_structure` `bail!`) must not strand the
    /// compaction destinations earlier iterations already allocated. A
    /// `vk::AccelerationStructureKHR` has no `Drop` impl, so a
    /// closure-owned `compact_accels` leaked one handle per already-
    /// compacted mesh — on the one path (OOM) where leaking makes the
    /// next attempt fail sooner. The vec must therefore be owned by the
    /// caller and walked by the rollback arm.
    #[test]
    fn alloc_compact_failure_destroys_already_compacted_structures() {
        let decl = BLAS_STATIC_RS
            .find(
                "let mut compact_accels: Vec<CompactedBlas> = Vec::with_capacity(prepared.len());",
            )
            .expect(
                "`compact_accels` must be declared OUTSIDE `alloc_compact` so the \
                 rollback arm can see what the closure allocated before it failed (#2926)",
            );
        let closure = BLAS_STATIC_RS
            .find("let mut alloc_compact = |compact_accels: &mut Vec<CompactedBlas>|")
            .expect(
                "`alloc_compact` must take `compact_accels` by `&mut` rather than \
                 owning it (#2926)",
            );
        assert!(
            decl < closure,
            "the caller-owned vec must be declared before the closure that fills it"
        );

        let err_arm = BLAS_STATIC_RS[closure..]
            .find("match alloc_compact(&mut compact_accels)")
            .map(|p| p + closure)
            .expect("the call site must pass the caller-owned vec in");
        // The rollback arm for the compaction-allocation failure runs
        // before the `prepared` rollback that #316 already had.
        let compact_cleanup = BLAS_STATIC_RS[err_arm..]
            .find("for (_, accel, mut buf, _, _, _) in compact_accels {")
            .map(|p| p + err_arm)
            .expect(
                "the `alloc_compact` failure arm must destroy every compaction \
                 destination already allocated — each is a raw \
                 vk::AccelerationStructureKHR with no Drop impl (#2926)",
            );
        let prepared_cleanup = BLAS_STATIC_RS[err_arm..]
            .find("for mut p in prepared {")
            .map(|p| p + err_arm)
            .expect("the #316 `prepared` rollback must still run on this arm");
        assert!(
            compact_cleanup < prepared_cleanup,
            "both rollbacks must run on the compaction-failure arm"
        );
    }

    /// #2927 / PERF-D3-03 — the compaction phase is where static-BLAS
    /// residency peaks (originals + destinations both live until Phase 7),
    /// and the Phase-1 `pending_bytes` ledger never sees it. The budget
    /// must be tested against `total_before + total_after` before the
    /// first destination is allocated — the readback above it has already
    /// made the exact peak knowable.
    #[test]
    fn compaction_phase_checks_the_budget_against_the_real_peak() {
        let totals = BLAS_STATIC_RS
            .find("let total_after: u64 = compacted_sizes.iter().sum();")
            .expect("alloc_compact must still sum the compacted sizes");
        let evict = BLAS_STATIC_RS[totals..]
            .find("self.evict_unused_blas(")
            .map(|p| p + totals)
            .expect(
                "the compaction phase must run a budget check — it is the phase \
                 that pushes static-BLAS residency to its batch maximum, and \
                 pre-#2927 it had no eviction call at all",
            );
        let alloc_loop = BLAS_STATIC_RS[totals..]
            .find("for (i, p) in prepared.iter().enumerate() {")
            .map(|p| p + totals)
            .expect("the destination-allocation loop must still exist");
        assert!(
            evict < alloc_loop,
            "the check must run BEFORE the first compaction destination is \
             allocated, or it is measuring a peak it can no longer avoid (#2927)"
        );
        assert!(
            BLAS_STATIC_RS[evict..alloc_loop].contains("total_before.saturating_add(total_after)"),
            "the pending figure must be originals + destinations — both sets are \
             simultaneously resident until Phase 7 destroys the originals (#2927)"
        );
    }
}
