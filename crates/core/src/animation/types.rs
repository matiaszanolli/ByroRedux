//! Animation data types: keyframes, channels, clips.

use crate::math::{Quat, Vec3};
use crate::string::FixedString;
use rustc_hash::FxHashMap;
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
    /// Stepped / constant hold (Gamebryo `KEY_CONST`): the segment holds
    /// the start key's value until the next key, with no interpolation.
    /// Used by hard-cut keyframed scenery and IK poses (LC-D5-02 / #1441).
    Const,
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
    /// Material emissive multiplier. Animated by
    /// `BSMaterialEmittanceMultController`. See #3327. Mirrors
    /// `ShaderFloat`'s state as of #2221: reaches this far, no
    /// ECS/render consumer yet.
    EmissiveMultiple,
    /// Glass refraction strength (Stealth Boy / heat-haze). Animated by
    /// `BSRefractionStrengthController`. See #3327 — same no-consumer-yet
    /// caveat as `EmissiveMultiple`.
    RefractionStrength,
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
    /// `LightSource.emitter.radiant_intensity` for the duration of the clip.
    /// See #983.
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
/// (per nif.xml).
///
/// Renderer integration (#2221) covers slot 0 (BASE_MAP) only — the
/// overwhelmingly common vanilla case (TV static, computer terminal
/// screens): `anim_convert::attach_animation_sinks` resolves
/// `source_paths` to bindless handles once at clip-attach time into an
/// `AnimatedTextureFlip` sink, `apply_texture_flip_channels`
/// (`byroredux::systems::animation`) updates its `current_index` from
/// this channel's curve each frame, and
/// `byroredux::render::static_meshes` reads the resulting handle in
/// place of the spawn-time `TextureHandle`. A flip targeting any other
/// slot still parses and stores correctly but has no renderer consumer
/// yet — it would need the same shader-type-aware `slot_to_role`
/// dispatch `cell_loader/spawn/mesh_instance.rs` uses for XTXR
/// overrides, which the render loop doesn't currently have a
/// mesh-material handle to run.
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
    /// Start-time offset in seconds from `NiControllerSequence.phase` (or the
    /// embedded controller envelope). Gamebryo's `NiTimeController` maps clip
    /// time as `frequency * t + phase`, so this staggers instances of the same
    /// clip that should not animate in lockstep.
    ///
    /// Seeded into `AnimationPlayer::local_time` / `AnimationLayer` start time
    /// at attach rather than added per-sample: the sampler's cycle handling
    /// (clamp / loop / ping-pong) already normalises `local_time` into
    /// `[0, duration)`, and an offset applied after that would fight it.
    ///
    /// **Zero on all vanilla FNV content** — measured across every FNV + DLC
    /// mesh archive (20,677 NIFs, 4,989 KFs): not one clip, embedded or KF,
    /// carries a non-zero phase (nor a frequency ≠ 1). #3097 wired the field
    /// through the NIF-side clip; pre-#3345 the NIF→core conversion dropped it
    /// again, so it would have silently no-op'd on the Skyrim/FO4 content where
    /// phase does appear.
    pub phase: f32,
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
    /// `FxHashMap` (#3677 / PERF-D1-2026-08-30-01) — `FixedString` is a
    /// 4-byte interned symbol, exactly the shape std's SipHash-1-3 loses
    /// on hardest; this map is probed once per animated channel per
    /// animated entity per frame. See `_audit-common.md`'s "Hot-path
    /// hashing" doctrine (#2923, #1368, #2174).
    pub channels: FxHashMap<FixedString, TransformChannel>,
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

#[cfg(test)]
mod fx_hash_guard_tests {
    //! #3677 / PERF-D1-2026-08-30-01 — a source-scan pin, mirroring
    //! `crates/renderer/src/vulkan/context/mod.rs`'s
    //! `pose_dirty_crosses_the_crate_boundary_without_siphash` (#2923):
    //! asserts by text rather than by type so a future edit that reverts
    //! `AnimationClip.channels` back to std's SipHash-1-3 fails at
    //! `cargo test` instead of silently regressing the hot path.

    // No negative "must not contain the std form" companion assertion here
    // (unlike the `crates/renderer` precedent this mirrors): that precedent
    // scans *other* files, but this one's only source is `types.rs` itself
    // — self-including via `include_str!` means the negative string would
    // match this very assertion's own text.
    #[test]
    fn animation_clip_channels_stays_fx_hash_map() {
        let src = include_str!("types.rs");
        assert!(
            src.contains("pub channels: FxHashMap<FixedString, TransformChannel>"),
            "AnimationClip.channels must stay FxHashMap (#3677)"
        );
    }
}
