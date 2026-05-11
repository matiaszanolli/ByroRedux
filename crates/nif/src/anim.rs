//! Animation clip import from NIF/KF files.
//!
//! Converts NiControllerSequence blocks (with their referenced interpolators
//! and keyframe data) into engine-friendly `AnimationClip` structures that
//! are decoupled from the NIF block graph.

use crate::blocks::controller::{
    ControlledBlock, NiControllerManager, NiControllerSequence, NiGeomMorpherController,
    NiMorphData,
};
use crate::blocks::extra_data::{AnimNoteType, BsAnimNote, BsAnimNotes};
use crate::blocks::interpolator::NiTextKeyExtraData;
use crate::blocks::interpolator::{
    FloatKey, KeyGroup, KeyType, NiBSplineBasisData, NiBSplineCompFloatInterpolator,
    NiBSplineCompPoint3Interpolator, NiBSplineCompTransformInterpolator, NiBSplineData,
    NiBlendBoolInterpolator, NiBlendFloatInterpolator, NiBlendInterpolator,
    NiBlendPoint3Interpolator, NiBlendTransformInterpolator, NiBoolInterpolator, NiColorData,
    NiColorInterpolator, NiFloatData, NiFloatInterpolator, NiLookAtInterpolator,
    NiPathInterpolator, NiPoint3Interpolator, NiPosData, NiTransformData, NiTransformInterpolator,
    Vec3Key,
};
use crate::blocks::properties::NiStringPalette;
use crate::scene::NifScene;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// Sampling rate for B-spline interpolators during import.
/// 30 Hz matches the typical Bethesda animation frame rate.
const BSPLINE_SAMPLE_HZ: f32 = 30.0;

/// Degree of the open uniform B-spline used by Gamebryo/Creation engine.
/// Always 3 (cubic) per nif.xml / legacy Gamebryo source.
const BSPLINE_DEGREE: usize = 3;

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
/// Discovers sequences in two ways:
/// 1. Top-level `NiControllerSequence` blocks (standalone .kf files)
/// 2. `NiControllerManager` blocks that reference sequences embedded
///    in .nif files (follows `sequence_refs` to find them)
///
/// The `cumulative` flag from NiControllerManager is stored in each
/// clip's `accum_root_name` field (non-empty when cumulative).
pub fn import_kf(scene: &NifScene) -> Vec<AnimationClip> {
    let mut clips = Vec::new();
    let mut seen_indices = std::collections::HashSet::new();

    // Path 1: NiControllerManager → follow sequence_refs.
    // This handles .nif files with embedded animations.
    for block in &scene.blocks {
        let Some(mgr) = block.as_any().downcast_ref::<NiControllerManager>() else {
            continue;
        };

        for seq_ref in &mgr.sequence_refs {
            let Some(idx) = seq_ref.index() else {
                continue;
            };
            if !seen_indices.insert(idx) {
                continue; // already imported
            }

            let Some(seq) = scene.get_as::<NiControllerSequence>(idx) else {
                log::warn!(
                    "NiControllerManager references block {} but it's not a NiControllerSequence",
                    idx
                );
                continue;
            };

            let clip = import_sequence(scene, seq);
            if clip_has_data(&clip) {
                log::debug!(
                    "Imported sequence '{}' from NiControllerManager (cumulative={})",
                    clip.name,
                    mgr.cumulative
                );
                clips.push(clip);
            }
        }
    }

    // Path 2: Top-level NiControllerSequence blocks (standalone .kf files).
    // Skip any already imported via a NiControllerManager above.
    for (i, block) in scene.blocks.iter().enumerate() {
        if seen_indices.contains(&i) {
            continue;
        }

        let Some(seq) = block.as_any().downcast_ref::<NiControllerSequence>() else {
            continue;
        };

        let clip = import_sequence(scene, seq);
        if clip_has_data(&clip) {
            clips.push(clip);
        }
    }

    clips
}

fn clip_has_data(clip: &AnimationClip) -> bool {
    !clip.channels.is_empty()
        || !clip.float_channels.is_empty()
        || !clip.color_channels.is_empty()
        || !clip.bool_channels.is_empty()
        || !clip.texture_flip_channels.is_empty()
}

