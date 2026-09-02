//! Acceleration-structure tests — scratch-buffer sizing, alignment, growth/shrink and the serialize-barrier invariant.
//!
//! Split out of the 2 329-LOC monolithic `tests.rs` under #2977. Every
//! test here is a pure unit test (no live Vulkan context); the split
//! mirrors the production submodule names where tests exist for them.

use super::super::predicates::*;
use super::{KB, MB};

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
    let src = include_str!("../blas_skinned.rs");

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

/// #2915 / REN-D1-03 — two latent defects in `shrink_tlas_scratch_to_fit`'s
/// live-slot arm. Both need a live device plus a failing allocation, so —
/// matching this file's convention for rollback/ordering invariants — they
/// are pinned at the source level.
///
/// #2774 re-examined whether the live-slot arm can even be reached: its
/// audit premise was that `ensure_tlas_state` writes `current` (the
/// scratch buffer's actual capacity) and `peak`
/// (`tlas_scratch_peak_bytes`) together, so they can differ by at most
/// `scratch_align - 1` and `tlas_scratch_should_shrink`'s `current > 2 ×
/// peak` gate can never fire. That premise doesn't hold: `peak` is
/// recorded unconditionally on every fresh TLAS build (see
/// `fresh_build_records_peak_unconditionally_of_scratch_regrow` below),
/// but the scratch buffer itself is grow-only (`scratch_needs_growth`,
/// already pinned above) — a shrink-triggered rebuild
/// (`tlas_shrink_pending`, set by `shrink_tlas_to_fit`) that needs less
/// scratch than the slot's existing (oversized, from an earlier large
/// build) buffer updates `peak` down without touching `current` at all.
/// The next end-of-frame `shrink_tlas_scratch_to_fit` call then sees
/// exactly the divergence the live-slot arm exists to reclaim. The arm is
/// reachable; do not delete it as dead code.
#[cfg(test)]
mod tlas_scratch_shrink_tests {
    const MEMORY_RS: &str = include_str!("../memory.rs");
    const TLAS_RS: &str = include_str!("../tlas.rs");

    /// The live-slot arm's body, so the assertions can't match the
    /// `shrink_blas_scratch_to_fit` sibling above it.
    fn live_slot_arm() -> &'static str {
        let body = MEMORY_RS
            .split("pub unsafe fn shrink_tlas_scratch_to_fit")
            .nth(1)
            .expect("shrink_tlas_scratch_to_fit must still exist");
        body.split("\n    /// Current total BLAS memory")
            .next()
            .expect("the fn must still be followed by total_blas_bytes' doc")
    }

    /// Defect 1 — the realloc target must carry the alignment headroom
    /// `build_tlas`'s `align_scratch_address` round-up consumes. The
    /// recorded peak is the *unpadded* `build_scratch_size`, and unlike
    /// the BLAS path there is no per-build growth check to correct it.
    #[test]
    fn live_slot_realloc_target_carries_alignment_padding() {
        let arm = live_slot_arm();
        assert!(
            arm.contains("peak.saturating_add(scratch_alignment_padding(self.scratch_align))"),
            "the TLAS scratch shrink must reallocate at peak + \
             scratch_alignment_padding, as `shrink_blas_scratch_to_fit` and \
             `ensure_tlas_state` both do — sizing to the bare peak lets the \
             build's scratch range overrun the allocation by up to `align - 1` \
             bytes on a misaligning driver (#2915 / #1386)"
        );
    }

    /// Defect 2 — allocate-then-swap. Destroy-first left the slot's
    /// scratch `None` with `tlas[slot]` still `Some` on the error path,
    /// which the next `build_tlas` turns into an abort mid-recording.
    #[test]
    fn live_slot_allocates_replacement_before_retiring_the_old_buffer() {
        let arm = live_slot_arm();
        let alloc = arm
            .find("let new_buf = match GpuBuffer::create_device_local_uninit(")
            .expect("the live-slot arm must allocate its replacement into a local (#2915)");
        let destroy = arm
            .find("if let Some(mut old) = self.scratch_buffers[slot_index].take() {\n            old.destroy(device, allocator);\n        }\n        log::debug!(")
            .expect("the live-slot arm must still retire the old buffer");
        assert!(
            alloc < destroy,
            "the replacement must be allocated BEFORE the old buffer is \
             retired — destroy-first strands a live TLAS slot with no scratch \
             when the allocation fails, under exactly the VRAM pressure the \
             shrink exists to relieve (#2915, mirroring #2673)"
        );
        assert!(
            arm.contains("return false;"),
            "the failure path must keep the existing buffer and report that \
             nothing changed, not claim a reallocation happened (#2915)"
        );
    }

    /// `build_tlas` must not abort the process inside an open command
    /// buffer recording if a slot's scratch is ever missing.
    #[test]
    fn build_tlas_does_not_unwrap_the_slot_scratch() {
        assert!(
            !TLAS_RS.contains("self.scratch_buffers[frame_index].as_ref().unwrap()"),
            "build_tlas must not `unwrap` the slot scratch — the panic lands \
             inside an open command-buffer recording, so the frame is never \
             ended or submitted. Return Err instead and let draw_frame's \
             `tlas_build_failed` arm degrade to no-RT for the frame (#2915)"
        );
        assert!(
            TLAS_RS.contains("the slot is live but its \\\n                     scratch was retired without a replacement"),
            "the replacement must explain the invariant it guards (#2915)"
        );
    }

    /// #2774 — the structural fact that makes `shrink_tlas_scratch_to_fit`'s
    /// live-slot arm reachable: `ensure_tlas_state`'s fresh-build path
    /// records `tlas_scratch_peak_bytes[frame_index]` OUTSIDE (after) the
    /// `if let Some(scratch) = new_scratch { .. }` block that would
    /// reallocate the scratch buffer — so a fresh build that reuses the
    /// existing (larger) scratch buffer still lowers the recorded peak.
    /// If a future refactor moved the peak write inside that block (so it
    /// only updates alongside a real reallocation), `current` and `peak`
    /// would go back to always matching and the live-slot arm this test
    /// file spends two other tests pinning would become genuinely dead
    /// code — this test exists so that change doesn't happen unnoticed.
    #[test]
    fn fresh_build_records_peak_unconditionally_of_scratch_regrow() {
        let new_scratch_close = TLAS_RS
            .find(
                "                self.scratch_buffers[frame_index] = Some(scratch);\n            }",
            )
            .expect(
                "ensure_tlas_state must still conditionally reallocate the scratch buffer \
                 inside an `if let Some(scratch) = new_scratch` block",
            );
        let peak_write = TLAS_RS
            .find("self.tlas_scratch_peak_bytes[frame_index] = sizes.build_scratch_size;")
            .expect("ensure_tlas_state must still record the fresh build's scratch peak");
        assert!(
            new_scratch_close < peak_write,
            "the peak write must come AFTER (outside) the conditional scratch \
             reallocation, not be nested inside it — moving it inside would make \
             peak track current 1:1 again, and the live-slot shrink arm in \
             `shrink_tlas_scratch_to_fit` would become unreachable (#2774)"
        );
    }
}
