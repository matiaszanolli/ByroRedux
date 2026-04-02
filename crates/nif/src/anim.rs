//! Animation clip import from NIF/KF files.
//!
//! Converts NiControllerSequence blocks (with their referenced interpolators
//! and keyframe data) into engine-friendly `AnimationClip` structures that
//! are decoupled from the NIF block graph.

use crate::blocks::controller::{ControlledBlock, NiControllerSequence};
use crate::blocks::interpolator::{
    FloatKey, KeyGroup, KeyType, NiTransformData, NiTransformInterpolator, Vec3Key,
};
use crate::scene::NifScene;
use std::collections::HashMap;

// ── Public animation types ────────────────────────────────────────────

/// How the animation behaves when it reaches its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleType {
    /// Stop at the last frame.
    Clamp,
    /// Loop back to the start.
    Loop,
    /// Play forward then backward (ping-pong).
    Reverse,
}

impl CycleType {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Clamp,
            1 => Self::Loop,
            2 => Self::Reverse,
            _ => Self::Clamp,
        }
    }
}

/// A single channel of transform animation for one named node.
#[derive(Debug, Clone)]
pub struct TransformChannel {
    pub translation_keys: Vec<TranslationKey>,
    pub translation_type: KeyType,
    pub rotation_keys: Vec<RotationKey>,
    pub rotation_type: KeyType,
    pub scale_keys: Vec<ScaleKey>,
    pub scale_type: KeyType,
}

/// Translation keyframe in engine space (already converted from Z-up to Y-up).
#[derive(Debug, Clone, Copy)]
pub struct TranslationKey {
    pub time: f32,
    pub value: [f32; 3],
    pub forward: [f32; 3],
    pub backward: [f32; 3],
    pub tbc: Option<[f32; 3]>,
}

/// Rotation keyframe — quaternion (x, y, z, w) in glam convention,
/// already converted from Gamebryo's (w, x, y, z) and Z-up to Y-up.
#[derive(Debug, Clone, Copy)]
pub struct RotationKey {
    pub time: f32,
    pub value: [f32; 4], // x, y, z, w (glam order)
    pub tbc: Option<[f32; 3]>,
}

/// Scale keyframe.
#[derive(Debug, Clone, Copy)]
pub struct ScaleKey {
    pub time: f32,
    pub value: f32,
    pub forward: f32,
    pub backward: f32,
    pub tbc: Option<[f32; 3]>,
}

/// A complete animation clip extracted from one NiControllerSequence.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub cycle_type: CycleType,
    pub frequency: f32,
    /// Map from node name to its transform animation channel.
    pub channels: HashMap<String, TransformChannel>,
}

// ── Coordinate conversion helpers ─────────────────────────────────────

/// Convert a position from Gamebryo Z-up to Y-up: (x, y, z) → (x, z, -y).
fn zup_to_yup_pos(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[2], -v[1]]
}

/// Convert a quaternion from Gamebryo (w,x,y,z) Z-up to glam (x,y,z,w) Y-up.
/// The coordinate change quaternion is a 90° rotation around X:
/// q_conv = (sin(45°), 0, 0, cos(45°)) = (√2/2, 0, 0, √2/2)
/// q_yup = q_conv * q_zup * q_conv_inv
/// Simplified: swap y↔z and negate new z for the vector part.
fn zup_to_yup_quat(wxyz: [f32; 4]) -> [f32; 4] {
    let [w, x, y, z] = wxyz;
    // Z-up to Y-up: (w, x, y, z) → (w, x, z, -y), then reorder to glam (x, y, z, w)
    [x, z, -y, w]
}

// ── Import function ───────────────────────────────────────────────────

/// Import all animation clips from a parsed NIF/KF scene.
///
/// Finds all `NiControllerSequence` blocks, follows their interpolator
/// and data references, and produces `AnimationClip` instances.
pub fn import_kf(scene: &NifScene) -> Vec<AnimationClip> {
    let mut clips = Vec::new();

    for block in &scene.blocks {
        let Some(seq) = block.as_any().downcast_ref::<NiControllerSequence>() else {
            continue;
        };

        let clip = import_sequence(scene, seq);
        if !clip.channels.is_empty() {
            clips.push(clip);
        }
    }

    clips
}

