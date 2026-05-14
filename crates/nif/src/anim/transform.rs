//! TRS channel extraction.
//!
//! Transform interpolators → engine-side translation / rotation / scale
//! channels.

use super::*;
use crate::blocks::controller::ControlledBlock;
use crate::blocks::interpolator::{
    KeyType, NiBSplineCompTransformInterpolator, NiLookAtInterpolator,
    NiPathInterpolator, NiPosData, NiTransformData, NiTransformInterpolator,
};
use crate::scene::NifScene;

pub fn extract_transform_channel(scene: &NifScene, cb: &ControlledBlock) -> Option<TransformChannel> {
    let mut interp_idx = cb.interpolator_ref.index()?;

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
        let data_idx = interp.data_ref.index()?;
        let data = scene.get_as::<NiTransformData>(data_idx)?;

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

    // Fall back to the Skyrim / FO4 NiBSplineCompTransformInterpolator path.
    // See issue #155. The B-spline is evaluated at BSPLINE_SAMPLE_HZ and
    // emitted as linear-interpolated TQS keys.
    if let Some(interp) = scene.get_as::<NiBSplineCompTransformInterpolator>(interp_idx) {
        return extract_transform_channel_bspline(scene, interp);
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

    // #605 — NiPathInterpolator references an NiPosData whose
    // `KeyGroup<Vec3Key>` IS the position-vs-time animation. nif.xml
    // documents a separate `percent_data_ref` (NiFloatData) for
    // non-uniform path traversal, but vanilla content (Oblivion door
    // hinges, FO3/FNV moving platforms, Skyrim minecart rails) uses
    // the simple time-keyed form where path keys map directly to
    // animation time. Emit the path keys as translation keys via the
    // shared `convert_vec3_keys` (Z-up → Y-up + interpolation type
    // preserved); rotation/scale stay identity to match the legacy
    // Gamebryo path-interpolator semantics. Frenet-frame rotation
    // (banking via `bank_dir` / `max_bank_angle`, follow-axis tangent
    // alignment) is a future improvement once a real consumer needs
    // it. Pre-#605 every embedded path animation static-posed.
    if let Some(interp) = scene.get_as::<NiPathInterpolator>(interp_idx) {
        return extract_transform_channel_path(scene, interp);
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
    let pose_t = [t.translation.x, t.translation.y, t.translation.z];
    let translation_keys =
        if is_flt_max(pose_t[0]) || is_flt_max(pose_t[1]) || is_flt_max(pose_t[2]) {
            Vec::new()
        } else {
            vec![TranslationKey {
                time: 0.0,
                value: zup_to_yup_pos(pose_t),
                forward: [0.0; 3],
                backward: [0.0; 3],
                tbc: None,
            }]
        };
    let rotation_keys = if is_flt_max(t.rotation[0])
        || is_flt_max(t.rotation[1])
        || is_flt_max(t.rotation[2])
        || is_flt_max(t.rotation[3])
    {
        Vec::new()
    } else {
        vec![RotationKey {
            time: 0.0,
            value: zup_to_yup_quat(t.rotation),
            tbc: None,
        }]
    };
    let scale_keys = if is_flt_max(t.scale) {
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
        translation_type: KeyType::Linear,
        rotation_keys,
        rotation_type: KeyType::Linear,
        scale_keys,
        scale_type: KeyType::Linear,
        priority: 0,
    }
}

/// Extract a `TransformChannel` from an `NiPathInterpolator`. The path's
/// `NiPosData` keys become translation keys; rotation and scale stay
/// identity. Returns `None` if `path_data_ref` is null, the referenced
/// block isn't an `NiPosData`, or the data carries zero keys (no useful
/// animation to emit).
pub fn extract_transform_channel_path(
    scene: &NifScene,
    interp: &NiPathInterpolator,
) -> Option<TransformChannel> {
    let data_idx = interp.path_data_ref.index()?;
    let data = scene.get_as::<NiPosData>(data_idx)?;
    if data.keys.keys.is_empty() {
        return None;
    }
    let (translation_keys, translation_type) = convert_vec3_keys(&data.keys);
    Some(TransformChannel {
        translation_keys,
        translation_type,
        rotation_keys: vec![RotationKey {
            time: 0.0,
            value: [0.0, 0.0, 0.0, 1.0], // identity quat (x, y, z, w)
            tbc: None,
        }],
        rotation_type: KeyType::Linear,
        scale_keys: vec![ScaleKey {
            time: 0.0,
            value: 1.0,
            forward: 0.0,
            backward: 0.0,
            tbc: None,
        }],
        scale_type: KeyType::Linear,
        priority: 0,
    })
}
