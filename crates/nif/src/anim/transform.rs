//! TRS channel extraction.
//!
//! Transform interpolators → engine-side translation / rotation / scale
//! channels.

use super::*;
use crate::blocks::controller::ControlledBlock;
use crate::blocks::interpolator::{
    NiBSplineCompTransformInterpolator, NiFloatData, NiLookAtInterpolator, NiPathInterpolator,
    NiPosData, NiTransformData, NiTransformInterpolator,
};
use byroredux_core::animation::types::KeyType as CoreKeyType;
use byroredux_core::math::{Quat, Vec3};
use crate::scene::NifScene;

pub fn extract_transform_channel(
    scene: &NifScene,
    cb: &ControlledBlock,
    sampling: &mut super::bspline::BsplineSampling,
) -> Option<TransformChannel> {
    extract_transform_channel_at(scene, cb.interpolator_ref.index()?, sampling)
}

/// `ControlledBlock`-free TRS extraction, keyed directly on an interpolator
/// block index. The KF-sequence path reaches it via the controlled block's
/// `interpolator_ref` ([`extract_transform_channel`]); the embedded-animation
/// path (an inline `NiKeyframeController` / `NiTransformController` hung off a
/// node, with no `NiControllerSequence`) reaches it via the controller's own
/// `NiSingleInterpController::interpolator_ref`. Both arrive at the same
/// interpolator, so the dispatch below — blend-target follow, modern
/// `NiTransformInterpolator`, B-spline, look-at static pose, path — is shared.
/// `priority` is left at 0; the sequence path overwrites it with `cb.priority`.
pub fn extract_transform_channel_at(
    scene: &NifScene,
    mut interp_idx: usize,
    sampling: &mut super::bspline::BsplineSampling,
) -> Option<TransformChannel> {
    // #334 (AR-08) — NIFs with embedded controller managers commonly
    // bind a `NiBlendTransformInterpolator` between the ControlledBlock
    // and the real NiTransformInterpolator so runtime can weight
    // multiple active sequences. Follow the blend's dominant
    // sub-interpolator so the channel extraction reaches the real
    // keyframe data instead of silently returning None.
    if let Some(resolved) = resolve_blend_interpolator_target(scene, interp_idx) {
        interp_idx = resolved;
    }

    // Try the modern NiTransformInterpolator → NiTransformData path first.
    if let Some(interp) = scene.get_as::<NiTransformInterpolator>(interp_idx) {
        // #3316 / FNV-D6-01 — a null `data_ref` (or a data ref that does not
        // resolve to an `NiTransformData`) is NOT "no channel": nif.xml
        // documents `NiTransformInterpolator.Transform` as the pose used when
        // the keyframe data is absent, exactly like its `NiPoint3Interpolator`
        // sibling's "Pose value if lacking NiPosData" (nif.xml:3250-3256), and
        // Gamebryo v2.6/v3.2 describe the same field as "this NiQuatTransform
        // can be an unchanging pose".
        //
        // Returning `None` here dropped 49 182 of 123 729 (39.7%) of every FNV
        // transform controlled block, imported 64 vanilla `.kf` files as zero
        // clips (`2hrdeath`, `mtjumploop`, holster/aim), and cost live clips
        // real channels (`mtidle.kf` lost Neck + both Clavicles).
        // `constant_transform_channel` is the same helper `NiLookAtInterpolator`
        // has used since #604, and it already honours the FLT_MAX per-axis
        // "no pose for this axis" sentinel.
        let Some(data) = interp
            .data_ref
            .index()
            .and_then(|i| scene.get_as::<NiTransformData>(i))
        else {
            return Some(constant_transform_channel(&interp.transform));
        };

        let (translation_keys, translation_type) = convert_vec3_keys(&data.translations);
        let (rotation_keys, rotation_type) = convert_quat_keys(data);
        let (scale_keys, scale_type) = convert_float_keys(&data.scales);

        return Some(TransformChannel {
            translation_keys,
            translation_type,
            rotation_keys,
            rotation_type,
            scale_keys,
            scale_type,
            priority: 0, // Will be overwritten by import_sequence with cb.priority.
        });
    }

    // Fall back to the NiBSplineCompTransformInterpolator path (FO3/FNV onwards).
    // See issue #155. The B-spline is evaluated at BSPLINE_SAMPLE_HZ and
    // emitted as linear-interpolated TQS keys.
    if let Some(interp) = scene.get_as::<NiBSplineCompTransformInterpolator>(interp_idx) {
        return extract_transform_channel_bspline(scene, interp, sampling);
    }

    // #604 — NiLookAtInterpolator carries a static `transform`
    // (NiQuatTransform) that nif.xml documents as the pose used when
    // the three TRS sub-interpolators are null. Without this branch
    // every embedded look-at chain (FNV ~18 / SkyrimSE ~5 per R3 sweep)
    // returned None and silently dropped the channel. Emitting a
    // single-key constant TransformChannel from the static pose makes
    // the fall-back explicit. The runtime look-at solve (rotate-to-
    // face the target NiNode each frame) is a separate ECS-side
    // feature; the parser-side dispatch hole is what this closes.
    if let Some(interp) = scene.get_as::<NiLookAtInterpolator>(interp_idx) {
        return Some(constant_transform_channel(&interp.transform));
    }

    // #605 / #5571 — NiPathInterpolator: the NiPosData keys are a *parametric*
    // curve (key time = ratio 0..1 along the path) and the NiFloatData
    // "percent" keys map animation time onto that ratio (Gamebryo 3.2
    // `NiPathInterpolator` reference: "uses the input time to interpolate a
    // percentage value from the percentage keys. It then uses this
    // percentage to determine a position along the spline"). The importer
    // composes the two into translation keys over animation time. Follow-axis
    // / banking / constant-velocity are parked — see `nifal.md` §Animation.
    if let Some(interp) = scene.get_as::<NiPathInterpolator>(interp_idx) {
        return extract_transform_channel_path(scene, interp, sampling);
    }

    None
}

