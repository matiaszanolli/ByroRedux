//! Compressed B-spline evaluation (`anim/bspline.rs`, issue #155).

use super::super::*;

#[test]
fn bspline_dequant_midpoint() {
    // raw=0 → offset; raw=32767 → offset + half_range; raw=-32767 → offset - half_range
    assert!((dequant(0, 10.0, 5.0) - 10.0).abs() < 1e-5);
    assert!((dequant(32767, 10.0, 5.0) - 15.0).abs() < 1e-4);
    assert!((dequant(-32767, 10.0, 5.0) - 5.0).abs() < 1e-4);
}

#[test]
fn deboor_cubic_clamped_endpoints() {
    // With 4 control points on a single-scalar channel, the cubic
    // B-spline at u=0 should equal CP[0], at u=1 should equal CP[3]
    // because an open uniform knot vector is fully clamped at both
    // ends for the minimum degree-3 case.
    let cps = vec![1.0, 2.0, 3.0, 10.0];
    let v0 = deboor_cubic(&cps, 4, 1, 0.0);
    let v1 = deboor_cubic(&cps, 4, 1, 1.0);
    assert!(
        (v0[0] - 1.0).abs() < 1e-4,
        "u=0 should give CP[0], got {}",
        v0[0]
    );
    assert!(
        (v1[0] - 10.0).abs() < 1e-4,
        "u=1 should give CP[3], got {}",
        v1[0]
    );
}

#[test]
fn deboor_cubic_monotone_between_endpoints() {
    // With a monotone CP sequence and a monotone knot parameter,
    // the evaluated curve should also be monotone (not strictly,
    // but the sign of successive differences should agree).
    let cps = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let n = 5;
    let u_max = (n - BSPLINE_DEGREE) as f32;
    let mut prev = f32::NEG_INFINITY;
    for i in 0..=10 {
        let u = u_max * (i as f32 / 10.0);
        let v = deboor_cubic(&cps, n, 1, u)[0];
        assert!(
            v >= prev - 1e-4,
            "non-monotone: v[{}] = {} < prev {}",
            i,
            v,
            prev
        );
        prev = v;
    }
}

#[test]
fn bspline_channel_slice_invalid_handle() {
    let raw: Vec<i16> = vec![0; 100];
    assert!(channel_slice(u32::MAX, &raw, 4, 3, 0.0, 1.0).is_none());
}

#[test]
fn bspline_channel_slice_out_of_bounds() {
    let raw: Vec<i16> = vec![0; 10];
    // Needs 4 * 3 = 12 slots starting at handle 0 → should fail (only 10).
    assert!(channel_slice(0, &raw, 4, 3, 0.0, 1.0).is_none());
}

#[test]
fn bspline_channel_slice_dequantizes() {
    // 4 CPs × stride 1, raw values [0, 32767, -32767, 0]
    // with offset=10, half_range=5 → [10, 15, 5, 10]
    let raw: Vec<i16> = vec![0, 32767, -32767, 0];
    let out = channel_slice(0, &raw, 4, 1, 10.0, 5.0).unwrap();
    assert_eq!(out.len(), 4);
    assert!((out[0] - 10.0).abs() < 1e-4);
    assert!((out[1] - 15.0).abs() < 1e-4);
    assert!((out[2] - 5.0).abs() < 1e-4);
    assert!((out[3] - 10.0).abs() < 1e-4);
}

// #3765 (SAFE-2026-08-30-D9-01) — `translation_offset` / `translation_half_range`
// (and the rotation/scale/float siblings) are raw f32s read off disk with no
// validation. A NaN/±Inf in either poisoned every dequantized control point
// and `deboor_cubic` propagated it into the sampled channel unfiltered — the
// mainline keyframe converters (`sanitize_keyframe_streams`, #1443) and the
// pose-fallback branches (`is_flt_max`) were already gated; this sampled path,
// the whole point of the block, had neither. `channel_slice` is the single
// choke point all four callers (float channel; transform channel's
// translation/rotation/scale) funnel through, so a fixture here pins all four
// at once.
#[test]
fn bspline_channel_slice_rejects_nan_offset() {
    let raw: Vec<i16> = vec![0, 32767, -32767, 0];
    assert!(
        channel_slice(0, &raw, 4, 1, f32::NAN, 5.0).is_none(),
        "a NaN offset must drop the whole channel to the pose fallback, \
         not produce a NaN-poisoned control point"
    );
}

