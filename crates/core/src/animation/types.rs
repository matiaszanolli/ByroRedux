//! Animation data types: keyframes, channels, clips.

use crate::math::{Quat, Vec3};
use crate::string::FixedString;
use std::collections::HashMap;
use std::sync::Arc;

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
    /// NiLight `dimmer` slot — multiplicative scalar on the base
    /// color. Animated by `NiLightDimmerController`. See #983.
    LightDimmer,
    /// NiLight intensity multiplier — distinct from `dimmer` (some
    /// content layers flicker on `intensity` while `dimmer` stays
    /// at the authored steady value). Animated by
    /// `NiLightIntensityController`. See #983.
    LightIntensity,
    /// NiLight `radius` slot. Animated by `NiLightRadiusController`.
    /// Drives the renderer's per-light attenuation cutoff in world
    /// units. See #983.
    LightRadius,
}

/// What a color channel targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorTarget {
    Diffuse,
    Ambient,
    Specular,
    Emissive,
    ShaderColor,
    /// NiLight diffuse color slot — animated by
    /// `NiLightColorController` with `target_color == 0`. Replaces
    /// `LightSource.color` for the duration of the clip. See #983.
    LightDiffuse,
    /// NiLight ambient color slot — animated by
    /// `NiLightColorController` with `target_color == 1`. The
    /// renderer doesn't currently consume the ambient slot per-
    /// light (cell ambient drives the unlit fallback), but the
    /// channel is captured so a future ambient-per-light path
    /// finds the data. See #983.
    LightAmbient,
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

/// Texture flipbook animation channel — `NiFlipController` semantics.
///
/// The float-typed `keys` carry the cycle position (the controller's
/// `NiFloatInterpController` interpolator output: typically a 0..N saw
/// or stepped ramp). At sample time the runtime picks the source
/// `source_paths[floor(value) % source_paths.len()]` and rebinds it
/// into the texture slot identified by `texture_slot`. Source paths
/// are resolved at clip-load time from the chain
/// `NiFlipController.sources → NiSourceTexture.filename`, so the
/// runtime never has to walk back into the NIF scene.
///
/// `texture_slot` is the raw `TexType` enum from the controller —
/// 0=BASE_MAP, 1=DARK_MAP, 2=DETAIL_MAP, 3=GLOSS_MAP, 4=GLOW_MAP, etc.
/// (per nif.xml). The renderer consumer is expected to interpret it.
///
/// Renderer integration is deferred — only Oblivion / FO3 / FNV ship
/// `NiFlipController`; Skyrim+ moved to `BSEffectShader` UV scrolling.
#[derive(Debug, Clone)]
pub struct TextureFlipChannel {
    pub texture_slot: u32,
    pub source_paths: Vec<Arc<str>>,
    pub keys: Vec<AnimFloatKey>,
}

/// A complete animation clip (one per NiControllerSequence).
#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub cycle_type: CycleType,
    pub frequency: f32,
    /// Default weight from `NiControllerSequence.weight` (0.0–1.0).
    /// Modulates the layer's `effective_weight()` inside
    /// `sample_blended_transform`: the sequence author can pre-attenuate
    /// a clip so that even at full layer weight it only contributes
    /// `weight` of the blend. Distinct from `AnimationLayer::weight`,
    /// which is the runtime crossfade factor. See #469.
    ///
    /// Single-clip playback (`AnimationPlayer` / `advance_time`) does
    /// not apply `clip.weight` — it's a blend scaler, meaningful only
    /// when more than one clip contributes at the same priority.
    pub weight: f32,
    /// Accumulation root node name — horizontal translation on this node
    /// is extracted as root motion delta rather than applied as animation.
    pub accum_root_name: Option<FixedString>,
    /// Map from interned node name to its transform animation channel.
    /// Keys are `FixedString` symbols — pre-interned at clip load time so
    /// the animation hot path does zero-allocation lookups. See #340.
    pub channels: HashMap<FixedString, TransformChannel>,
    /// Float channels: (node_name, channel).
    pub float_channels: Vec<(FixedString, FloatChannel)>,
    /// Color channels: (node_name, channel).
    pub color_channels: Vec<(FixedString, ColorChannel)>,
    /// Bool channels: (node_name, channel).
    pub bool_channels: Vec<(FixedString, BoolChannel)>,
    /// Texture-flipbook channels: (node_name, channel). Captured from
    /// `NiFlipController` blocks during NIF import. Carries resolved
    /// source-texture paths so the sample-time consumer can rebind a
    /// texture without re-walking the NIF scene. See `TextureFlipChannel`.
    pub texture_flip_channels: Vec<(FixedString, TextureFlipChannel)>,
    /// Text key events: (time, label). Imported from NiTextKeyExtraData.
    /// Emitted as transient ECS markers when crossed during playback.
    /// Labels are interned at clip-load time so the visitor and event-
    /// emission paths can hand symbols around without per-fire allocations
    /// (#231 / SI-04). Resolve via `StringPool::resolve` when a `&str`
    /// is actually required.
    pub text_keys: Vec<(f32, FixedString)>,
}
