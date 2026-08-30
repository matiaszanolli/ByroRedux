//! Acceleration-structure tests — pure decision predicates — TLAS eligibility, BUILD-vs-UPDATE, refit validation, BLAS budget/eviction, flag composition.
//!
//! Split out of the 2 329-LOC monolithic `tests.rs` under #2977. Every
//! test here is a pure unit test (no live Vulkan context); the split
//! mirrors the production submodule names where tests exist for them.

use super::super::predicates::*;
use super::super::*;
use super::make_draw_command;

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
        super::super::constants::UPDATABLE_AS_FLAGS,
        super::super::constants::SKINNED_BLAS_FLAGS,
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
        super::super::constants::UPDATABLE_AS_FLAGS,
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
        super::super::constants::SKINNED_BLAS_FLAGS,
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
        super::super::constants::STATIC_BLAS_FLAGS,
        F::PREFER_FAST_TRACE | F::ALLOW_COMPACTION,
        "STATIC_BLAS_FLAGS must be exactly PREFER_FAST_TRACE | ALLOW_COMPACTION — \
         FAST_TRACE because static geometry is traced far more than rebuilt; \
         ALLOW_COMPACTION kept in lockstep across all static-BLAS sites so the \
         compact pass lights up without a flag-drift bisect. No ALLOW_UPDATE — \
         static BLAS is rebuilt, never refit."
    );
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
    const TLAS_RS: &str = include_str!("../tlas.rs");

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
    let src = include_str!("../memory.rs");
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
    let src = include_str!("../tlas.rs");

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

/// #3043 — select the heap behind BLAS-compatible memory, not the smallest
/// DEVICE_LOCAL heap (which is commonly an AMD BAR aperture).
#[test]
fn blas_budget_derives_from_the_compatible_allocation_heap() {
    use super::super::constants::MIN_BLAS_BUDGET_BYTES;
    use crate::vulkan::device::device_local_heap_bytes_for_memory_type_bits;

    // 12 GB single-heap desktop part → 4 GB, the figure the
    // `blas_budget_bytes` field doc quotes.
    assert_eq!(
        blas_budget_for_heap(12 * 1024 * 1024 * 1024),
        4 * 1024 * 1024 * 1024
    );
    // 6 GB RT-minimum target → 2 GB, likewise.
    assert_eq!(
        blas_budget_for_heap(6 * 1024 * 1024 * 1024),
        2 * 1024 * 1024 * 1024
    );
    // Floor holds for tiny and degenerate (no DEVICE_LOCAL heap → 0) heaps.
    assert_eq!(blas_budget_for_heap(0), MIN_BLAS_BUDGET_BYTES);
    assert_eq!(
        blas_budget_for_heap(64 * 1024 * 1024),
        MIN_BLAS_BUDGET_BYTES
    );

    // Multi-heap AMD-style layout: main VRAM plus a small host-visible BAR.
    let main = 8 * 1024 * 1024 * 1024u64;
    let bar = 256 * 1024 * 1024u64;
    let mut props = vk::PhysicalDeviceMemoryProperties::default();
    props.memory_heap_count = 2;
    props.memory_heaps[0] = vk::MemoryHeap {
        size: main,
        flags: vk::MemoryHeapFlags::DEVICE_LOCAL,
    };
    props.memory_heaps[1] = vk::MemoryHeap {
        size: bar,
        flags: vk::MemoryHeapFlags::DEVICE_LOCAL,
    };
    props.memory_type_count = 2;
    props.memory_types[0] = vk::MemoryType {
        property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
        heap_index: 0,
    };
    props.memory_types[1] = vk::MemoryType {
        property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL
            | vk::MemoryPropertyFlags::HOST_VISIBLE,
        heap_index: 1,
    };

    assert_eq!(
        device_local_heap_bytes_for_memory_type_bits(&props, 0b11),
        Some(main),
        "GpuOnly's first compatible type is backed by main VRAM"
    );
    assert_eq!(
        device_local_heap_bytes_for_memory_type_bits(&props, 0b10),
        Some(bar),
        "requirements that only admit BAR memory must budget the BAR heap"
    );
    assert_eq!(
        device_local_heap_bytes_for_memory_type_bits(&props, 0),
        None
    );
}