fn import_sequence(scene: &NifScene, seq: &NiControllerSequence) -> AnimationClip {
    let name = seq.name.clone().unwrap_or_else(|| "unnamed".to_string());
    let duration = seq.stop_time - seq.start_time;
    let cycle_type = CycleType::from_u32(seq.cycle_type);
    let frequency = seq.frequency;
    let mut channels = HashMap::new();

    for cb in &seq.controlled_blocks {
        // Only handle NiTransformController channels for M21
        let controller_type = cb.controller_type.as_deref().unwrap_or("");
        if controller_type != "NiTransformController" {
            continue;
        }

        let Some(node_name) = cb.node_name.as_ref() else {
            continue;
        };

        if let Some(channel) = extract_transform_channel(scene, cb) {
            channels.insert(node_name.clone(), channel);
        }
    }

    AnimationClip {
        name,
        duration,
        cycle_type,
        frequency,
        channels,
    }
}

fn extract_transform_channel(
    scene: &NifScene,
    cb: &ControlledBlock,
) -> Option<TransformChannel> {
    let interp_idx = cb.interpolator_ref.index()?;
    let interp = scene.get_as::<NiTransformInterpolator>(interp_idx)?;
    let data_idx = interp.data_ref.index()?;
    let data = scene.get_as::<NiTransformData>(data_idx)?;

    let (translation_keys, translation_type) = convert_vec3_keys(&data.translations);
    let (rotation_keys, rotation_type) = convert_quat_keys(data);
    let (scale_keys, scale_type) = convert_float_keys(&data.scales);

    Some(TransformChannel {
        translation_keys,
        translation_type,
        rotation_keys,
        rotation_type,
        scale_keys,
        scale_type,
    })
}

fn convert_vec3_keys(group: &KeyGroup<Vec3Key>) -> (Vec<TranslationKey>, KeyType) {
    let keys = group
        .keys
        .iter()
        .map(|k| TranslationKey {
            time: k.time,
            value: zup_to_yup_pos(k.value),
            forward: zup_to_yup_pos(k.tangent_forward),
            backward: zup_to_yup_pos(k.tangent_backward),
            tbc: k.tbc,
        })
        .collect();
    (keys, group.key_type)
}

fn convert_quat_keys(data: &NiTransformData) -> (Vec<RotationKey>, KeyType) {
    let rotation_type = data.rotation_type.unwrap_or(KeyType::Linear);

    // XYZ euler rotations not supported yet — would need euler→quat conversion
    if rotation_type == KeyType::XyzRotation {
        log::warn!("XYZ euler rotation keys not yet supported, skipping rotation channel");
        return (Vec::new(), KeyType::Linear);
    }

    let keys = data
        .rotation_keys
        .iter()
        .map(|k| RotationKey {
            time: k.time,
            value: zup_to_yup_quat(k.value),
            tbc: k.tbc,
        })
        .collect();
    (keys, rotation_type)
}

fn convert_float_keys(group: &KeyGroup<FloatKey>) -> (Vec<ScaleKey>, KeyType) {
    let keys = group
        .keys
        .iter()
        .map(|k| ScaleKey {
            time: k.time,
            value: k.value,
            forward: k.tangent_forward,
            backward: k.tangent_backward,
            tbc: k.tbc,
        })
        .collect();
    (keys, group.key_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_type_from_u32() {
        assert_eq!(CycleType::from_u32(0), CycleType::Clamp);
        assert_eq!(CycleType::from_u32(1), CycleType::Loop);
        assert_eq!(CycleType::from_u32(2), CycleType::Reverse);
        assert_eq!(CycleType::from_u32(99), CycleType::Clamp);
    }

    #[test]
    fn zup_to_yup_position() {
        // Gamebryo Z-up (1, 2, 3) → Y-up (1, 3, -2)
        let result = zup_to_yup_pos([1.0, 2.0, 3.0]);
        assert_eq!(result, [1.0, 3.0, -2.0]);
    }

    #[test]
    fn zup_to_yup_identity_quat() {
        // Gamebryo identity (w=1, x=0, y=0, z=0) → glam (x=0, y=0, z=0, w=1)
        let result = zup_to_yup_quat([1.0, 0.0, 0.0, 0.0]);
        assert_eq!(result, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn empty_scene_produces_no_clips() {
        let scene = NifScene {
            blocks: Vec::new(),
            root_index: None,
        };
        let clips = import_kf(&scene);
        assert!(clips.is_empty());
    }
}
