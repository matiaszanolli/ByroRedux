//! NIF animation clip conversion and subtree name map building.

use byroredux_core::animation::{
    AnimBoolKey, AnimColorKey, AnimFloatKey, AnimationClip, BoolChannel, ColorChannel, ColorTarget,
    CycleType, FloatChannel, FloatTarget, KeyType, RotationKey, ScaleKey, TransformChannel,
    TranslationKey,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Children, Name, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::FixedString;
use std::collections::HashMap;

/// Build a scoped name→entity map by walking the subtree rooted at `root`.
pub(crate) fn build_subtree_name_map(
    world: &World,
    root: EntityId,
) -> HashMap<FixedString, EntityId> {
    let mut map = HashMap::new();

    // Include the root itself.
    if let Some(nq) = world.query::<Name>() {
        if let Some(name) = nq.get(root) {
            map.insert(name.0, root);
        }
    }

    // BFS through children.
    let children_q = world.query::<Children>();
    let name_q = world.query::<Name>();
    let Some(ref cq) = children_q else { return map };

    let mut queue = vec![root];
    while let Some(entity) = queue.pop() {
        if let Some(children) = cq.get(entity) {
            for &child in &children.0 {
                if let Some(ref nq) = name_q {
                    if let Some(name) = nq.get(child) {
                        map.insert(name.0, child);
                    }
                }
                queue.push(child);
            }
        }
    }

    map
}

/// Convert a NIF animation clip (byroredux_nif types) to a core animation clip (glam types).
pub(crate) fn convert_nif_clip(nif: &byroredux_nif::anim::AnimationClip) -> AnimationClip {
    use byroredux_nif::anim as na;

    let cycle_type = match nif.cycle_type {
        na::CycleType::Clamp => CycleType::Clamp,
        na::CycleType::Loop => CycleType::Loop,
        na::CycleType::Reverse => CycleType::Reverse,
    };

    let channels = nif
        .channels
        .iter()
        .map(|(name, ch)| {
            let convert_key_type = |kt: byroredux_nif::blocks::interpolator::KeyType| match kt {
                byroredux_nif::blocks::interpolator::KeyType::Linear => KeyType::Linear,
                byroredux_nif::blocks::interpolator::KeyType::Quadratic => KeyType::Quadratic,
                byroredux_nif::blocks::interpolator::KeyType::Tbc => KeyType::Tbc,
                byroredux_nif::blocks::interpolator::KeyType::XyzRotation => KeyType::Linear,
                byroredux_nif::blocks::interpolator::KeyType::Constant => KeyType::Linear,
            };

            let translation_keys = ch
                .translation_keys
                .iter()
                .map(|k| TranslationKey {
                    time: k.time,
                    value: Vec3::from_array(k.value),
                    forward: Vec3::from_array(k.forward),
                    backward: Vec3::from_array(k.backward),
                    tbc: k.tbc,
                })
                .collect();

            let rotation_keys = ch
                .rotation_keys
                .iter()
                .map(|k| RotationKey {
                    time: k.time,
                    value: Quat::from_xyzw(k.value[0], k.value[1], k.value[2], k.value[3]),
                    tbc: k.tbc,
                })
                .collect();

            let scale_keys = ch
                .scale_keys
                .iter()
                .map(|k| ScaleKey {
                    time: k.time,
                    value: k.value,
                    forward: k.forward,
                    backward: k.backward,
                    tbc: k.tbc,
                })
                .collect();

            (
                name.clone(),
                TransformChannel {
                    translation_keys,
                    translation_type: convert_key_type(ch.translation_type),
                    rotation_keys,
                    rotation_type: convert_key_type(ch.rotation_type),
                    scale_keys,
                    scale_type: convert_key_type(ch.scale_type),
                    priority: ch.priority,
                },
            )
        })
        .collect();

    let convert_float_target = |t: na::FloatTarget| match t {
        na::FloatTarget::Alpha => FloatTarget::Alpha,
        na::FloatTarget::UvOffsetU => FloatTarget::UvOffsetU,
        na::FloatTarget::UvOffsetV => FloatTarget::UvOffsetV,
        na::FloatTarget::UvScaleU => FloatTarget::UvScaleU,
        na::FloatTarget::UvScaleV => FloatTarget::UvScaleV,
        na::FloatTarget::UvRotation => FloatTarget::UvRotation,
        na::FloatTarget::ShaderFloat => FloatTarget::ShaderFloat,
    };

    let convert_color_target = |t: na::ColorTarget| match t {
        na::ColorTarget::Diffuse => ColorTarget::Diffuse,
        na::ColorTarget::Ambient => ColorTarget::Ambient,
        na::ColorTarget::Specular => ColorTarget::Specular,
        na::ColorTarget::Emissive => ColorTarget::Emissive,
        na::ColorTarget::ShaderColor => ColorTarget::ShaderColor,
    };

    let float_channels = nif
        .float_channels
        .iter()
        .map(|(name, ch)| {
            (
                name.clone(),
                FloatChannel {
                    target: convert_float_target(ch.target),
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimFloatKey {
                            time: k.time,
                            value: k.value,
                        })
                        .collect(),
                },
            )
        })
        .collect();

    let color_channels = nif
        .color_channels
        .iter()
        .map(|(name, ch)| {
            (
                name.clone(),
                ColorChannel {
                    target: convert_color_target(ch.target),
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimColorKey {
                            time: k.time,
                            value: Vec3::from_array(k.value),
                        })
                        .collect(),
                },
            )
        })
        .collect();

    let bool_channels = nif
        .bool_channels
        .iter()
        .map(|(name, ch)| {
            (
                name.clone(),
                BoolChannel {
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimBoolKey {
                            time: k.time,
                            value: k.value,
                        })
                        .collect(),
                },
            )
        })
        .collect();

    AnimationClip {
        name: nif.name.clone(),
        duration: nif.duration,
        cycle_type,
        frequency: nif.frequency,
        weight: nif.weight,
        accum_root_name: nif.accum_root_name.clone(),
        channels,
        float_channels,
        color_channels,
        bool_channels,
        text_keys: nif.text_keys.clone(),
    }
}
