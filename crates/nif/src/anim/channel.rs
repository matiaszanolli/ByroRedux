//! Float / Color / Bool / texture-transform channels.
//!
//! Per-property animation channel extraction, plus morph-target index
//! resolution.

use super::*;
use crate::blocks::controller::{
    ControlledBlock, NiGeomMorpherController,
    NiMorphData,
};
use crate::blocks::interpolator::{
    NiBSplineBasisData, NiBSplineCompFloatInterpolator,
    NiBSplineCompPoint3Interpolator, NiBSplineData, NiBoolInterpolator, NiColorData,
    NiColorInterpolator, NiFloatData, NiFloatInterpolator, NiPoint3Interpolator, NiPosData,
};
use crate::scene::NifScene;
use std::sync::Arc;

pub fn resolve_morph_target_index(scene: &NifScene, cb: &ControlledBlock) -> Option<u32> {
    let target_name = cb.controller_id.as_deref()?;
    let ctrl_idx = cb.controller_ref.index()?;
    let ctrl = scene.get_as::<NiGeomMorpherController>(ctrl_idx)?;
    let data_idx = ctrl.data_ref.index()?;
    let data = scene.get_as::<NiMorphData>(data_idx)?;
    data.morphs
        .iter()
        .position(|m| {
            m.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(target_name))
        })
        .map(|i| i as u32)
}

/// Extract a float channel from a NiFloatInterpolator → NiFloatData.
pub fn extract_float_channel(
    scene: &NifScene,
    cb: &ControlledBlock,
    target: FloatTarget,
) -> Option<FloatChannel> {
    let interp_idx = cb.interpolator_ref.index()?;
    extract_float_channel_at(scene, interp_idx, target)
}

/// ControlledBlock-free core used by both the KF import path
/// (`extract_float_channel`) and the mesh-embedded controller path
/// (`import_embedded_animations` / #261).
pub fn extract_float_channel_at(
    scene: &NifScene,
    mut interp_idx: usize,
    target: FloatTarget,
) -> Option<FloatChannel> {
    // #334 — follow a NiBlendFloatInterpolator to its dominant sub-
    // interpolator. See `resolve_blend_interpolator_target` for why.
    if let Some(resolved) = resolve_blend_interpolator_target(scene, interp_idx) {
        interp_idx = resolved;
    }
    if let Some(interp) = scene.get_as::<NiFloatInterpolator>(interp_idx) {
        let data_idx = interp.data_ref.index()?;
        let data = scene.get_as::<NiFloatData>(data_idx)?;

        let keys: Vec<AnimFloatKey> = data
            .keys
            .keys
            .iter()
            .map(|k| AnimFloatKey {
                time: k.time,
                value: k.value,
            })
            .collect();

        if keys.is_empty() {
            return None;
        }
        return Some(FloatChannel { target, keys });
    }

    // #936 — compact B-spline scalar channel. Used by Skyrim+ / FO4 KFs
    // for alpha or scale curves paired with NiBSplineCompTransformInterpolator
    // on the same NiControllerSequence. Sample at BSPLINE_SAMPLE_HZ and
    // emit linearly-interpolated keys — same pattern as the transform
    // path in `extract_transform_channel_bspline`.
    if let Some(interp) = scene.get_as::<NiBSplineCompFloatInterpolator>(interp_idx) {
        return extract_float_channel_bspline(scene, interp, target);
    }

    None
}

/// Resolve a `NiFlipController.sources` BlockRef list into source
/// texture filenames — used by the embedded and KF import paths to
/// freeze the per-frame texture roster at clip-load time. Refs that
/// fail to resolve, or that point at a `NiSourceTexture` without an
/// external filename (embedded `NiPixelData`), are silently skipped.
/// The returned ordering matches the source list so frame indices
/// stay aligned with the original NIF.
pub fn resolve_flip_source_paths(
    scene: &NifScene,
    sources: &[crate::types::BlockRef],
) -> Vec<Arc<str>> {
    let mut out = Vec::with_capacity(sources.len());
    for src_ref in sources {
        let Some(src_idx) = src_ref.index() else {
            continue;
        };
        let Some(tex) = scene.get_as::<crate::blocks::texture::NiSourceTexture>(src_idx) else {
            continue;
        };
        if let Some(name) = &tex.filename {
            out.push(Arc::clone(name));
        }
    }
    out
}