#[test]
fn bspline_channel_slice_rejects_nan_half_range() {
    let raw: Vec<i16> = vec![0, 32767, -32767, 0];
    assert!(
        channel_slice(0, &raw, 4, 1, 10.0, f32::NAN).is_none(),
        "a NaN half_range must drop the whole channel to the pose fallback"
    );
}

#[test]
fn bspline_channel_slice_rejects_infinite_quantization_params() {
    let raw: Vec<i16> = vec![0, 32767, -32767, 0];
    assert!(channel_slice(0, &raw, 4, 1, f32::INFINITY, 5.0).is_none());
    assert!(channel_slice(0, &raw, 4, 1, f32::NEG_INFINITY, 5.0).is_none());
    assert!(channel_slice(0, &raw, 4, 1, 10.0, f32::INFINITY).is_none());
    assert!(channel_slice(0, &raw, 4, 1, 10.0, f32::NEG_INFINITY).is_none());
}

/// Belt-and-braces layer: even with finite `offset`/`half_range`, a
/// pathologically large (but still finite) `half_range` can overflow
/// `deboor_cubic`'s repeated blending into a non-finite sampled value. This
/// pins that the sampled-value guards added at the three `push` call sites
/// (`extract_float_channel_bspline`, `extract_transform_channel_bspline`'s
/// translation/scale) actually reject such a value rather than propagating
/// it — reproduced directly against `dequant`/`deboor_cubic`, the same
/// primitives those call sites use.
#[test]
fn bspline_dequant_and_deboor_can_overflow_and_the_guard_catches_it() {
    // `dequant`'s ratio term (`raw / 32767.0`) is bounded to [-1, 1], so
    // `offset + ratio * half_range` alone only overflows when `offset` and
    // the scaled `half_range` are both already near f32::MAX and add past
    // it — both individually finite, both individually pass
    // `is_key_value_sane`, exactly the "finite but pathological" case the
    // belt-and-braces guard exists for.
    let poisoned = dequant(32767, f32::MAX, f32::MAX);
    assert!(!poisoned.is_finite(), "test fixture sanity: this must overflow");
    assert!(
        !is_key_value_sane(poisoned),
        "the sampled-value guard must recognize this as unsafe to push as a key"
    );
}

/// Regression: #4166 (NIFAL-D7-2026-09-11-01) — the rotation sub-channel
/// was the one #3765 left unguarded, and the two failure modes it admits
/// are *not* the ones the report predicted. This drives the production
/// guard directly rather than reproducing its arithmetic in the test.
#[test]
fn bspline_rotation_sample_rejects_infinite_control_points() {
    // The genuine NaN path: an infinite component survives to `inf * 0.0`.
    for poisoned in [
        [f32::INFINITY, 0.0, 0.0, 0.0],
        [0.0, f32::NEG_INFINITY, 0.0, 0.0],
        [0.0, 0.0, f32::INFINITY, 0.0],
        [0.0, 0.0, 0.0, f32::INFINITY],
    ] {
        assert!(
            normalized_rotation_sample(poisoned).is_none(),
            "an infinite control point must skip the sample, not push NaN: {poisoned:?}"
        );
    }
    // FLT_MAX sentinel components are rejected on the same sweep.
    assert!(normalized_rotation_sample([f32::MAX, 0.0, 0.0, 0.0]).is_none());
}