/// Build a single-key `TransformChannel` from a static `NiQuatTransform`.
/// Used by `NiLookAtInterpolator` (and any future block that exposes a
/// pose without keyframe data) to surface the documented fall-back pose
/// instead of dropping the channel.
pub fn constant_transform_channel(t: &crate::types::NiQuatTransform) -> TransformChannel {
    // FLT_MAX in any TRS axis means "no static pose for this axis";
    // emit an empty key list so the bone keeps its bind-pose value
    // for that axis. See FLT_MAX_SENTINEL above.
    //
    // #4397 — the gates below read [`is_key_value_sane`], not bare
    // `is_flt_max`: the sentinel check alone is false for NaN, so a NaN
    // pose component passed every gate and reached the canonical clip.
    // `is_key_value_sane` keeps the sentinel semantics (it rejects
    // FLT_MAX) and adds NaN / ±inf.
    let pose_t = [t.translation.x, t.translation.y, t.translation.z];
    let translation_keys = if [pose_t[0], pose_t[1], pose_t[2]]
        .iter()
        .any(|v| !is_key_value_sane(*v))
    {
        Vec::new()
    } else {
        vec![TranslationKey {
            time: 0.0,
            value: Vec3::from_array(zup_to_yup_pos(pose_t)),
            forward: Vec3::ZERO,
            backward: Vec3::ZERO,
            tbc: None,
        }]
    };
    // #4396 — the shared sanitizer, not a raw copy: a non-unit raw pose
    // quaternion now arrives normalized, and the zero/overflow/non-finite
    // classes (invisible or mis-handled by the old `is_flt_max` gate)
    // skip to an empty list — bind pose — like every other rejection.
    let rotation_keys = match normalized_rotation_sample(t.rotation) {
        None => Vec::new(),
        Some(q) => vec![RotationKey {
            time: 0.0,
            value: Quat::from_array(zup_to_yup_quat(q)),
            tbc: None,
        }],
    };
    let scale_keys = if !is_key_value_sane(t.scale) {
        Vec::new()
    } else {
        vec![ScaleKey {
            time: 0.0,
            value: t.scale,
            forward: 0.0,
            backward: 0.0,
            tbc: None,
        }]
    };
    TransformChannel {
        translation_keys,
        translation_type: CoreKeyType::Linear,
        rotation_keys,
        rotation_type: CoreKeyType::Linear,
        scale_keys,
        scale_type: CoreKeyType::Linear,
        priority: 0,
    }
}

