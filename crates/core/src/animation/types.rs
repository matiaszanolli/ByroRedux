//! Animation data types: keyframes, channels, clips.

use crate::math::{Quat, Vec3};
use std::collections::HashMap;

/// How the animation behaves when it reaches its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleType {
    Clamp,
    Loop,
    Reverse,
}

/// Interpolation type for keyframe data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Linear,
    Quadratic,
    Tbc,
}

/// Translation keyframe.
#[derive(Debug, Clone, Copy)]
pub struct TranslationKey {
    pub time: f32,
    pub value: Vec3,
    pub forward: Vec3,
    pub backward: Vec3,
    pub tbc: Option<[f32; 3]>,
}

/// Rotation keyframe (quaternion).
#[derive(Debug, Clone, Copy)]
pub struct RotationKey {
    pub time: f32,
    pub value: Quat,
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

/// A single channel of transform animation for one named node.
#[derive(Debug, Clone)]
pub struct TransformChannel {
    pub translation_keys: Vec<TranslationKey>,
    pub translation_type: KeyType,
    pub rotation_keys: Vec<RotationKey>,
    pub rotation_type: KeyType,
    pub scale_keys: Vec<ScaleKey>,
    pub scale_type: KeyType,
    /// Per-channel priority from ControlledBlock (0 = lowest, 255 = highest).
    pub priority: u8,
}

// ── Non-transform channel types ───────────────────────────────────────

/// What a float channel targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatTarget {
    Alpha,
    UvOffsetU,
    UvOffsetV,
    UvScaleU,
    UvScaleV,
    UvRotation,
    ShaderFloat,
    /// Morph target weight (blend shape). The u32 is the morph index
    /// into the NiGeomMorpherController's target list.
    MorphWeight(u32),
}

/// What a color channel targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorTarget {
    Diffuse,
    Ambient,
    Specular,
    Emissive,
    ShaderColor,
}

/// A float keyframe (alpha, UV params, shader floats).
#[derive(Debug, Clone, Copy)]
pub struct AnimFloatKey {
    pub time: f32,
    pub value: f32,
}

/// A color keyframe (RGB).
#[derive(Debug, Clone, Copy)]
pub struct AnimColorKey {
    pub time: f32,
    pub value: Vec3,
}

/// A bool keyframe (visibility).
#[derive(Debug, Clone, Copy)]
pub struct AnimBoolKey {
    pub time: f32,
    pub value: bool,
}

/// Float animation channel.
#[derive(Debug, Clone)]
pub struct FloatChannel {
    pub target: FloatTarget,
    pub keys: Vec<AnimFloatKey>,
}

/// Color animation channel.
#[derive(Debug, Clone)]
pub struct ColorChannel {
    pub target: ColorTarget,
    pub keys: Vec<AnimColorKey>,
}

/// Visibility animation channel.
#[derive(Debug, Clone)]
pub struct BoolChannel {
    pub keys: Vec<AnimBoolKey>,
}

/// A complete animation clip (one per NiControllerSequence).
#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub cycle_type: CycleType,
    pub frequency: f32,
    /// Default weight from NiControllerSequence (0.0–1.0).
    pub weight: f32,
    /// Accumulation root node name — horizontal translation on this node
    /// is extracted as root motion delta rather than applied as animation.
    pub accum_root_name: Option<String>,
    /// Map from node name to its transform animation channel.
    pub channels: HashMap<String, TransformChannel>,
    /// Float channels: (node_name, channel).
    pub float_channels: Vec<(String, FloatChannel)>,
    /// Color channels: (node_name, channel).
    pub color_channels: Vec<(String, ColorChannel)>,
    /// Bool channels: (node_name, channel).
    pub bool_channels: Vec<(String, BoolChannel)>,
    /// Text key events: (time, label). Imported from NiTextKeyExtraData.
    /// Emitted as transient ECS markers when crossed during playback.
    pub text_keys: Vec<(f32, String)>,
}
