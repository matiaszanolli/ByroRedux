//! Engine-side animation data types produced by the NIF→clip importer.
//!
//! `AnimationClip` is the top-level shape — a named bundle of per-bone
//! transform channels plus optional float / color / bool / texture-flip
//! channels for shader / morph / visibility animation. Decoupled from
//! the NIF block graph so the runtime animation system doesn't care
//! whether the source was a KF file, an embedded NiControllerSequence,
//! or a synthetic clip built in tests.

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
    /// Decode nif.xml's `CycleType` enum (`CYCLE_LOOP` = 0, `CYCLE_REVERSE`
    /// = 1, `CYCLE_CLAMP` = 2 — both the raw `NiControllerSequence.Cycle
    /// Type` field and the 2-bit `Cycle Type` member of `NiTimeController`'s
    /// `flags` bitfield share this ordinal). #3097 SIBLING check: this
    /// previously mapped 0→Clamp/1→Loop/2→Reverse, a one-position rotation
    /// off nif.xml's actual ordinals with no cross-check on record —
    /// every non-default `.kf` cycle type imported since M21 read as the
    /// wrong variant. Unknown values fall back to `Clamp` (nif.xml's own
    /// documented default for the bitfield member).
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Loop,
            1 => Self::Reverse,
            2 => Self::Clamp,
            _ => Self::Clamp,
        }
    }
}

/// #3862 unified `FloatTarget` / `ColorTarget` (a second, identical
/// 13- and 7-variant copy bridged by an identity match in
/// `byroredux/src/anim_convert.rs`); #5798 finishes the move for the key
/// and channel structs — they carried no information the core ones lack,
/// so `convert_nif_clip`'s element-by-element key loops existed only to
/// copy between the two spellings, and `phase` (#3345) was dropped for a
/// week exactly because one of those literals missed a sibling field.
///
/// The remaining NIF-side declarations below are the genuine wire tier:
/// [`CycleType`] (nif.xml ordinals), [`TextureFlipChannel`] (the raw
/// `TexType` slot, resolved to a role at the translate boundary) and
/// [`AnimationClip`] itself (String names, raw unsanitized scalars).
pub use byroredux_core::animation::types::{
    AnimBoolKey, AnimColorKey, AnimFloatKey, BoolChannel, ColorChannel, ColorTarget, FloatChannel,
    FloatTarget, RotationKey, ScaleKey, TransformChannel, TranslationKey,
};

/// Texture-flipbook animation channel — `NiFlipController` semantics.
///
/// Source paths are resolved at clip-load time from
/// `NiFlipController.sources → NiSourceTexture.filename` so the
/// runtime never has to walk back into the NIF scene. The float keys
/// drive a cycle position; the consumer picks
/// `source_paths[floor(value) % source_paths.len()]`. `texture_slot`
/// is the raw `TexType` enum value (0=BASE_MAP, 4=GLOW_MAP, …);
/// `byroredux::anim_convert` resolves it to a canonical
/// `FlipTextureRole` before the channel reaches the ECS (#3901).
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
    /// Phase offset within the cycle, as authored on
    /// `NiControllerSequence.Phase` (.kf clips) or the 2-bit-adjacent
    /// `NiTimeControllerBase.phase` field (embedded controllers). Parsed
    /// under #3097 and consumed since #3345: `AnimationPlayer::with_phase`
    /// seeds a player's initial elapsed time from it at each spawn site
    /// (`scene.rs`, `scene/nif_loader.rs`, `cell_loader/spawn.rs`), so
    /// same-mesh instances do not animate in lockstep.
    pub phase: f32,
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

#[cfg(test)]
mod target_enum_identity_tests {
    //! #3862 — prove the NIF-side names ARE the core types, not a second copy.
    //!
    //! Deliberately not a source-shape scan: type identity is something the
    //! compiler can check, and checking it that way makes the failure mode a
    //! build error at the moment someone redeclares the enum, rather than a
    //! test failure discovered later. If `pub use` above is ever replaced by a
    //! `pub enum`, these bindings stop type-checking.

    use byroredux_core::animation::types as core_types;

    #[test]
    fn the_nif_float_target_is_the_core_float_target() {
        // No `as`, no `From`, no match: assignment across the two paths only
        // compiles while they name one type.
        let target: super::FloatTarget = core_types::FloatTarget::MorphWeight(7);
        assert_eq!(target, core_types::FloatTarget::MorphWeight(7));
        assert_ne!(target, core_types::FloatTarget::MorphWeight(8));
    }

    #[test]
    fn the_nif_color_target_is_the_core_color_target() {
        let target: super::ColorTarget = core_types::ColorTarget::LightAmbient;
        assert_eq!(target, core_types::ColorTarget::LightAmbient);
        assert_ne!(target, core_types::ColorTarget::LightDiffuse);
    }
}
