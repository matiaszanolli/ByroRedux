//! Acceleration-structure tests — TLAS instance mapping, transforms, shrink/handle lifecycle and build bookkeeping.
//!
//! Split out of the 2 329-LOC monolithic `tests.rs` under #2977. Every
//! test here is a pure unit test (no live Vulkan context); the split
//! mirrors the production submodule names where tests exist for them.

use super::super::predicates::*;
use super::super::*;
use super::{make_draw_command, KB, MB};

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

// ── tlas_scratch_should_shrink (#1226) ────────────────────────────
//
// Pre-#1226 the TLAS scratch shrink path called `scratch_should_shrink`
// with its 16 MB BLAS-scale slack; TLAS scratch lives at tens of KB to
// <1 MB so the entire mechanism was dead code. The new predicate uses
// a 256 KB TLAS-calibrated slack. Tests below pin the threshold math
// against realistic TLAS scratch scales.

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
        MATERIAL_KIND_MULTI_LAYER_PARALLAX, MATERIAL_KIND_NO_LIGHTING,
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
    let src = include_str!("../../context/draw.rs");

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
