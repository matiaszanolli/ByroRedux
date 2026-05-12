//! Engine-side animation data types produced by the NIF→clip importer.
//!
//! `AnimationClip` is the top-level shape — a named bundle of per-bone
//! transform channels plus optional float / color / bool / texture-flip
//! channels for shader / morph / visibility animation. Decoupled from
//! the NIF block graph so the runtime animation system doesn't care
//! whether the source was a KF file, an embedded NiControllerSequence,
//! or a synthetic clip built in tests.

use crate::blocks::interpolator::KeyType;
use std::collections::HashMap;
use std::sync::Arc;

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
    /// Per-channel priority from ControlledBlock (0 = lowest).
    pub priority: u8,
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

/// What a float animation channel targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatTarget {
    /// Material alpha (NiAlphaController).
    Alpha,
    /// UV offset U (NiTextureTransformController, operation=0).
    UvOffsetU,
    /// UV offset V (operation=1).
    UvOffsetV,
    /// UV scale U (operation=2).
    UvScaleU,
    /// UV scale V (operation=3).
    UvScaleV,
    /// UV rotation (operation=4).
    UvRotation,
    /// Shader float property (BSEffectShader/BSLightingShader float controllers).
    ShaderFloat,
    /// Morph target weight (NiGeomMorpherController blend shape).
    MorphWeight(u32),
    /// NiLight dimmer slot (NiLightDimmerController). See #983.
    LightDimmer,
    /// NiLight intensity multiplier (NiLightIntensityController). See #983.
    LightIntensity,
    /// NiLight radius (NiLightRadiusController). See #983.
    LightRadius,
}

/// What a color animation channel targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorTarget {
    /// Diffuse color (NiMaterialColorController, target_color=0).
    Diffuse,
    /// Ambient color (target_color=1).
    Ambient,
    /// Specular color (target_color=2).
    Specular,
    /// Emissive color (target_color=3).
    Emissive,
    /// Shader color property.
    ShaderColor,
    /// NiLight diffuse slot (NiLightColorController, target_color=0). See #983.
    LightDiffuse,
    /// NiLight ambient slot (NiLightColorController, target_color=1). See #983.
    LightAmbient,
}

/// A float keyframe for non-transform channels.
#[derive(Debug, Clone, Copy)]
pub struct AnimFloatKey {
    pub time: f32,
    pub value: f32,
}

/// A color keyframe (RGB).
#[derive(Debug, Clone, Copy)]
pub struct AnimColorKey {
    pub time: f32,
    pub value: [f32; 3],
}

/// A bool keyframe (visibility).
#[derive(Debug, Clone, Copy)]
pub struct AnimBoolKey {
    pub time: f32,
    pub value: bool,
}

/// A float animation channel (alpha, UV params, shader floats).
#[derive(Debug, Clone)]
pub struct FloatChannel {
    pub target: FloatTarget,
    pub keys: Vec<AnimFloatKey>,
}

/// A color animation channel (material/shader colors).
#[derive(Debug, Clone)]
pub struct ColorChannel {
    pub target: ColorTarget,
    pub keys: Vec<AnimColorKey>,
}

/// A visibility animation channel.
#[derive(Debug, Clone)]
pub struct BoolChannel {
    pub keys: Vec<AnimBoolKey>,
}

/// Texture-flipbook animation channel — `NiFlipController` semantics.
///
/// Source paths are resolved at clip-load time from
/// `NiFlipController.sources → NiSourceTexture.filename` so the
/// runtime never has to walk back into the NIF scene. The float keys
/// drive a cycle position; the consumer picks
/// `source_paths[floor(value) % source_paths.len()]`. `texture_slot`
/// is the raw `TexType` enum value (0=BASE_MAP, 4=GLOW_MAP, …) — the
/// runtime decides which material slot it routes to.
#[derive(Debug, Clone)]
pub struct TextureFlipChannel {
    pub texture_slot: u32,
    pub source_paths: Vec<Arc<str>>,
    pub keys: Vec<AnimFloatKey>,
}

/// A complete animation clip extracted from one NiControllerSequence.
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
    /// Map from node name to its transform animation channel. `Arc<str>`
    /// avoids per-channel allocation — parser holds names as `Arc<str>`. #244.
    pub channels: HashMap<Arc<str>, TransformChannel>,
    /// Float channels keyed by (node_name, target).
    pub float_channels: Vec<(Arc<str>, FloatChannel)>,
    /// Color channels keyed by (node_name, target).
    pub color_channels: Vec<(Arc<str>, ColorChannel)>,
    /// Bool (visibility) channels keyed by node_name.
    pub bool_channels: Vec<(Arc<str>, BoolChannel)>,
    /// Texture-flipbook channels keyed by node_name. Captured from
    /// `NiFlipController` blocks during NIF import. See
    /// `TextureFlipChannel`. #545.
    pub texture_flip_channels: Vec<(Arc<str>, TextureFlipChannel)>,
    /// Text key events: (time, label). Imported from NiTextKeyExtraData.
    /// Emitted as transient ECS markers when crossed during playback.
    pub text_keys: Vec<(f32, String)>,
}