/// Import mesh-embedded animation controllers into a single looping
/// `AnimationClip`. See #261.
///
/// Walks every NiObjectNET-bearing block in the scene (scene-graph
/// nodes + geometry). For each block whose `controller_ref` is
/// non-null, follows the `next_controller_ref` chain and emits a
/// float / color / bool channel per supported controller type. These
/// are the *ambient* animations authored directly into the .nif —
/// UV scrolling on water, alpha fade on ghost meshes, visibility
/// flicker on torch flames, material color pulses on lava — as
/// distinct from the sequence-driven KF clips that [`import_kf`]
/// collects.
///
/// Returns `None` when no supported embedded controllers are found.
/// The clip's `cycle_type` is `Loop` and `frequency` is `1.0` so the
/// runtime plays it continuously — cell-load-time start, no end.
///
/// Supported controller types match the KF importer's dispatch
/// (`NiAlphaController`, `NiVisController`, `NiTextureTransformController`,
/// `NiMaterialColorController`, `BSEffect/BSLightingShaderProperty{Float,Color}Controller`,
/// `NiUVController`). Unsupported types are skipped with a debug-log.
pub fn import_embedded_animations(scene: &NifScene) -> Option<AnimationClip> {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::controller::{
        BsShaderController, NiFlipController, NiMaterialColorController, NiSingleInterpController,
        NiTextureTransformController,
    };
    use crate::blocks::node::{NiCamera, NiNode};
    use crate::blocks::tri_shape::{BsTriShape, NiTriShape};
    use crate::types::BlockRef;

    // Resolve a block's NiObjectNET view (name + controller_ref). Covers
    // every block type the import pipeline cares about — adding a new
    // block kind with its own embedded-controller chain is a one-line
    // downcast addition here.
    fn net_of<'a>(block: &'a dyn crate::NiObject) -> Option<&'a NiObjectNETData> {
        let any = block.as_any();
        if let Some(n) = any.downcast_ref::<NiNode>() {
            return Some(&n.av.net);
        }
        if let Some(t) = any.downcast_ref::<NiTriShape>() {
            return Some(&t.av.net);
        }
        if let Some(t) = any.downcast_ref::<BsTriShape>() {
            return Some(&t.av.net);
        }
        if let Some(c) = any.downcast_ref::<NiCamera>() {
            return Some(&c.av.net);
        }
        // Property blocks that carry embedded controllers (material color,
        // shader float/color). Using a macro would save lines but every
        // block here has a `.net` field reachable at a distinct path.
        if let Some(b) = any.downcast_ref::<crate::blocks::properties::NiMaterialProperty>() {
            return Some(&b.net);
        }
        if let Some(b) = any.downcast_ref::<crate::blocks::properties::NiTexturingProperty>() {
            return Some(&b.net);
        }
        if let Some(b) = any.downcast_ref::<crate::blocks::shader::BSLightingShaderProperty>() {
            return Some(&b.net);
        }
        if let Some(b) = any.downcast_ref::<crate::blocks::shader::BSEffectShaderProperty>() {
            return Some(&b.net);
        }
        let _ = NiAVObjectData::parse; // keep the import path alive for future block types
        None
    }

    // Follow the `next_controller_ref` chain from `controller_ref` head,
    // invoking `visit` once per controller block. Returns on chain
    // termination (BlockRef::NULL) or on the first missing block.
    fn walk_controller_chain(
        scene: &NifScene,
        head: BlockRef,
        mut visit: impl FnMut(usize, &dyn crate::NiObject),
    ) {
        let mut cur = head;
        let mut hops = 0u32;
        while let Some(idx) = cur.index() {
            let Some(block) = scene.blocks.get(idx) else {
                break;
            };
            visit(idx, block.as_ref());

            // Advance via NiTimeControllerBase.next_controller_ref. Every
            // NIF controller inherits NiTimeControllerBase, but the field
            // lives at block-specific offsets — dispatch per known type.
            let any = block.as_any();
            cur = if let Some(c) = any.downcast_ref::<NiSingleInterpController>() {
                c.base.next_controller_ref
            } else if let Some(c) = any.downcast_ref::<NiTextureTransformController>() {
                c.base.next_controller_ref
            } else if let Some(c) = any.downcast_ref::<NiFlipController>() {
                // NiFlipController : NiFloatInterpController : NiSingleInterpController.
                // Two `.base` hops to reach NiTimeControllerBase.
                c.base.base.next_controller_ref
            } else if let Some(c) = any.downcast_ref::<BsShaderController>() {
                c.base.base.next_controller_ref
            } else if let Some(c) = any.downcast_ref::<NiMaterialColorController>() {
                c.base.next_controller_ref
            } else if let Some(c) = any.downcast_ref::<crate::blocks::controller::NiUVController>()
            {
                c.base.next_controller_ref
            } else if let Some(c) = any.downcast_ref::<NiGeomMorpherController>() {
                c.base.next_controller_ref
            } else {
                // Unknown chain node — stop rather than infinite-loop.
                BlockRef::NULL
            };
            // Cycle guard: Bethesda controllers don't normally form cycles,
            // but malformed files could. Bound the walk at 64 hops.
            hops += 1;
            if hops >= 64 {
                log::warn!(
                    "Embedded controller chain exceeded 64 hops at block {} — stopping",
                    idx
                );
                break;
            }
        }
    }

    let mut clip = AnimationClip {
        name: "embedded".to_string(),
        duration: 0.0,
        cycle_type: CycleType::Loop,
        frequency: 1.0,
        weight: 1.0,
        accum_root_name: None,
        channels: HashMap::new(),
        float_channels: Vec::new(),
        color_channels: Vec::new(),
        bool_channels: Vec::new(),
        texture_flip_channels: Vec::new(),
        text_keys: Vec::new(),
    };

    // Track seen controllers so a controller linked into multiple
    // chains (rare but legal — shared via NiControllerManager) doesn't
    // produce duplicate channels.
    let mut seen_controllers = std::collections::HashSet::<usize>::new();

    for block in &scene.blocks {
        let Some(net) = net_of(block.as_ref()) else {
            continue;
        };
        if net.controller_ref.is_null() {
            continue;
        }
        let Some(node_name) = net.name.clone() else {
            // Unnamed nodes can't receive animation at runtime — the
            // animation stack keys channels by FixedString(name).
            continue;
        };

        walk_controller_chain(scene, net.controller_ref, |ctrl_idx, ctrl_block| {
            if !seen_controllers.insert(ctrl_idx) {
                return;
            }
            let ctrl_type = ctrl_block.block_type_name();
            let any = ctrl_block.as_any();

            // For each controller, dispatch on type and use the
            // ControlledBlock-free extract_*_at helpers.
            match ctrl_type {
                "NiAlphaController" => {
                    let interp_idx = any
                        .downcast_ref::<NiSingleInterpController>()
                        .and_then(|c| c.interpolator_ref.index());
                    if let Some(idx) = interp_idx {
                        if let Some(ch) = extract_float_channel_at(scene, idx, FloatTarget::Alpha) {
                            clip.float_channels.push((Arc::clone(&node_name), ch));
                        }
                    }
                }
                "NiVisController" => {
                    let interp_idx = any
                        .downcast_ref::<NiSingleInterpController>()
                        .and_then(|c| c.interpolator_ref.index());
                    if let Some(idx) = interp_idx {
                        if let Some(ch) = extract_bool_channel_at(scene, idx) {
                            clip.bool_channels.push((Arc::clone(&node_name), ch));
                        }
                    }
                }
                "NiTextureTransformController" => {
                    if let Some(c) = any.downcast_ref::<NiTextureTransformController>() {
                        let target = match c.operation {
                            0 => FloatTarget::UvOffsetU,
                            1 => FloatTarget::UvOffsetV,
                            2 => FloatTarget::UvScaleU,
                            3 => FloatTarget::UvScaleV,
                            4 => FloatTarget::UvRotation,
                            _ => FloatTarget::UvOffsetU,
                        };
                        if let Some(idx) = c.interpolator_ref.index() {
                            if let Some(ch) = extract_float_channel_at(scene, idx, target) {
                                clip.float_channels.push((Arc::clone(&node_name), ch));
                            }
                        }
                    }
                }
                "NiMaterialColorController" => {
                    if let Some(c) = any.downcast_ref::<NiMaterialColorController>() {
                        if let Some(idx) = c.interpolator_ref.index() {
                            let keys = resolve_color_keys_at(scene, idx);
                            if !keys.is_empty() {
                                let target = match c.target_color {
                                    1 => ColorTarget::Ambient,
                                    2 => ColorTarget::Specular,
                                    3 => ColorTarget::Emissive,
                                    _ => ColorTarget::Diffuse,
                                };
                                clip.color_channels
                                    .push((Arc::clone(&node_name), ColorChannel { target, keys }));
                            }
                        }
                    }
                }
                "BSEffectShaderPropertyFloatController"
                | "BSLightingShaderPropertyFloatController" => {
                    let interp_idx = any
                        .downcast_ref::<BsShaderController>()
                        .and_then(|c| c.base.interpolator_ref.index());
                    if let Some(idx) = interp_idx {
                        if let Some(ch) =
                            extract_float_channel_at(scene, idx, FloatTarget::ShaderFloat)
                        {
                            clip.float_channels.push((Arc::clone(&node_name), ch));
                        }
                    }
                }
                "BSEffectShaderPropertyColorController"
                | "BSLightingShaderPropertyColorController" => {
                    let interp_idx = any
                        .downcast_ref::<BsShaderController>()
                        .and_then(|c| c.base.interpolator_ref.index());
                    if let Some(idx) = interp_idx {
                        let keys = resolve_color_keys_at(scene, idx);
                        if !keys.is_empty() {
                            clip.color_channels.push((
                                Arc::clone(&node_name),
                                ColorChannel {
                                    target: ColorTarget::ShaderColor,
                                    keys,
                                },
                            ));
                        }
                    }
                }
                "NiFlipController" => {
                    // Texture-flipbook controller (#545). Resolve the
                    // per-frame source list to filenames at import time
                    // so the runtime never has to walk back into the
                    // NIF scene. Float keys come from the inherited
                    // NiSingleInterpController.interpolator_ref —
                    // typically a stepped saw 0..N over the cycle.
                    if let Some(c) = any.downcast_ref::<NiFlipController>() {
                        let source_paths = resolve_flip_source_paths(scene, &c.sources);
                        if source_paths.is_empty() {
                            // Empty source list — controller is structurally
                            // valid but contributes nothing to render.
                            return;
                        }
                        let keys = c
                            .base
                            .interpolator_ref
                            .index()
                            .and_then(|idx| {
                                extract_float_channel_at(scene, idx, FloatTarget::ShaderFloat)
                            })
                            .map(|ch| ch.keys)
                            .unwrap_or_default();
                        clip.texture_flip_channels.push((
                            Arc::clone(&node_name),
                            TextureFlipChannel {
                                texture_slot: c.texture_slot,
                                source_paths,
                                keys,
                            },
                        ));
                    }
                }
                "NiUVController" => {
                    // The NiUVController + NiUVData path is distinct from
                    // the NiTextureTransformController: UVData stores four
                    // independent float-key groups (offsetU, offsetV,
                    // scaleU, scaleV). Emit up to four channels per host
                    // node, each with its own target. See #154.
                    if let Some(c) = any.downcast_ref::<crate::blocks::controller::NiUVController>()
                    {
                        if let Some(data_idx) = c.data_ref.index() {
                            if let Some(data) =
                                scene.get_as::<crate::blocks::interpolator::NiUVData>(data_idx)
                            {
                                // NiUVData.groups = [offset_u, offset_v, tiling_u, tiling_v].
                                for (group, target) in [
                                    (&data.groups[0], FloatTarget::UvOffsetU),
                                    (&data.groups[1], FloatTarget::UvOffsetV),
                                    (&data.groups[2], FloatTarget::UvScaleU),
                                    (&data.groups[3], FloatTarget::UvScaleV),
                                ] {
                                    if group.keys.is_empty() {
                                        continue;
                                    }
                                    let keys: Vec<AnimFloatKey> = group
                                        .keys
                                        .iter()
                                        .map(|k| AnimFloatKey {
                                            time: k.time,
                                            value: k.value,
                                        })
                                        .collect();
                                    clip.float_channels.push((
                                        Arc::clone(&node_name),
                                        FloatChannel { target, keys },
                                    ));
                                }
                            }
                        }
                    }
                }
                other => {
                    log::debug!(
                        "Skipping unsupported embedded controller type '{}' on node '{}'",
                        other,
                        node_name
                    );
                }
            }
        });
    }

    if !clip_has_data(&clip) {
        return None;
    }

    // Duration = maximum key time across every channel — the looping
    // sampler wraps around this boundary. Fall back to 1.0 s when every
    // channel is a single constant key (e.g. NiVisController with a
    // constant visibility value that still needs a non-zero duration to
    // avoid a mod-by-zero in the stack sampler).
    let mut max_time = 0.0_f32;
    for (_, ch) in &clip.float_channels {
        if let Some(k) = ch.keys.last() {
            max_time = max_time.max(k.time);
        }
    }
    for (_, ch) in &clip.color_channels {
        if let Some(k) = ch.keys.last() {
            max_time = max_time.max(k.time);
        }
    }
    for (_, ch) in &clip.bool_channels {
        if let Some(k) = ch.keys.last() {
            max_time = max_time.max(k.time);
        }
    }
    for (_, ch) in &clip.texture_flip_channels {
        if let Some(k) = ch.keys.last() {
            max_time = max_time.max(k.time);
        }
    }
    clip.duration = if max_time > 0.0 { max_time } else { 1.0 };

    Some(clip)
}