/// Resolve the interpolator referenced by a controlled block into a flat
/// list of RGB keys, trying both historical shapes:
///   1. `NiColorInterpolator` → `NiColorData` (nif.xml canonical form for
///      `BSEffect/BSLightingShaderPropertyColorController`, and the form
///      used whenever the authoring tool emits a dedicated color
///      interpolator — #431).
///   2. `NiPoint3Interpolator` → `NiPosData` (legacy
///      `NiMaterialColorController` authored with a Point3 interp
///      because NiColorInterpolator wasn't in the dispatch table before
///      #431; keys already read as RGB).
///
/// Returns an empty Vec when the interpolator lands on neither. Alpha
/// is dropped here to match the downstream `AnimColorKey` shape
/// (`value: [f32; 3]`) — color animations on alpha channels were not
/// supported pre-#431 either and callers that need alpha should drive
/// a separate `NiAlphaController` float channel.
pub fn resolve_color_keys(scene: &NifScene, cb: &ControlledBlock) -> Vec<AnimColorKey> {
    let Some(interp_idx) = cb.interpolator_ref.index() else {
        return Vec::new();
    };
    resolve_color_keys_at(scene, interp_idx)
}

/// ControlledBlock-free core of [`resolve_color_keys`]. Reused by the
/// mesh-embedded controller path (#261).
pub fn resolve_color_keys_at(scene: &NifScene, mut interp_idx: usize) -> Vec<AnimColorKey> {
    // #334 — follow NiBlendPoint3Interpolator. See resolver docs.
    if let Some(resolved) = resolve_blend_interpolator_target(scene, interp_idx) {
        interp_idx = resolved;
    }

    // Path 1: NiColorInterpolator → NiColorData (canonical).
    if let Some(interp) = scene.get_as::<NiColorInterpolator>(interp_idx) {
        if let Some(data_idx) = interp.data_ref.index() {
            if let Some(data) = scene.get_as::<NiColorData>(data_idx) {
                return data
                    .keys
                    .keys
                    .iter()
                    .map(|k| AnimColorKey {
                        time: k.time,
                        value: [k.value[0], k.value[1], k.value[2]],
                    })
                    .collect();
            }
        }
        return Vec::new();
    }

    // Path 2: NiPoint3Interpolator → NiPosData (legacy fallback).
    if let Some(interp) = scene.get_as::<NiPoint3Interpolator>(interp_idx) {
        if let Some(data_idx) = interp.data_ref.index() {
            if let Some(data) = scene.get_as::<NiPosData>(data_idx) {
                return data
                    .keys
                    .keys
                    .iter()
                    .map(|k| AnimColorKey {
                        time: k.time,
                        value: k.value,
                    })
                    .collect();
            }
        }
    }

    // Path 3 — #936 / NIF-D5-NEW-01. NiBSplineCompPoint3Interpolator
    // surfaces a Vec3-stride compact spline channel that vanilla KFs
    // pair with NiBSplineCompTransformInterpolator for color or
    // translation curves. The channel emitter samples at
    // BSPLINE_SAMPLE_HZ — same recipe as the transform path. Pre-fix
    // these were stripped at dispatch time, so any KF whose color
    // controller landed on the compact-Point3 variant silently dropped
    // its keys.
    if let Some(interp) = scene.get_as::<NiBSplineCompPoint3Interpolator>(interp_idx) {
        return sample_color_keys_bspline_point3(scene, interp);
    }

    Vec::new()
}