/// Regression: #4166 / #4406. The case the issue named as its headline
/// hazard, with the correction that matters: control points that are
/// individually sane but square past `f32::MAX` do **not** produce NaN —
/// every component is finite, so `finite * 0.0 == 0.0` and the result is
/// the *zero quaternion*.
///
/// That is exactly why the guard is `len_sq.is_finite()` and not the
/// post-normalize `is_key_value_sane` check the issue prescribed:
/// `[0, 0, 0, 0]` passes `is_key_value_sane` on all four components, so a
/// post-normalize guard is blind to it. Asserted here so nobody
/// "simplifies" the guard back to the sibling one-liner.
///
/// #4406 — the overflow arm originally substituted identity, contradicting
/// the guard's own doc and the translation/scale/float siblings, which all
/// skip the sample. It now returns `None` (skip → bind pose), matching
/// them.
#[test]
fn bspline_rotation_sample_skips_when_squaring_overflows() {
    // sqrt(f32::MAX) ≈ 1.844e19 — just past it, so v*v is inf.
    let huge = 2.0e19f32;
    assert!(
        is_key_value_sane(huge),
        "fixture sanity: each component must individually pass the sibling guard"
    );
    assert!(
        !(huge * huge).is_finite(),
        "fixture sanity: squaring must overflow to inf"
    );

    assert!(
        normalized_rotation_sample([huge, 0.0, 0.0, 0.0]).is_none(),
        "an overflowing len_sq must skip the sample like the sibling \
         sub-channels, not substitute an invented identity pose (#4406)"
    );

    // The refutation of a post-normalize guard, pinned: the zero quaternion
    // the unguarded code produced would have passed a post-normalize check.
    assert!(
        [0.0f32, 0.0, 0.0, 0.0]
            .iter()
            .all(|v| is_key_value_sane(*v)),
        "a post-normalize is_key_value_sane guard cannot see the zero quaternion"
    );
}

/// Regression: #4166. A NaN control point was already safe before this
/// fix — `len_sq` is NaN, `NaN > f32::EPSILON` is false, and the
/// degenerate arm substitutes identity. Pinned so the guard's docs and
/// the code keep agreeing about which inputs were actually broken.
/// (#4406 narrowed the identity arm to near-zero only; NaN was already
/// rejected up front by the pre-normalize sweep and is unaffected.)
#[test]
fn bspline_rotation_sample_was_already_safe_against_nan_control_points() {
    for raw in [[f32::NAN; 4], [f32::NAN, 1.0, 0.0, 0.0]] {
        // Rejected up front now, but the point is that the *result* was
        // never NaN even before the guard existed.
        assert!(normalized_rotation_sample(raw).is_none());
        let len_sq: f32 = raw.iter().map(|v| v * v).sum();
        assert!(len_sq.is_nan());
        // NaN compares false against everything — that is the property the
        // guard below leans on, stated as the incomparability it is.
        assert_eq!(len_sq.partial_cmp(&f32::EPSILON), None);
    }
}

/// Regression: #4166. The guard must not disturb ordinary content — a
/// non-unit but well-scaled quaternion still normalizes as before.
/// (#4396 — the near-zero arm moved from identity-substitution to skip
/// when this sanitizer became the single rotation-key sanitizer; that
/// case now has its own pin in `tests/sanitize.rs`.)
#[test]
fn bspline_rotation_sample_still_normalizes_ordinary_quaternions() {
    let q = normalized_rotation_sample([0.0, 3.0, 4.0, 0.0]).expect("ordinary sample must survive");
    let len = (q.iter().map(|v| v * v).sum::<f32>()).sqrt();
    assert!((len - 1.0).abs() < 1e-5, "must be unit length, got {len}");
    assert!(
        (q[1] - 0.6).abs() < 1e-5 && (q[2] - 0.8).abs() < 1e-5,
        "{q:?}"
    );
    // #4396 — a near-zero (all-zero authored) quaternion skips like every
    // other rejection: identity substitution was an invented pose, and
    // `normalize_quat`'s zero-length pass-through let the raw zero quat
    // sail through on the converter paths.
    assert_eq!(
        normalized_rotation_sample([0.0, 0.0, 0.0, 0.0]),
        None,
        "near-zero must skip, not substitute identity (#4396)"
    );
}