fn import_sequence(scene: &NifScene, seq: &NiControllerSequence) -> AnimationClip {
    let name = seq
        .name
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| "unnamed".to_string());
    let duration = seq.stop_time - seq.start_time;
    let cycle_type = CycleType::from_u32(seq.cycle_type);
    let frequency = seq.frequency;
    let weight = seq.weight;
    let accum_root_name = seq.accum_root_name.as_deref().map(str::to_string);
    let mut channels = HashMap::new();
    let mut float_channels = Vec::new();
    let mut color_channels = Vec::new();
    let mut bool_channels = Vec::new();
    let mut texture_flip_channels = Vec::new();

    for cb in &seq.controlled_blocks {
        let resolved_node_name = resolve_cb_string(scene, cb, CbString::NodeName);
        let resolved_ctrl_type = resolve_cb_string(scene, cb, CbString::ControllerType);
        let Some(node_name) = resolved_node_name else {
            continue;
        };
        let controller_type = resolved_ctrl_type.as_deref().unwrap_or("");

        match controller_type {
            "NiTransformController" => {
                if let Some(mut channel) = extract_transform_channel(scene, cb) {
                    channel.priority = cb.priority;
                    channels.insert(Arc::clone(&node_name), channel);
                }
            }
            "NiMaterialColorController" => {
                if let Some(ch) = extract_color_channel(scene, cb) {
                    color_channels.push((Arc::clone(&node_name), ch));
                }
            }
            "NiAlphaController" => {
                if let Some(ch) = extract_float_channel(scene, cb, FloatTarget::Alpha) {
                    float_channels.push((Arc::clone(&node_name), ch));
                }
            }
            "NiVisController" => {
                if let Some(ch) = extract_bool_channel(scene, cb) {
                    bool_channels.push((Arc::clone(&node_name), ch));
                }
            }
            "NiTextureTransformController" => {
                if let Some(ch) = extract_texture_transform_channel(scene, cb) {
                    float_channels.push((Arc::clone(&node_name), ch));
                }
            }
            "BSEffectShaderPropertyFloatController" | "BSLightingShaderPropertyFloatController" => {
                if let Some(ch) = extract_float_channel(scene, cb, FloatTarget::ShaderFloat) {
                    float_channels.push((Arc::clone(&node_name), ch));
                }
            }
            "BSEffectShaderPropertyColorController" | "BSLightingShaderPropertyColorController" => {
                if let Some(ch) = extract_shader_color_channel(scene, cb) {
                    color_channels.push((Arc::clone(&node_name), ch));
                }
            }
            "NiGeomMorpherController" => {
                // Each morph target is a separate controlled_block with its
                // own interpolator. cb.controller_id identifies the target
                // by name; resolve it to an index in the NiMorphData array
                // referenced by the controller. See #262.
                let target_idx = resolve_morph_target_index(scene, cb).unwrap_or(0);
                if let Some(ch) =
                    extract_float_channel(scene, cb, FloatTarget::MorphWeight(target_idx))
                {
                    float_channels.push((Arc::clone(&node_name), ch));
                }
            }
            "NiUVController" => {
                // UV scrolling — maps to UvOffsetU/V float channels.
                // The default UV scroll is offset U (horizontal scroll).
                if let Some(ch) = extract_float_channel(scene, cb, FloatTarget::UvOffsetU) {
                    float_channels.push((Arc::clone(&node_name), ch));
                }
            }
            "NiFlipController" => {
                // Texture flipbook (#545). The KF path resolves the
                // controller block via `cb.controller_ref` so we can
                // pick up `texture_slot` + the `sources` array; the
                // float keys come from `cb.interpolator_ref`. Skip
                // silently if either ref fails to resolve.
                if let Some(ctrl_idx) = cb.controller_ref.index() {
                    if let Some(ctrl) =
                        scene.get_as::<crate::blocks::controller::NiFlipController>(ctrl_idx)
                    {
                        let source_paths = resolve_flip_source_paths(scene, &ctrl.sources);
                        if source_paths.is_empty() {
                            continue;
                        }
                        let keys = extract_float_channel(scene, cb, FloatTarget::ShaderFloat)
                            .map(|ch| ch.keys)
                            .unwrap_or_default();
                        texture_flip_channels.push((
                            Arc::clone(&node_name),
                            TextureFlipChannel {
                                texture_slot: ctrl.texture_slot,
                                source_paths,
                                keys,
                            },
                        ));
                    }
                }
            }
            _ => {
                log::debug!(
                    "Skipping unsupported controller type: '{}'",
                    controller_type
                );
            }
        }
    }

    // Import text keys from NiTextKeyExtraData if referenced.
    let mut text_keys = seq
        .text_keys_ref
        .index()
        .and_then(|idx| scene.get_as::<NiTextKeyExtraData>(idx))
        .map(|tkd| tkd.text_keys.clone())
        .unwrap_or_default();

    // Import BSAnimNote IK hints (#432). Each `BSAnimNotes` referenced by
    // `seq.anim_note_refs` holds a list of `BSAnimNote` refs; each note
    // has a time + IK kind (grab / look) + conditional payload. Serialize
    // each as a labeled text-key entry so the existing
    // `collect_text_key_events` dispatch feeds them into the ECS text-
    // event channel alongside the gameplay triggers — consumers can
    // filter on the `animnote:` prefix to pick up IK hints specifically.
    let anim_notes_before = text_keys.len();
    for notes_ref in &seq.anim_note_refs {
        let Some(notes_idx) = notes_ref.index() else {
            continue;
        };
        let Some(notes) = scene.get_as::<BsAnimNotes>(notes_idx) else {
            continue;
        };
        for note_ref in &notes.notes {
            let Some(note_idx) = note_ref.index() else {
                continue;
            };
            let Some(note) = scene.get_as::<BsAnimNote>(note_idx) else {
                continue;
            };
            text_keys.push((note.time, format_anim_note_label(note)));
        }
    }

    if !text_keys.is_empty() {
        log::debug!(
            "Imported {} text keys ({} anim-note hints) for sequence '{}'",
            text_keys.len(),
            text_keys.len() - anim_notes_before,
            name
        );
    }

    AnimationClip {
        name,
        duration,
        cycle_type,
        frequency,
        weight,
        accum_root_name,
        channels,
        float_channels,
        color_channels,
        bool_channels,
        texture_flip_channels,
        text_keys,
    }
}

