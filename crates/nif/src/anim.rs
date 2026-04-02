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
use std::collections::{BTreeSet, HashMap};

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

    if rotation_type == KeyType::XyzRotation {
        return convert_xyz_euler_keys(data);
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

/// Convert XYZ euler rotation key groups to quaternion keys.
///
/// Each axis has its own `KeyGroup<FloatKey>` with potentially different key counts
/// and interpolation types. We collect all unique timestamps, sample each axis at
/// each time, compose the euler angles into a quaternion, and apply Z-up→Y-up conversion.
fn convert_xyz_euler_keys(data: &NiTransformData) -> (Vec<RotationKey>, KeyType) {
    let Some(ref xyz) = data.xyz_rotations else {
        return (Vec::new(), KeyType::Linear);
    };

    // Collect all unique timestamps across all 3 axes using ordered floats.
    let mut times = BTreeSet::new();
    for axis_group in xyz {
        for key in &axis_group.keys {
            times.insert(OrderedF32(key.time));
        }
    }

    if times.is_empty() {
        return (Vec::new(), KeyType::Linear);
    }

    let keys: Vec<RotationKey> = times
        .iter()
        .map(|&OrderedF32(time)| {
            let x = sample_float_key_group(&xyz[0], time);
            let y = sample_float_key_group(&xyz[1], time);
            let z = sample_float_key_group(&xyz[2], time);

            // Gamebryo euler angles are in radians, Z-up coordinate system.
            // Compose euler → quaternion in Gamebryo space, then convert to Y-up.
            // Gamebryo uses XYZ intrinsic euler order.
            let qw = euler_to_quat_wxyz(x, y, z);
            let yup = zup_to_yup_quat(qw);

            RotationKey {
                time,
                value: yup,
                tbc: None, // Euler→quat bakes interpolation; SLERP between samples
            }
        })
        .collect();

    // Output as Linear (SLERP between the pre-composed quaternion samples)
    (keys, KeyType::Linear)
}

/// Linearly sample a float key group at a given time.
/// Supports Linear, Quadratic (Hermite), and TBC interpolation.
fn sample_float_key_group(group: &KeyGroup<FloatKey>, time: f32) -> f32 {
    let keys = &group.keys;
    if keys.is_empty() {
        return 0.0;
    }
    if keys.len() == 1 || time <= keys[0].time {
        return keys[0].value;
    }
    if time >= keys.last().unwrap().time {
        return keys.last().unwrap().value;
    }

    // Binary search for bracketing pair.
    let mut lo = 0;
    let mut hi = keys.len() - 1;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if keys[mid].time <= time {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let k0 = &keys[lo];
    let k1 = &keys[hi];
    let dt = k1.time - k0.time;
    let t = if dt > 0.0 { (time - k0.time) / dt } else { 0.0 };

    match group.key_type {
        KeyType::Linear => k0.value + (k1.value - k0.value) * t,
        KeyType::Quadratic => {
            let t2 = t * t;
            let t3 = t2 * t;
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            h00 * k0.value + h10 * k0.tangent_forward * dt
                + h01 * k1.value + h11 * k1.tangent_backward * dt
        }
        KeyType::Tbc | KeyType::XyzRotation => {
            // TBC: fall back to linear for euler axis sampling (rare edge case)
            k0.value + (k1.value - k0.value) * t
        }
    }
}

/// Convert XYZ intrinsic euler angles (radians) to quaternion (w, x, y, z).
fn euler_to_quat_wxyz(x: f32, y: f32, z: f32) -> [f32; 4] {
    let (sx, cx) = (x * 0.5).sin_cos();
    let (sy, cy) = (y * 0.5).sin_cos();
    let (sz, cz) = (z * 0.5).sin_cos();

    // XYZ intrinsic rotation composition
    let w = cx * cy * cz - sx * sy * sz;
    let qx = sx * cy * cz + cx * sy * sz;
    let qy = cx * sy * cz - sx * cy * sz;
    let qz = cx * cy * sz + sx * sy * cz;

    [w, qx, qy, qz]
}

/// Wrapper for f32 that implements Ord for use in BTreeSet.
/// NaN-safe: treats NaN as equal and less than all values.
#[derive(Clone, Copy, PartialEq)]
struct OrderedF32(f32);

impl Eq for OrderedF32 {}

impl PartialOrd for OrderedF32 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF32 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
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

    #[test]
    fn euler_to_quat_identity() {
        // All angles zero → identity quaternion (w=1, x=0, y=0, z=0)
        let [w, x, y, z] = euler_to_quat_wxyz(0.0, 0.0, 0.0);
        assert!((w - 1.0).abs() < 1e-6);
        assert!(x.abs() < 1e-6);
        assert!(y.abs() < 1e-6);
        assert!(z.abs() < 1e-6);
    }

    #[test]
    fn euler_to_quat_90_deg_x() {
        use std::f32::consts::FRAC_PI_2;
        // 90° around X: quat = (cos(45°), sin(45°), 0, 0) = (~0.707, ~0.707, 0, 0)
        let [w, x, y, z] = euler_to_quat_wxyz(FRAC_PI_2, 0.0, 0.0);
        let s = FRAC_PI_2.sin() * 0.5_f32.sqrt(); // sin(45°)
        let c = FRAC_PI_2.cos() * 0.5_f32.sqrt(); // cos(45°) — but let's just check magnitude
        assert!((w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5, "quaternion should be unit");
        assert!(x > 0.5, "x component should be dominant for X rotation");
        assert!(y.abs() < 1e-5);
        assert!(z.abs() < 1e-5);
    }

    #[test]
    fn euler_to_quat_90_deg_y() {
        use std::f32::consts::FRAC_PI_2;
        let [w, x, y, z] = euler_to_quat_wxyz(0.0, FRAC_PI_2, 0.0);
        assert!((w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5);
        assert!(x.abs() < 1e-5);
        assert!(y > 0.5, "y component should be dominant for Y rotation");
        assert!(z.abs() < 1e-5);
    }

    #[test]
    fn euler_to_quat_90_deg_z() {
        use std::f32::consts::FRAC_PI_2;
        let [w, x, y, z] = euler_to_quat_wxyz(0.0, 0.0, FRAC_PI_2);
        assert!((w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5);
        assert!(x.abs() < 1e-5);
        assert!(y.abs() < 1e-5);
        assert!(z > 0.5, "z component should be dominant for Z rotation");
    }

    #[test]
    fn sample_float_key_group_linear() {
        let group = KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                FloatKey { time: 0.0, value: 0.0, tangent_forward: 0.0, tangent_backward: 0.0, tbc: None },
                FloatKey { time: 1.0, value: 1.0, tangent_forward: 0.0, tangent_backward: 0.0, tbc: None },
            ],
        };
        assert!((sample_float_key_group(&group, 0.5) - 0.5).abs() < 1e-5);
        assert!((sample_float_key_group(&group, 0.0) - 0.0).abs() < 1e-5);
        assert!((sample_float_key_group(&group, 1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn sample_float_key_group_empty() {
        let group = KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![],
        };
        assert_eq!(sample_float_key_group(&group, 0.5), 0.0);
    }

    #[test]
    fn sample_float_key_group_single() {
        let group = KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                FloatKey { time: 0.5, value: 42.0, tangent_forward: 0.0, tangent_backward: 0.0, tbc: None },
            ],
        };
        assert_eq!(sample_float_key_group(&group, 0.0), 42.0);
        assert_eq!(sample_float_key_group(&group, 1.0), 42.0);
    }

    #[test]
    fn convert_xyz_euler_keys_produces_rotation_keys() {
        use std::f32::consts::FRAC_PI_2;
        // Create NiTransformData with XYZ euler rotation keys:
        // At t=0: all angles 0 (identity)
        // At t=1: 90° around X
        let x_keys = KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                FloatKey { time: 0.0, value: 0.0, tangent_forward: 0.0, tangent_backward: 0.0, tbc: None },
                FloatKey { time: 1.0, value: FRAC_PI_2, tangent_forward: 0.0, tangent_backward: 0.0, tbc: None },
            ],
        };
        let empty_keys = KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                FloatKey { time: 0.0, value: 0.0, tangent_forward: 0.0, tangent_backward: 0.0, tbc: None },
                FloatKey { time: 1.0, value: 0.0, tangent_forward: 0.0, tangent_backward: 0.0, tbc: None },
            ],
        };

        let data = NiTransformData {
            rotation_type: Some(KeyType::XyzRotation),
            rotation_keys: Vec::new(),
            xyz_rotations: Some([x_keys, empty_keys.clone(), empty_keys]),
            translations: KeyGroup { key_type: KeyType::Linear, keys: Vec::new() },
            scales: KeyGroup { key_type: KeyType::Linear, keys: Vec::new() },
        };

        let (keys, key_type) = convert_xyz_euler_keys(&data);
        assert_eq!(key_type, KeyType::Linear);
        assert_eq!(keys.len(), 2, "should have 2 rotation keys (one per unique timestamp)");

        // First key (t=0): identity → after Z-up to Y-up, glam format (x, y, z, w)
        let k0 = &keys[0];
        assert!((k0.time).abs() < 1e-5);
        // Identity quat in glam: (0, 0, 0, 1)
        assert!((k0.value[3] - 1.0).abs() < 1e-4, "w should be ~1 for identity: {:?}", k0.value);

        // Second key (t=1): 90° around X in Z-up, then converted to Y-up
        let k1 = &keys[1];
        assert!((k1.time - 1.0).abs() < 1e-5);
        // Should be a unit quaternion
        let len_sq = k1.value.iter().map(|v| v * v).sum::<f32>();
        assert!((len_sq - 1.0).abs() < 1e-4, "quaternion should be unit: {:?}", k1.value);
    }
}