/// Sample a `NiBSplineCompPoint3Interpolator` into a flat
/// `Vec<AnimColorKey>` at `BSPLINE_SAMPLE_HZ`. Returns an empty Vec when
/// the spline data is missing or under-defined; the caller treats that
/// as "no animation" and leaves the channel at its static value. See #936.
pub fn sample_color_keys_bspline_point3(
    scene: &NifScene,
    interp: &NiBSplineCompPoint3Interpolator,
) -> Vec<AnimColorKey> {
    // Single-key static fallback. FLT_MAX-encoded axes mean "no static
    // pose for this axis" — emit nothing if any axis is sentinel-valued.
    let static_fallback = || -> Vec<AnimColorKey> {
        if is_flt_max(interp.value[0]) || is_flt_max(interp.value[1]) || is_flt_max(interp.value[2])
        {
            return Vec::new();
        }
        vec![AnimColorKey {
            time: interp.start_time,
            value: interp.value,
        }]
    };

    let (Some(basis_idx), Some(data_idx)) = (
        interp.basis_data_ref.index(),
        interp.spline_data_ref.index(),
    ) else {
        return static_fallback();
    };
    let (Some(basis), Some(data)) = (
        scene.get_as::<NiBSplineBasisData>(basis_idx),
        scene.get_as::<NiBSplineData>(data_idx),
    ) else {
        return static_fallback();
    };

    let n_cp = basis.num_control_points as usize;
    if n_cp < BSPLINE_DEGREE + 1 {
        return static_fallback();
    }

    let Some(cps) = channel_slice(
        interp.handle,
        &data.compact_control_points,
        n_cp,
        3, // Vec3 — stride 3
        interp.position_offset,
        interp.position_half_range,
    ) else {
        return static_fallback();
    };

    let duration = (interp.stop_time - interp.start_time).max(0.0);
    let n_samples_f = (duration * BSPLINE_SAMPLE_HZ).ceil();
    let n_samples = (n_samples_f as usize).max(2).min(1_000_000);
    let u_max = (n_cp - BSPLINE_DEGREE) as f32;

    let mut keys = Vec::with_capacity(n_samples);
    for i in 0..n_samples {
        let t = if n_samples > 1 {
            interp.start_time + duration * (i as f32 / (n_samples - 1) as f32)
        } else {
            interp.start_time
        };
        let u = if duration > f32::EPSILON {
            ((t - interp.start_time) / duration) * u_max
        } else {
            0.0
        };
        let p = deboor_cubic(&cps, n_cp, 3, u);
        keys.push(AnimColorKey {
            time: t,
            value: [p[0], p[1], p[2]],
        });
    }
    keys
}

/// Extract a color channel from a material-color controller interpolator
/// chain. Used by `NiMaterialColorController`. Accepts both color-
/// interpolator shapes via [`resolve_color_keys`].
pub fn extract_color_channel(scene: &NifScene, cb: &ControlledBlock) -> Option<ColorChannel> {
    let keys = resolve_color_keys(scene, cb);
    if keys.is_empty() {
        return None;
    }

    // Determine which material color slot from the controller.
    // NiMaterialColorController.target_color:
    // 0=diffuse, 1=ambient, 2=specular, 3=emissive. Default to Diffuse
    // when the controller isn't resolvable (most common target anyway).
    let target = cb
        .controller_ref
        .index()
        .and_then(|idx| scene.get_as::<crate::blocks::controller::NiMaterialColorController>(idx))
        .map(|ctrl| match ctrl.target_color {
            1 => ColorTarget::Ambient,
            2 => ColorTarget::Specular,
            3 => ColorTarget::Emissive,
            _ => ColorTarget::Diffuse,
        })
        .unwrap_or(ColorTarget::Diffuse);

    Some(ColorChannel { target, keys })
}