/// Which of the five `ControlledBlock` string fields to resolve when
/// walking the dual string-table / string-palette layouts.
#[derive(Debug, Clone, Copy)]
enum CbString {
    NodeName,
    ControllerType,
}

/// Resolve a `ControlledBlock` string field across both on-disk layouts.
///
/// NIFs from Oblivion / pre-FNV Bethesda titles (`10.2.0.0 ≤ v < 20.1.0.1`)
/// store the five per-block strings as byte offsets into a sibling
/// `NiStringPalette`; newer files inline them via the header's string
/// table and the parser pre-resolves them into `cb.node_name` et al.
/// Before #402 the importer only checked the string-table fields, so
/// every Oblivion `ControlledBlock` short-circuited at the `node_name`
/// guard and `import_kf` returned zero clips on all 1843 Oblivion KF
/// files. Falling through to the palette lookup fixes the whole range
/// of pre-Skyrim animations (Oblivion / Morrowind BBBB-era content)
/// without changing modern-path semantics.
fn resolve_cb_string(scene: &NifScene, cb: &ControlledBlock, which: CbString) -> Option<Arc<str>> {
    let (inline, offset) = match which {
        CbString::NodeName => (cb.node_name.as_ref(), cb.node_name_offset),
        CbString::ControllerType => (cb.controller_type.as_ref(), cb.controller_type_offset),
    };
    if let Some(s) = inline {
        return Some(Arc::clone(s));
    }
    let pal_idx = cb.string_palette_ref.index()?;
    let palette = scene.get_as::<NiStringPalette>(pal_idx)?;
    let s = palette.get_string(offset)?;
    if s.is_empty() {
        return None;
    }
    Some(Arc::from(s))
}