/// #4396 / #4397 — the B-spline STATIC fallback pose (`n_cp < 4` →
/// `static_transform_channel`, and the per-sample `else` arms) had the
/// same two holes as the mainline converters: `is_flt_max`-only gates
/// let NaN through, and the rotation was copied raw so overflow/zero
/// quaternions reached keys. Both routes now share
/// `normalized_rotation_sample` and the `is_key_value_sane` gates.
#[test]
fn bspline_static_pose_drops_nan_overflow_and_sentinel_components() {
    use crate::blocks::interpolator::NiBSplineCompTransformInterpolator;
    use crate::types::{NiPoint3, NiQuatTransform};

    let interp_for = |pose: NiQuatTransform| NiBSplineCompTransformInterpolator {
        start_time: 0.0,
        stop_time: 1.0,
        spline_data_ref: crate::types::BlockRef::NULL,
        basis_data_ref: crate::types::BlockRef::NULL,
        transform: pose,
        translation_handle: u32::MAX,
        rotation_handle: u32::MAX,
        scale_handle: u32::MAX,
        translation_offset: 0.0,
        translation_half_range: 0.0,
        rotation_offset: 0.0,
        rotation_half_range: 0.0,
        scale_offset: 0.0,
        scale_half_range: 0.0,
    };

    // NaN pose — invisible to the old is_flt_max-only gates (#4397).
    let nan = static_transform_channel(&interp_for(NiQuatTransform {
        translation: NiPoint3 {
            x: f32::NAN,
            y: f32::NAN,
            z: f32::NAN,
        },
        rotation: [f32::NAN, f32::NAN, f32::NAN, f32::NAN],
        scale: f32::NAN,
    }));
    assert!(nan.translation_keys.is_empty(), "NaN translation drops");
    assert!(nan.rotation_keys.is_empty(), "NaN rotation drops");
    assert!(nan.scale_keys.is_empty(), "NaN scale drops");

    // Individually-sane rotation squaring past f32::MAX (#4396); clean
    // translation/scale on the same pose survive per-axis.
    let overflow = static_transform_channel(&interp_for(NiQuatTransform {
        translation: NiPoint3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        rotation: [2.0e19, 0.0, 0.0, 0.0],
        scale: 0.75,
    }));
    assert_eq!(overflow.translation_keys.len(), 1);
    assert!(
        overflow.rotation_keys.is_empty(),
        "overflow rotation must skip, not arrive as the zero quaternion (#4396)"
    );
    assert_eq!(overflow.scale_keys.len(), 1);

    // The FLT_MAX sentinel keeps its "axis inactive" semantics.
    let sentinel = static_transform_channel(&interp_for(NiQuatTransform {
        translation: NiPoint3 {
            x: f32::MAX,
            y: f32::MAX,
            z: f32::MAX,
        },
        rotation: [f32::MAX; 4],
        scale: f32::MAX,
    }));
    assert!(sentinel.translation_keys.is_empty());
    assert!(sentinel.rotation_keys.is_empty());
    assert!(sentinel.scale_keys.is_empty());
}

/// #5486 — the aggregate-amplification regression family.
///
/// Pre-fix, every B-spline channel was resampled at 30 Hz over the
/// *interpolator's own* `[start, stop]` span with a 1 M per-channel
/// ceiling and no aggregate bound. One shared interpolator with
/// `stop_time = 1e9` and N controlled blocks (each ~29 B on disk) cost
/// ~118 MiB per block — ~15 GB from a ~4 KB file, an allocation failure
/// that aborts the stream worker (`catch_unwind` cannot intercept it).
mod amplification_tests {
    use super::super::super::bspline::{
        BsplineSampling, BSPLINE_IMPORT_KEY_BUDGET, BSPLINE_MAX_SAMPLES_PER_CHANNEL,
    };

    #[test]
    fn sampled_span_is_clamped_to_the_owning_sequence() {
        let mut sampling = BsplineSampling::new();
        sampling.seq_span = Some((0.0, 1.0));
        // The audit repro's numbers: interpolator claims 1e9 s, the
        // owning sequence is 1 s. Pre-fix: 30 M samples (clamped to 1 M).
        let (start, stop, n) = sampling
            .reserve(0.0, 1.0e9, 3)
            .expect("well within budget");
        assert_eq!((start, stop), (0.0, 1.0));
        assert_eq!(n, 30, "1 s at 30 Hz");
    }

    #[test]
    fn per_channel_ceiling_is_one_hour_at_30_hz() {
        let mut sampling = BsplineSampling::new();
        // No owning sequence (embedded-controller path): only the
        // ceiling bounds the span.
        let (_, _, n) = sampling
            .reserve(0.0, 1.0e9, 1)
            .expect("within budget");
        assert_eq!(n, BSPLINE_MAX_SAMPLES_PER_CHANNEL);
        assert_eq!(n, 108_000, "30 Hz × 1 h (#5486 sizing)");
    }