/// Extract a shader color channel from a
/// `BSEffect/BSLightingShaderPropertyColorController` interpolator
/// chain. Same resolver as the material-color path — targets
/// `ColorTarget::ShaderColor` unconditionally.
pub fn extract_shader_color_channel(scene: &NifScene, cb: &ControlledBlock) -> Option<ColorChannel> {
    let keys = resolve_color_keys(scene, cb);
    if keys.is_empty() {
        return None;
    }
    Some(ColorChannel {
        target: ColorTarget::ShaderColor,
        keys,
    })
}

/// Extract a bool (visibility) channel from NiBoolInterpolator.
pub fn extract_bool_channel(scene: &NifScene, cb: &ControlledBlock) -> Option<BoolChannel> {
    let interp_idx = cb.interpolator_ref.index()?;
    extract_bool_channel_at(scene, interp_idx)
}

/// ControlledBlock-free core of [`extract_bool_channel`]. Reused by
/// the mesh-embedded controller path (#261).
pub fn extract_bool_channel_at(scene: &NifScene, mut interp_idx: usize) -> Option<BoolChannel> {
    // #334 — follow NiBlendBoolInterpolator. See resolver docs.
    if let Some(resolved) = resolve_blend_interpolator_target(scene, interp_idx) {
        interp_idx = resolved;
    }
    let interp = scene.get_as::<NiBoolInterpolator>(interp_idx)?;

    // NiBoolInterpolator may have inline data or reference NiBoolData.
    // For simple vis controllers, the interpolator itself has the value.
    // If it references data, extract keys from there.
    if let Some(data_idx) = interp.data_ref.index() {
        if let Some(data) = scene.get_as::<crate::blocks::interpolator::NiBoolData>(data_idx) {
            let keys: Vec<AnimBoolKey> = data
                .keys
                .keys
                .iter()
                .map(|k| AnimBoolKey {
                    time: k.time,
                    value: k.value > 0.5,
                })
                .collect();
            if !keys.is_empty() {
                return Some(BoolChannel { keys });
            }
        }
    }

    // Fallback: single constant value from the interpolator.
    Some(BoolChannel {
        keys: vec![AnimBoolKey {
            time: 0.0,
            value: interp.value,
        }],
    })
}

/// Extract a texture transform float channel.
/// Maps NiTextureTransformController.operation to the appropriate FloatTarget.
pub fn extract_texture_transform_channel(
    scene: &NifScene,
    cb: &ControlledBlock,
) -> Option<FloatChannel> {
    // Determine target from the controller's operation field.
    let target = cb
        .controller_ref
        .index()
        .and_then(|idx| {
            scene.get_as::<crate::blocks::controller::NiTextureTransformController>(idx)
        })
        .map(|ctrl| match ctrl.operation {
            0 => FloatTarget::UvOffsetU,
            1 => FloatTarget::UvOffsetV,
            2 => FloatTarget::UvScaleU,
            3 => FloatTarget::UvScaleV,
            4 => FloatTarget::UvRotation,
            _ => FloatTarget::UvOffsetU,
        })
        .unwrap_or(FloatTarget::UvOffsetU);

    extract_float_channel(scene, cb, target)
}

/// Bethesda's KF authoring tool stores `±FLT_MAX` (≈3.4028235e38) in
/// `NiTransformInterpolator::transform` (the static pose-value triple
/// of translation / rotation / scale) for channels whose B-spline
/// payload omits that TRS axis — the runtime is meant to fall through
/// to the bone's bind-pose for the absent axis. Same FLT_MAX-as-no-
/// value convention as BSShaderPPLighting's rimlight gate
/// ([shader.rs:977-978]); threshold sits below the literal so float-
/// precision noise on the authoring round-trip doesn't slip through.
/// Without this gate the B-spline fallback path materialises FLT_MAX
/// as a real frame-0 key, the animation system writes it to the
/// bone's `Transform.translation`, and the skinning matrix flies to
/// infinity — NPCs vanish on first tick (#772 / FO3 TestQAHairM 31→0;
/// FNV Doc Mitchell finger bones).
pub const FLT_MAX_SENTINEL: f32 = 3.0e38;