/// Extract a `TransformChannel` from an `NiPathInterpolator`. The path's
/// Compose an `NiPathInterpolator`'s percent curve with its path spline into
/// translation keys over animation time (#5571).
///
/// `position(t) = path(percent(t))`: the `NiFloatData` percent keys are
/// evaluated at animation time `t`, and that percentage is the key time at
/// which the `NiPosData` path keys (time domain 0..1) are evaluated. Both
/// curves keep their authored interpolation type (linear / Hermite / TBC) —
/// evaluation goes through `byroredux_core::animation::interpolation`, the
/// same code the runtime uses — and the composition is resampled to linear
/// keys at [`BSPLINE_SAMPLE_HZ`] over the percent curve's span (plus the
/// percent key times themselves, so authored corners stay exact), under the
/// same span / ceiling / aggregate-budget bounds as B-spline resampling.
///
/// Rotation and scale are left empty so the node keeps its bind rotation and
/// scale; follow-axis alignment, banking and constant-velocity
/// reparameterisation are not translated (parked, `nifal.md` §Animation). A
/// percentage outside the path's key range holds the nearest end — what
/// Gamebryo does there for closed curves is not in the available reference
/// headers. Returns `None` when the path or percent data is missing or
/// empty: the interpolator has no meaning without both, and a guessed
/// time-keyed fallback would be invented motion.
pub fn extract_transform_channel_path(
    scene: &NifScene,
    interp: &NiPathInterpolator,
    sampling: &mut super::bspline::BsplineSampling,
) -> Option<TransformChannel> {
    use byroredux_core::animation::interpolation::{sample_scale, sample_translation};
    use byroredux_core::animation::types as core;

    let path = scene.get_as::<NiPosData>(interp.path_data_ref.index()?)?;
    let Some(pct) = interp
        .percent_data_ref
        .index()
        .and_then(|i| scene.get_as::<NiFloatData>(i))
    else {
        log::debug!("NiPathInterpolator without percent data — no time→path mapping, skipped (#5571)");
        return None;
    };
    let (path_keys, path_type) = convert_vec3_keys(&path.keys);
    let (pct_keys, pct_type) = convert_float_keys(&pct.keys);
    if path_keys.is_empty() || pct_keys.is_empty() {
        return None;
    }

    // Sample times: a uniform grid over the percent span, bounded like the
    // B-spline resampler, plus every percent key time inside the window.
    // (Read before the key vectors move into the curves below.)
    let (span_start, span_stop) = (pct_keys.first()?.time, pct_keys.last()?.time);
    let times: Vec<f32> = match sampling.reserve(span_start, span_stop, 1) {
        Some(w) if w.stop > w.start => {
            let mut t: Vec<f32> = (0..w.n_samples).map(|i| w.sample(i, 1.0).0).collect();
            t.extend(
                pct_keys
                    .iter()
                    .map(|k| k.time)
                    .filter(|&k| k >= w.start && k <= w.stop),
            );
            t.sort_by(f32::total_cmp);
            t.dedup();
            t
        }
        Some(w) => vec![w.start],
        // Budget spent: the pose at the percent curve's first key.
        None => vec![span_start],
    };

    // #5798 — the extracted keys are already the core types, so the
    // shared evaluators take the curves as-is (this used to shadow-copy
    // each key into a core-typed twin just to call them).
    let path_curve = core::TransformChannel {
        translation_keys: path_keys,
        translation_type: path_type,
        rotation_keys: Vec::new(),
        rotation_type: core::KeyType::Linear,
        scale_keys: Vec::new(),
        scale_type: core::KeyType::Linear,
        priority: 0,
    };
    let pct_curve = core::TransformChannel {
        translation_keys: Vec::new(),
        translation_type: core::KeyType::Linear,
        rotation_keys: Vec::new(),
        rotation_type: core::KeyType::Linear,
        scale_keys: pct_keys,
        scale_type: pct_type,
        priority: 0,
    };

    let translation_keys: Vec<TranslationKey> = times
        .into_iter()
        .filter_map(|t| {
            let percent = sample_scale(&pct_curve, t)?;
            let p = sample_translation(&path_curve, percent)?;
            (is_key_value_sane(t)
                && [p.x, p.y, p.z].iter().all(|v| is_key_value_sane(*v)))
            .then_some(TranslationKey {
                time: t,
                value: p,
                forward: Vec3::ZERO,
                backward: Vec3::ZERO,
                tbc: None,
            })
        })
        .collect();
    if translation_keys.is_empty() {
        return None;
    }
    Some(TransformChannel {
        translation_keys,
        translation_type: CoreKeyType::Linear,
        rotation_keys: Vec::new(),
        rotation_type: CoreKeyType::Linear,
        scale_keys: Vec::new(),
        scale_type: CoreKeyType::Linear,
        priority: 0,
    })
}