    #[test]
    fn aggregate_budget_declines_once_spent() {
        let mut sampling = BsplineSampling::new();
        // A degenerate (0-span) channel still reserves the 2-sample
        // minimum per key vector, so each reserve(…, k) costs 2k.
        // Drain with small reservations until one is declined. Each
        // costs 2 samples × 2 key vectors = 4 keys, so the loop is
        // bounded well under BUDGET/4 iterations.
        let mut declined = false;
        for _ in 0..(BSPLINE_IMPORT_KEY_BUDGET / 4 + 2) {
            if sampling.reserve(0.0, 0.0, 2).is_none() {
                declined = true;
                break;
            }
        }
        assert!(declined, "the budget must run out eventually");
        // Once declined, even the smallest channel stays declined — a
        // shared interpolator can no longer multiply the cost past it.
        assert!(sampling.reserve(0.0, 0.0, 1).is_none());
        assert!(sampling.reserve(0.0, 100.0, 3).is_none());
    }

    /// The audit's end-to-end repro through the public `import_kf`:
    /// one `NiBSplineCompTransformInterpolator` with `stop_time = 1e9`
    /// shared by 8 controlled blocks of a sequence whose own span is
    /// 1 s. Pre-fix: ~118 MiB per block (3 M keys each). Post-fix: 30
    /// samples per block, and the import returns promptly.
    #[test]
    fn import_kf_bounds_a_shared_huge_span_interpolator() {
        use crate::blocks::controller::{ControlledBlock, NiControllerSequence};
        use crate::blocks::interpolator::{
            NiBSplineBasisData, NiBSplineCompTransformInterpolator, NiBSplineData,
        };
        use crate::scene::NifScene;
        use crate::types::BlockRef;
        use std::sync::Arc;

        let interp = NiBSplineCompTransformInterpolator {
            start_time: 0.0,
            stop_time: 1.0e9,
            spline_data_ref: BlockRef(2),
            basis_data_ref: BlockRef(1),
            transform: Default::default(),
            // INVALID handles + empty spline data: every sample falls
            // back to the static pose, which is exactly the audit's
            // "costs the allocation with no data at all" shape.
            translation_handle: u32::MAX,
            rotation_handle: u32::MAX,
            scale_handle: u32::MAX,
            translation_offset: 0.0,
            translation_half_range: 1.0,
            rotation_offset: 0.0,
            rotation_half_range: 1.0,
            scale_offset: 0.0,
            scale_half_range: 1.0,
        };
        let blocks: Vec<Box<dyn crate::NiObject>> = vec![
            Box::new(interp),
            Box::new(NiBSplineBasisData {
                num_control_points: 4,
            }),
            Box::new(NiBSplineData {
                float_control_points: Vec::new(),
                compact_control_points: Vec::new(),
            }),
        ];
        let mut scene = NifScene::default();
        for b in blocks {
            scene.blocks.push(b);
        }
        let seq = NiControllerSequence {
            name: Some(Arc::from("Amp")),
            controlled_blocks: (0..8)
                .map(|i| ControlledBlock {
                    interpolator_ref: BlockRef(0),
                    controller_ref: BlockRef::NULL,
                    priority: 0,
                    node_name: Some(Arc::from(format!("Bone{i}").as_str())),
                    property_type: None,
                    controller_type: Some(Arc::from("NiTransformController")),
                    controller_id: None,
                    interpolator_id: None,
                    string_palette_ref: BlockRef::NULL,
                    node_name_offset: 0,
                    property_type_offset: 0,
                    controller_type_offset: 0,
                    controller_id_offset: 0,
                    interpolator_id_offset: 0,
                })
                .collect(),
            array_grow_by: 0,
            weight: 1.0,
            text_keys_ref: BlockRef::NULL,
            cycle_type: 2,
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 1.0,
            manager_ref: BlockRef::NULL,
            accum_root_name: None,
            anim_note_refs: Vec::new(),
        };
        scene.blocks.push(Box::new(seq));
        scene.root_index = Some(3);

        let clips = super::super::super::import_kf(&scene);
        assert_eq!(clips.len(), 1, "the one sequence imports");
        let clip = &clips[0];
        assert_eq!(clip.channels.len(), 8, "every controlled block channels");
        for channel in clip.channels.values() {
            assert_eq!(
                channel.translation_keys.len(),
                30,
                "span must clamp to the sequence's 1 s at 30 Hz, not the \
                 interpolator's 1e9 s (pre-fix: 1 M keys ≈ 118 MiB/block)"
            );
        }
    }
}