// ── #3540 — per-frame static-BLAS recovery bound ──────────────

/// The ordinary case: a handful of meshes came back into view after
/// eviction, and the visible set is nowhere near the budget. Restore
/// all of them in one frame — the cap must be inert here.
#[test]
fn small_recovery_inside_budget_restores_everything() {
    // 1000 resident entries totalling 100 MB → 100 KB mean; 1200 visible
    // draws project to ~117 MB against a 4 GB budget.
    assert_eq!(
        plan_static_blas_restore(
            12,
            1200,
            100 * 1024 * 1024,
            1000,
            4 * 1024 * 1024 * 1024,
            MAX_STATIC_BLAS_RESTORES_PER_FRAME,
        ),
        12
    );
}

/// A large but fittable recovery is amortised across frames rather than
/// stalling one frame on a single fence-waiting batch.
#[test]
fn large_fittable_recovery_is_capped_per_frame() {
    assert_eq!(
        plan_static_blas_restore(
            5_000,
            8_000,
            100 * 1024 * 1024,
            1000,
            4 * 1024 * 1024 * 1024,
            MAX_STATIC_BLAS_RESTORES_PER_FRAME,
        ),
        MAX_STATIC_BLAS_RESTORES_PER_FRAME
    );
}

/// The #3540 hang: Starfield `citycydoniamainlevel` scale. ~95 k visible
/// rigid draws at a 100 KB mean project to ~9 GB against a 4 GB budget,
/// so every restore displaces a mesh the same frame still needs. The
/// pass must decline entirely instead of rebuild/evict thrashing — that
/// cycle is what pinned one core at frame 0 for over ten minutes.
#[test]
fn visible_set_larger_than_budget_declines_the_whole_pass() {
    assert_eq!(
        plan_static_blas_restore(
            40_000,
            95_095,
            100 * 1024 * 1024,
            1000,
            4 * 1024 * 1024 * 1024,
            MAX_STATIC_BLAS_RESTORES_PER_FRAME,
        ),
        0
    );
}

/// Exactly at the budget still fits — the projection declines only on a
/// strict breach, matching `blas_over_budget`'s `>` line.
#[test]
fn visible_set_exactly_at_budget_still_restores() {
    // 10 resident entries × 1 MB mean, 4096 visible → 4096 MB projected
    // against a 4096 MB budget.
    let budget = 4096 * 1024 * 1024;
    assert_eq!(
        plan_static_blas_restore(1, 4096, 10 * 1024 * 1024, 10, budget, 256),
        1
    );
    assert_eq!(
        plan_static_blas_restore(1, 4097, 10 * 1024 * 1024, 10, budget, 256),
        0
    );
}

/// With nothing resident there is no measured mean to project from, so
/// only the cap applies. It alone still bounds the frame.
#[test]
fn no_resident_entries_falls_back_to_the_cap_alone() {
    assert_eq!(
        plan_static_blas_restore(100_000, 100_000, 0, 0, 4 * 1024 * 1024 * 1024, 256),
        256
    );
}

/// Degenerate inputs must not panic or over-restore: nothing missing,
/// a zero cap, and a zero budget all resolve to "do nothing".
#[test]
fn degenerate_recovery_inputs_do_nothing() {
    assert_eq!(plan_static_blas_restore(0, 5000, 1024, 1, 1 << 30, 256), 0);
    assert_eq!(plan_static_blas_restore(10, 5000, 1024, 1, 1 << 30, 0), 0);
    assert_eq!(plan_static_blas_restore(10, 5000, 1024, 1, 0, 256), 0);
    // Saturating projection: a huge mean over a huge visible count must
    // clamp rather than wrap into a false "fits".
    assert_eq!(
        plan_static_blas_restore(10, usize::MAX, u64::MAX, 1, u64::MAX - 1, 256),
        0
    );
}
