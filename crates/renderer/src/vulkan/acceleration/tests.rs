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
    let err = validate_refit_counts(100, 300, 80, 300)
        .expect_err("vertex-count drift must be rejected");
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

/// Sibling check: the threshold must be a sane number of frames.
/// At 60 FPS the issue suggested ~10 s = 600 frames — too low
/// would thrash the rebuild path, too high defeats the bug fix.
#[test]
fn skinned_blas_threshold_is_in_sane_range() {
    // 5 s ≤ threshold ≤ 30 s at 60 FPS.
    assert!(SKINNED_BLAS_REFIT_THRESHOLD >= 300);
    assert!(SKINNED_BLAS_REFIT_THRESHOLD <= 1800);
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
    let (use_update, did_zip) =
        decide_use_update(false, 7, 7, &cached_after_skip, &current_full);
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
    let (use_update, _) =
        decide_use_update(false, 7, 7, &cached_full, &current_after_evict);
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

/// #682 / MEM-2-7 — the policy that gates `shrink_tlas_scratch_to_fit`
/// is the same `scratch_should_shrink` that gates the BLAS path,
/// but the TLAS scratch failure mode is distinct: a single big
/// exterior frame (8 K+ instances → MB-scale build scratch) used
/// to pin that capacity for the rest of the session. Pin the
/// canonical scenario here so a future tweak to the threshold
/// surfaces in the diff for both #495 (BLAS) and #682 (TLAS)
/// failure modes.
#[test]
fn tlas_scratch_shrink_fires_after_exterior_peak() {
    // 8 K-instance exterior cell can land scratch at ~32 MB on
    // typical desktop drivers; settling into a small interior
    // typically needs <1 MB. Ratio = 32× and excess = 31 MB
    // > 16 MB SLACK → shrink.
    let exterior_peak = 32 * MB;
    let interior_steady = 1 * MB;
    assert!(scratch_should_shrink(exterior_peak, interior_steady));
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