/// Serialize a `BSAnimNote` into a label suitable for the `text_keys`
/// channel. Downstream consumers filter on the `animnote:` prefix to
/// pick up IK hints specifically and ignore gameplay text events. See
/// the `BSAnimNote` type for field semantics.
fn format_anim_note_label(note: &BsAnimNote) -> String {
    match note.kind {
        AnimNoteType::GrabIk => {
            format!("animnote:grabik:arm={}", note.arm.unwrap_or(0))
        }
        AnimNoteType::LookIk => {
            format!(
                "animnote:lookik:gain={};state={}",
                note.gain.unwrap_or(0.0),
                note.state.unwrap_or(0)
            )
        }
        AnimNoteType::Invalid => "animnote:invalid".to_string(),
        AnimNoteType::Unknown(raw) => format!("animnote:unknown={raw}"),
    }
}

/// Follow a `NiBlend*Interpolator` indirection to its dominant sub-
/// interpolator. Returns the picked sub-interpolator's block index, or
/// `None` when `interp_idx` is not a blend variant or has no usable
/// weighted items (e.g. the common "manager-controlled" case where the
/// manager supplies the sub-interpolator externally via sibling
/// sequences — those are driven through their own `ControlledBlock`s
/// and this extractor has nothing to pull off the blend block itself).
///
/// "Dominant" = the item with the highest `normalized_weight` that has
/// a non-null interpolator_ref. This is a single-layer resolution —
/// the AnimationStack performs layer-based blending at the ECS level,
/// so picking one representative interpolator here gets the data
/// through the bottleneck without faking a runtime blend at import
/// time. See #334 (AR-08).
fn resolve_blend_interpolator_target(scene: &NifScene, interp_idx: usize) -> Option<usize> {
    let base: &NiBlendInterpolator =
        if let Some(b) = scene.get_as::<NiBlendTransformInterpolator>(interp_idx) {
            &b.base
        } else if let Some(b) = scene.get_as::<NiBlendFloatInterpolator>(interp_idx) {
            &b.base
        } else if let Some(b) = scene.get_as::<NiBlendPoint3Interpolator>(interp_idx) {
            &b.base
        } else if let Some(b) = scene.get_as::<NiBlendBoolInterpolator>(interp_idx) {
            &b.base
        } else {
            return None;
        };

    // Manager-controlled blends carry an empty `items` array — the
    // NiControllerManager drives the sub-interpolators externally via
    // sibling ControlledBlocks. Fall through to None so the caller
    // logs nothing; those sequences import cleanly through their own
    // interpolator_refs.
    base.items
        .iter()
        .filter_map(|it| {
            it.interpolator_ref
                .index()
                .map(|i| (i, it.normalized_weight))
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx)
}

fn extract_transform_channel(scene: &NifScene, cb: &ControlledBlock) -> Option<TransformChannel> {
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
fn constant_transform_channel(t: &crate::types::NiQuatTransform) -> TransformChannel {
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
fn extract_transform_channel_path(
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

// ── B-spline evaluation (issue #155) ──────────────────────────────────────
//
// NiBSplineCompTransformInterpolator stores quantized control points for
// three sub-curves (translation, rotation, scale) that drive one node's
// transform. The data block contains a flat array of i16 control points;
// per-channel handles index into that array. Decompression:
//
//     value = offset + (short / 32767) * half_range
//
// Each channel is an open uniform cubic B-spline (degree 3). Evaluation
// uses a simple De Boor step: given `N` control points (N >= 4), the
// curve is parametrized over `[0, N - 3]` with knots
//     0, 0, 0, 0, 1, 2, ..., N-4, N-3, N-3, N-3, N-3.
// We sample the parameter `u ∈ [0, N - 3]` uniformly in time over
// `[start_time, stop_time]` and emit linear TQS keys — downstream
// evaluation interpolates linearly between samples, which is a visible
// but acceptable approximation of the continuous cubic curve at 30 Hz.
//
// This is the minimum viable implementation. A follow-up could sample
// non-uniformly at tangent breakpoints or emit Hermite tangents instead
// of linear keys.

/// Number of channels a comp-transform interpolator stores per control
/// point per channel: 3 for translation (x,y,z), 4 for rotation
/// (w,x,y,z), 1 for scale.
const BSPLINE_TRANS_STRIDE: usize = 3;
const BSPLINE_ROT_STRIDE: usize = 4;
const BSPLINE_SCALE_STRIDE: usize = 1;

/// Dequantize a single compact control point to an f32.
#[inline]
fn dequant(raw: i16, offset: f32, half_range: f32) -> f32 {
    offset + (raw as f32 / 32767.0) * half_range
}

/// Evaluate an open uniform cubic B-spline at parameter `u ∈ [0, N - 3]`
/// given `n` control points (each point is `stride` floats packed
/// contiguously in `control_points`).
///
/// Uses De Boor's algorithm restricted to degree 3. For an open uniform
/// knot vector with clamped endpoints, `u == 0` evaluates exactly to the
/// first control point and `u == n - 3` evaluates exactly to the last.
fn deboor_cubic(control_points: &[f32], n: usize, stride: usize, u: f32) -> Vec<f32> {
    debug_assert!(stride > 0);
    if n < BSPLINE_DEGREE + 1 {
        // Underdetermined — just return the first CP (or zeros if empty).
        return control_points
            .get(0..stride)
            .map(|s| s.to_vec())
            .unwrap_or_else(|| vec![0.0; stride]);
    }

    // Clamp u to the valid parameter range.
    let u_max = (n - BSPLINE_DEGREE) as f32;
    let u = u.clamp(0.0, u_max);

    // Find the knot span k such that knots[k] <= u < knots[k+1].
    // For the open uniform knot vector:
    //   knots = [0, 0, 0, 0, 1, 2, ..., n-4, n-3, n-3, n-3, n-3]
    // Internal knots 1..=n-4 correspond to parameter values 1..=n-4.
    // k is in [3, n-1].
    let k = {
        let mut k = (u.floor() as usize) + BSPLINE_DEGREE;
        if k >= n {
            k = n - 1;
        }
        if k < BSPLINE_DEGREE {
            k = BSPLINE_DEGREE;
        }
        k
    };

    // De Boor triangle: start with control points P[k-d..=k] and
    // fold them toward the evaluation point.
    // For the open uniform knot vector, the spans between interior
    // knots are all length 1, so the alpha values simplify.
    let mut d: [Vec<f32>; BSPLINE_DEGREE + 1] = [
        vec![0.0; stride],
        vec![0.0; stride],
        vec![0.0; stride],
        vec![0.0; stride],
    ];
    for j in 0..=BSPLINE_DEGREE {
        let cp_idx = k + j - BSPLINE_DEGREE;
        let cp_idx = cp_idx.min(n - 1);
        let start = cp_idx * stride;
        d[j].copy_from_slice(&control_points[start..start + stride]);
    }

    // Open uniform clamped knot vector for `n` control points and
    // degree `d = BSPLINE_DEGREE`:
    //     knots = [0, 0, 0, 0, 1, 2, ..., n-d-1, n-d, n-d, n-d, n-d]
    // with `n + d + 1` total entries.
    //   - Indices 0..=d all map to value 0 (left clamp).
    //   - Indices d+1..=n-1 are interior knots at values 1..n-d-1.
    //   - Indices n..=n+d all map to value n-d (right clamp).
    let knot_at = |idx: isize| -> f32 {
        let d = BSPLINE_DEGREE as isize;
        let n_d = (n - BSPLINE_DEGREE) as isize;
        if idx <= d {
            0.0
        } else {
            (idx - d).min(n_d) as f32
        }
    };

    for r in 1..=BSPLINE_DEGREE {
        for j in (r..=BSPLINE_DEGREE).rev() {
            let k_i = k as isize;
            let left = knot_at(k_i + j as isize - BSPLINE_DEGREE as isize);
            let right = knot_at(k_i + j as isize - (r as isize - 1));
            let span = right - left;
            let alpha = if span > f32::EPSILON {
                (u - left) / span
            } else {
                0.0
            };
            for c in 0..stride {
                d[j][c] = (1.0 - alpha) * d[j - 1][c] + alpha * d[j][c];
            }
        }
    }

    d[BSPLINE_DEGREE].clone()
}

/// Dequantize a range of compact control points into a flat f32 vector,
/// preserving the given channel stride.
fn dequantize_channel(
    raw: &[i16],
    start: usize,
    count: usize,
    stride: usize,
    offset: f32,
    half_range: f32,
) -> Vec<f32> {
    let end = start + count * stride;
    // #408 — `count` and `stride` originate from `NiBSplineBasisData`
    // / per-channel STRIDE constants. Caller already validates `end`
    // against `raw.len()` via the `channel_slice` callers above, but
    // pre-allocate against the input slice length so a malformed
    // basis can't request more capacity than the data could justify.
    let mut out = Vec::with_capacity((count * stride).min(raw.len()));
    for &r in &raw[start..end] {
        out.push(dequant(r, offset, half_range));
    }
    out
}

/// Extract a `FloatChannel` by sampling a
/// `NiBSplineCompFloatInterpolator`. Same de Boor + `BSPLINE_SAMPLE_HZ`
/// recipe as `extract_transform_channel_bspline` — restricted to a
/// stride-1 scalar channel. Returns `None` when the spline data is
/// missing or under-defined (fewer than `degree + 1` control points,
/// invalid handle, off-end slice) and the caller's downstream behaviour
/// is to leave the channel at its bind value. See #936.
fn extract_float_channel_bspline(
    scene: &NifScene,
    interp: &NiBSplineCompFloatInterpolator,
    target: FloatTarget,
) -> Option<FloatChannel> {
    // Single-key static fallback used by every "no usable spline data"
    // branch below (null refs, missing data blocks, under-defined basis,
    // invalid handle). Returns None when the fallback `value` is also
    // FLT_MAX-sentinel, in which case the caller treats it as "no
    // animation" and the channel stays at its bind value.
    let static_fallback = || -> Option<FloatChannel> {
        if is_flt_max(interp.value) {
            return None;
        }
        Some(FloatChannel {
            target,
            keys: vec![AnimFloatKey {
                time: interp.start_time,
                value: interp.value,
            }],
        })
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

    let channel = channel_slice(
        interp.handle,
        &data.compact_control_points,
        n_cp,
        1, // scalar — stride 1
        interp.float_offset,
        interp.float_half_range,
    );

    let Some(cps) = channel else {
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
        let p = deboor_cubic(&cps, n_cp, 1, u);
        keys.push(AnimFloatKey {
            time: t,
            value: p[0],
        });
    }

    if keys.is_empty() {
        return None;
    }
    Some(FloatChannel { target, keys })
}

/// Extract a TransformChannel by sampling a NiBSplineCompTransformInterpolator.
fn extract_transform_channel_bspline(
    scene: &NifScene,
    interp: &NiBSplineCompTransformInterpolator,
) -> Option<TransformChannel> {
    let basis_idx = interp.basis_data_ref.index()?;
    let basis = scene.get_as::<NiBSplineBasisData>(basis_idx)?;
    let data_idx = interp.spline_data_ref.index()?;
    let data = scene.get_as::<NiBSplineData>(data_idx)?;

    let n_cp = basis.num_control_points as usize;
    if n_cp < BSPLINE_DEGREE + 1 {
        // Not enough control points for a cubic B-spline — fall back to
        // the static transform stored on the interpolator.
        return Some(static_transform_channel(interp));
    }

    // Determine number of samples from the animation duration.
    // #408 — clamp to a 1 M sample ceiling per channel (~9 hours of
    // animation at 30 Hz) so a malicious or corrupt `stop_time` can't
    // request `usize::MAX` slots and OOM the importer. Real anims top
    // out at a few thousand samples even for the longest cinematics.
    let duration = (interp.stop_time - interp.start_time).max(0.0);
    let n_samples_f = (duration * BSPLINE_SAMPLE_HZ).ceil();
    let n_samples = (n_samples_f as usize).max(2).min(1_000_000);

    // Per-channel setup. Each handle is an offset in i16 units into
    // `data.compact_control_points` where that channel's run of
    // `n_cp * stride` quantized values begins. INVALID = u32::MAX.
    let trans_q = channel_slice(
        interp.translation_handle,
        &data.compact_control_points,
        n_cp,
        BSPLINE_TRANS_STRIDE,
        interp.translation_offset,
        interp.translation_half_range,
    );
    let rot_q = channel_slice(
        interp.rotation_handle,
        &data.compact_control_points,
        n_cp,
        BSPLINE_ROT_STRIDE,
        interp.rotation_offset,
        interp.rotation_half_range,
    );
    let scale_q = channel_slice(
        interp.scale_handle,
        &data.compact_control_points,
        n_cp,
        BSPLINE_SCALE_STRIDE,
        interp.scale_offset,
        interp.scale_half_range,
    );

    let u_max = (n_cp - BSPLINE_DEGREE) as f32;

    let mut translation_keys = Vec::with_capacity(n_samples);
    let mut rotation_keys = Vec::with_capacity(n_samples);
    let mut scale_keys = Vec::with_capacity(n_samples);

    for i in 0..n_samples {
        let t = if n_samples > 1 {
            interp.start_time + duration * (i as f32 / (n_samples - 1) as f32)
        } else {
            interp.start_time
        };
        // Parameter u in [0, n-d] corresponding to t in [start, stop].
        let u = if duration > f32::EPSILON {
            ((t - interp.start_time) / duration) * u_max
        } else {
            0.0
        };

        // Translation. Bspline payload absent → pose-value fallback. If
        // the pose itself is the FLT_MAX sentinel ("axis inactive"),
        // skip the key so `sample_translation` returns None and the
        // bone keeps its bind-pose translation.
        if let Some(ref cps) = trans_q {
            let p = deboor_cubic(cps, n_cp, BSPLINE_TRANS_STRIDE, u);
            let zup = [p[0], p[1], p[2]];
            translation_keys.push(TranslationKey {
                time: t,
                value: zup_to_yup_pos(zup),
                forward: [0.0, 0.0, 0.0],
                backward: [0.0, 0.0, 0.0],
                tbc: None,
            });
        } else {
            let pose = [
                interp.transform.translation.x,
                interp.transform.translation.y,
                interp.transform.translation.z,
            ];
            if !(is_flt_max(pose[0]) || is_flt_max(pose[1]) || is_flt_max(pose[2])) {
                translation_keys.push(TranslationKey {
                    time: t,
                    value: zup_to_yup_pos(pose),
                    forward: [0.0, 0.0, 0.0],
                    backward: [0.0, 0.0, 0.0],
                    tbc: None,
                });
            }
        }

        // Rotation — normalize after sampling since the B-spline doesn't
        // enforce unit length on quaternions. Same FLT_MAX gate as
        // translation: an FLT_MAX-valued quaternion would normalise to
        // NaN and rotate the bone to garbage.
        if let Some(ref cps) = rot_q {
            let p = deboor_cubic(cps, n_cp, BSPLINE_ROT_STRIDE, u);
            let [mut w, mut x, mut y, mut z] = [p[0], p[1], p[2], p[3]];
            let len_sq = w * w + x * x + y * y + z * z;
            if len_sq > f32::EPSILON {
                let inv = 1.0 / len_sq.sqrt();
                w *= inv;
                x *= inv;
                y *= inv;
                z *= inv;
            } else {
                w = 1.0;
                x = 0.0;
                y = 0.0;
                z = 0.0;
            }
            rotation_keys.push(RotationKey {
                time: t,
                value: zup_to_yup_quat([w, x, y, z]),
                tbc: None,
            });
        } else {
            let q = interp.transform.rotation;
            if !(is_flt_max(q[0]) || is_flt_max(q[1]) || is_flt_max(q[2]) || is_flt_max(q[3])) {
                rotation_keys.push(RotationKey {
                    time: t,
                    value: zup_to_yup_quat(q),
                    tbc: None,
                });
            }
        }

        // Scale. Same FLT_MAX gate.
        if let Some(ref cps) = scale_q {
            let p = deboor_cubic(cps, n_cp, BSPLINE_SCALE_STRIDE, u);
            scale_keys.push(ScaleKey {
                time: t,
                value: p[0],
                forward: 0.0,
                backward: 0.0,
                tbc: None,
            });
        } else if !is_flt_max(interp.transform.scale) {
            scale_keys.push(ScaleKey {
                time: t,
                value: interp.transform.scale,
                forward: 0.0,
                backward: 0.0,
                tbc: None,
            });
        }
    }

    Some(TransformChannel {
        translation_keys,
        translation_type: KeyType::Linear,
        rotation_keys,
        rotation_type: KeyType::Linear,
        scale_keys,
        scale_type: KeyType::Linear,
        priority: 0,
    })
}

/// Build a static single-key TransformChannel from an interpolator's
/// fallback `NiQuatTransform`. FLT_MAX-encoded axes drop to empty key
/// lists so the bone keeps its bind-pose value (see FLT_MAX_SENTINEL).
fn static_transform_channel(interp: &NiBSplineCompTransformInterpolator) -> TransformChannel {
    let pose_t = [
        interp.transform.translation.x,
        interp.transform.translation.y,
        interp.transform.translation.z,
    ];
    let translation_keys =
        if is_flt_max(pose_t[0]) || is_flt_max(pose_t[1]) || is_flt_max(pose_t[2]) {
            Vec::new()
        } else {
            vec![TranslationKey {
                time: interp.start_time,
                value: zup_to_yup_pos(pose_t),
                forward: [0.0, 0.0, 0.0],
                backward: [0.0, 0.0, 0.0],
                tbc: None,
            }]
        };
    let q = interp.transform.rotation;
    let rotation_keys =
        if is_flt_max(q[0]) || is_flt_max(q[1]) || is_flt_max(q[2]) || is_flt_max(q[3]) {
            Vec::new()
        } else {
            vec![RotationKey {
                time: interp.start_time,
                value: zup_to_yup_quat(q),
                tbc: None,
            }]
        };
    let scale_keys = if is_flt_max(interp.transform.scale) {
        Vec::new()
    } else {
        vec![ScaleKey {
            time: interp.start_time,
            value: interp.transform.scale,
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

/// Slice the compact control-point array for a single channel and
/// dequantize it. Returns `None` when the handle is invalid (`u32::MAX`)
/// or when the slice would run off the end of the data buffer.
fn channel_slice(
    handle: u32,
    raw: &[i16],
    n_cp: usize,
    stride: usize,
    offset: f32,
    half_range: f32,
) -> Option<Vec<f32>> {
    if handle == u32::MAX {
        return None;
    }
    let start = handle as usize;
    let needed = n_cp * stride;
    if start
        .checked_add(needed)
        .map_or(true, |end| end > raw.len())
    {
        log::debug!(
            "NiBSplineCompTransformInterpolator: handle {} + {} > data len {}",
            handle,
            needed,
            raw.len(),
        );
        return None;
    }
    Some(dequantize_channel(
        raw, start, n_cp, stride, offset, half_range,
    ))
}

/// Resolve the morph target index for a NiGeomMorpherController-driven
/// controlled block. Follows cb.controller_ref → NiGeomMorpherController →
/// data_ref → NiMorphData.morphs, then matches cb.controller_id by name.
///
/// Returns `None` if any ref fails to resolve or the name isn't found;
/// the caller falls back to index 0 (matching the legacy behavior).
fn resolve_morph_target_index(scene: &NifScene, cb: &ControlledBlock) -> Option<u32> {
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
fn extract_float_channel(
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
fn extract_float_channel_at(
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
fn resolve_flip_source_paths(
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
fn resolve_color_keys(scene: &NifScene, cb: &ControlledBlock) -> Vec<AnimColorKey> {
    let Some(interp_idx) = cb.interpolator_ref.index() else {
        return Vec::new();
    };
    resolve_color_keys_at(scene, interp_idx)
}

/// ControlledBlock-free core of [`resolve_color_keys`]. Reused by the
/// mesh-embedded controller path (#261).
fn resolve_color_keys_at(scene: &NifScene, mut interp_idx: usize) -> Vec<AnimColorKey> {
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
fn sample_color_keys_bspline_point3(
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
fn extract_color_channel(scene: &NifScene, cb: &ControlledBlock) -> Option<ColorChannel> {
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
fn extract_shader_color_channel(scene: &NifScene, cb: &ControlledBlock) -> Option<ColorChannel> {
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
fn extract_bool_channel(scene: &NifScene, cb: &ControlledBlock) -> Option<BoolChannel> {
    let interp_idx = cb.interpolator_ref.index()?;
    extract_bool_channel_at(scene, interp_idx)
}

/// ControlledBlock-free core of [`extract_bool_channel`]. Reused by
/// the mesh-embedded controller path (#261).
fn extract_bool_channel_at(scene: &NifScene, mut interp_idx: usize) -> Option<BoolChannel> {
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
fn extract_texture_transform_channel(
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
const FLT_MAX_SENTINEL: f32 = 3.0e38;

fn is_flt_max(v: f32) -> bool {
    v.abs() >= FLT_MAX_SENTINEL
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
        KeyType::Constant => k0.value, // Step: hold value until next key
        KeyType::Linear => k0.value + (k1.value - k0.value) * t,
        KeyType::Quadratic => {
            let t2 = t * t;
            let t3 = t2 * t;
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            h00 * k0.value
                + h10 * k0.tangent_forward * dt
                + h01 * k1.value
                + h11 * k1.tangent_backward * dt
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
#[path = "anim_tests.rs"]
mod tests;
