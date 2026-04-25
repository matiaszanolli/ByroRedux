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
use crate::blocks::properties::NiStringPalette;
use crate::blocks::interpolator::{
    FloatKey, KeyGroup, KeyType, NiBSplineBasisData, NiBSplineCompTransformInterpolator,
    NiBSplineData, NiBlendBoolInterpolator, NiBlendFloatInterpolator, NiBlendInterpolator,
    NiBlendPoint3Interpolator, NiBlendTransformInterpolator, NiBoolInterpolator, NiColorData,
    NiColorInterpolator, NiFloatData, NiFloatInterpolator, NiLookAtInterpolator,
    NiPoint3Interpolator, NiPosData, NiTransformData, NiTransformInterpolator, Vec3Key,
};
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
        BsShaderController, NiMaterialColorController, NiSingleInterpController,
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
            } else if let Some(c) = any.downcast_ref::<BsShaderController>() {
                c.base.base.next_controller_ref
            } else if let Some(c) = any.downcast_ref::<NiMaterialColorController>() {
                c.base.next_controller_ref
            } else if let Some(c) =
                any.downcast_ref::<crate::blocks::controller::NiUVController>()
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
                        if let Some(ch) = extract_float_channel_at(scene, idx, FloatTarget::Alpha)
                        {
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
                "NiUVController" => {
                    // The NiUVController + NiUVData path is distinct from
                    // the NiTextureTransformController: UVData stores four
                    // independent float-key groups (offsetU, offsetV,
                    // scaleU, scaleV). Emit up to four channels per host
                    // node, each with its own target. See #154.
                    if let Some(c) =
                        any.downcast_ref::<crate::blocks::controller::NiUVController>()
                    {
                        if let Some(data_idx) = c.data_ref.index() {
                            if let Some(data) = scene
                                .get_as::<crate::blocks::interpolator::NiUVData>(data_idx)
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
fn resolve_cb_string(
    scene: &NifScene,
    cb: &ControlledBlock,
    which: CbString,
) -> Option<Arc<str>> {
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
        .filter_map(|it| it.interpolator_ref.index().map(|i| (i, it.normalized_weight)))
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

    None
}

/// Build a single-key `TransformChannel` from a static `NiQuatTransform`.
/// Used by `NiLookAtInterpolator` (and any future block that exposes a
/// pose without keyframe data) to surface the documented fall-back pose
/// instead of dropping the channel.
fn constant_transform_channel(t: &crate::types::NiQuatTransform) -> TransformChannel {
    let translation = zup_to_yup_pos([t.translation.x, t.translation.y, t.translation.z]);
    let rotation = zup_to_yup_quat(t.rotation);
    TransformChannel {
        translation_keys: vec![TranslationKey {
            time: 0.0,
            value: translation,
            forward: [0.0; 3],
            backward: [0.0; 3],
            tbc: None,
        }],
        translation_type: KeyType::Linear,
        rotation_keys: vec![RotationKey {
            time: 0.0,
            value: rotation,
            tbc: None,
        }],
        rotation_type: KeyType::Linear,
        scale_keys: vec![ScaleKey {
            time: 0.0,
            value: t.scale,
            forward: 0.0,
            backward: 0.0,
            tbc: None,
        }],
        scale_type: KeyType::Linear,
        priority: 0,
    }
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

        // Translation
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
            translation_keys.push(TranslationKey {
                time: t,
                value: zup_to_yup_pos([
                    interp.transform.translation.x,
                    interp.transform.translation.y,
                    interp.transform.translation.z,
                ]),
                forward: [0.0, 0.0, 0.0],
                backward: [0.0, 0.0, 0.0],
                tbc: None,
            });
        }

        // Rotation — normalize after sampling since the B-spline doesn't
        // enforce unit length on quaternions.
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
            rotation_keys.push(RotationKey {
                time: t,
                value: zup_to_yup_quat(interp.transform.rotation),
                tbc: None,
            });
        }

        // Scale
        if let Some(ref cps) = scale_q {
            let p = deboor_cubic(cps, n_cp, BSPLINE_SCALE_STRIDE, u);
            scale_keys.push(ScaleKey {
                time: t,
                value: p[0],
                forward: 0.0,
                backward: 0.0,
                tbc: None,
            });
        } else {
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
/// fallback `NiQuatTransform`.
fn static_transform_channel(interp: &NiBSplineCompTransformInterpolator) -> TransformChannel {
    TransformChannel {
        translation_keys: vec![TranslationKey {
            time: interp.start_time,
            value: zup_to_yup_pos([
                interp.transform.translation.x,
                interp.transform.translation.y,
                interp.transform.translation.z,
            ]),
            forward: [0.0, 0.0, 0.0],
            backward: [0.0, 0.0, 0.0],
            tbc: None,
        }],
        translation_type: KeyType::Linear,
        rotation_keys: vec![RotationKey {
            time: interp.start_time,
            value: zup_to_yup_quat(interp.transform.rotation),
            tbc: None,
        }],
        rotation_type: KeyType::Linear,
        scale_keys: vec![ScaleKey {
            time: interp.start_time,
            value: interp.transform.scale,
            forward: 0.0,
            backward: 0.0,
            tbc: None,
        }],
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
    let interp = scene.get_as::<NiFloatInterpolator>(interp_idx)?;
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
    Some(FloatChannel { target, keys })
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
    Vec::new()
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
mod tests {
    use super::*;

    #[test]
    fn resolve_morph_target_index_by_name() {
        use crate::blocks::controller::{MorphTarget, NiTimeControllerBase};
        use crate::types::BlockRef;

        // Build a scene with: [0] NiMorphData (3 named targets), [1] NiGeomMorpherController.
        let morph_data = NiMorphData {
            num_vertices: 0,
            relative_targets: 0,
            morphs: vec![
                MorphTarget {
                    name: Some(Arc::from("Blink")),
                    vectors: vec![],
                },
                MorphTarget {
                    name: Some(Arc::from("JawOpen")),
                    vectors: vec![],
                },
                MorphTarget {
                    name: Some(Arc::from("BrowUp")),
                    vectors: vec![],
                },
            ],
        };
        let morpher = NiGeomMorpherController {
            base: NiTimeControllerBase {
                next_controller_ref: BlockRef::NULL,
                flags: 0,
                frequency: 1.0,
                phase: 0.0,
                start_time: 0.0,
                stop_time: 1.0,
                target_ref: BlockRef::NULL,
            },
            morpher_flags: 0,
            data_ref: BlockRef(0),
            always_update: 0,
            interpolator_weights: vec![],
        };
        let scene = NifScene {
            blocks: vec![Box::new(morph_data), Box::new(morpher)],
            ..NifScene::default()
        };

        // Controlled block pointing at the morpher with controller_id = "JawOpen".
        let mut cb = dummy_controlled_block();
        cb.controller_ref = BlockRef(1);
        cb.controller_id = Some(Arc::from("JawOpen"));
        assert_eq!(resolve_morph_target_index(&scene, &cb), Some(1));

        // Case-insensitive match.
        cb.controller_id = Some(Arc::from("blink"));
        assert_eq!(resolve_morph_target_index(&scene, &cb), Some(0));

        // Missing name returns None (caller falls back to 0).
        cb.controller_id = Some(Arc::from("NotARealMorph"));
        assert_eq!(resolve_morph_target_index(&scene, &cb), None);

        // Null controller_ref returns None.
        cb.controller_ref = BlockRef::NULL;
        assert_eq!(resolve_morph_target_index(&scene, &cb), None);
    }

    fn dummy_controlled_block() -> ControlledBlock {
        ControlledBlock {
            interpolator_ref: crate::types::BlockRef::NULL,
            controller_ref: crate::types::BlockRef::NULL,
            priority: 0,
            node_name: None,
            property_type: None,
            controller_type: None,
            controller_id: None,
            interpolator_id: None,
            string_palette_ref: crate::types::BlockRef::NULL,
            node_name_offset: 0,
            property_type_offset: 0,
            controller_type_offset: 0,
            controller_id_offset: 0,
            interpolator_id_offset: 0,
        }
    }

    /// Regression: #334 (AR-08). A ControlledBlock pointing at a
    /// NiBlendTransformInterpolator must still produce a transform
    /// channel — the resolver picks the dominant sub-interpolator
    /// (highest normalized_weight) and the extractor recurses into it.
    /// Pre-fix the extractor returned None on the blend type and
    /// multi-sequence NPC animations silently lost every channel.
    #[test]
    fn extract_transform_channel_follows_blend_to_dominant_sub_interp() {
        use crate::blocks::interpolator::{
            InterpBlendItem, KeyGroup, NiBlendInterpolator, NiBlendTransformInterpolator,
            NiTransformData, NiTransformInterpolator,
        };
        use crate::types::{BlockRef, NiQuatTransform};

        // Scene layout:
        //   [0] NiTransformData (dominant — carries a single scale key)
        //   [1] NiTransformData (secondary — empty)
        //   [2] NiTransformInterpolator referencing [0]
        //   [3] NiTransformInterpolator referencing [1]
        //   [4] NiBlendTransformInterpolator with items [2]@0.8 + [3]@0.2
        let empty_floats = KeyGroup::<FloatKey> {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        };
        let empty_vec3s = KeyGroup::<Vec3Key> {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        };
        let dominant_data = NiTransformData {
            rotation_type: None,
            rotation_keys: Vec::new(),
            xyz_rotations: None,
            translations: empty_vec3s.clone(),
            scales: KeyGroup {
                key_type: KeyType::Linear,
                keys: vec![FloatKey {
                    time: 0.0,
                    value: 1.5,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                }],
            },
        };
        let secondary_data = NiTransformData {
            rotation_type: None,
            rotation_keys: Vec::new(),
            xyz_rotations: None,
            translations: empty_vec3s,
            scales: empty_floats,
        };
        let dom_interp = NiTransformInterpolator {
            transform: NiQuatTransform::default(),
            data_ref: BlockRef(0),
        };
        let sec_interp = NiTransformInterpolator {
            transform: NiQuatTransform::default(),
            data_ref: BlockRef(1),
        };
        let blend = NiBlendTransformInterpolator {
            base: NiBlendInterpolator {
                flags: 0, // not manager-controlled, so items is live
                array_size: 2,
                weight_threshold: 0.0,
                manager_controlled: false,
                interp_count: 2,
                single_index: 0,
                items: vec![
                    InterpBlendItem {
                        interpolator_ref: BlockRef(2),
                        weight: 0.8,
                        normalized_weight: 0.8,
                        priority: 0,
                        ease_spinner: 0.0,
                    },
                    InterpBlendItem {
                        interpolator_ref: BlockRef(3),
                        weight: 0.2,
                        normalized_weight: 0.2,
                        priority: 0,
                        ease_spinner: 0.0,
                    },
                ],
            },
        };
        let scene = NifScene {
            blocks: vec![
                Box::new(dominant_data),
                Box::new(secondary_data),
                Box::new(dom_interp),
                Box::new(sec_interp),
                Box::new(blend),
            ],
            ..NifScene::default()
        };

        let mut cb = dummy_controlled_block();
        cb.interpolator_ref = BlockRef(4); // point at the blend

        let channel = extract_transform_channel(&scene, &cb)
            .expect("blend transform interpolator must resolve to the dominant sub-interp");
        assert_eq!(channel.scale_keys.len(), 1, "must reach dominant data's scales");
        assert!((channel.scale_keys[0].value - 1.5).abs() < 1e-6);
    }

    /// #604 — NiLookAtInterpolator must produce a constant TransformChannel
    /// from its static `transform` pose instead of returning None. Pre-fix
    /// the dispatch had no third branch and embedded look-at chains in
    /// FNV / SkyrimSE silently dropped every channel.
    #[test]
    fn extract_transform_channel_emits_constant_pose_for_lookat() {
        use crate::types::{BlockRef, NiPoint3, NiQuatTransform};

        // Static pose with a 90° rotation around Z-up Z (= around Y-up Y
        // after coord conversion). Translation + scale are both
        // non-default so the test catches a coord-handling regression on
        // any field.
        let half = std::f32::consts::FRAC_1_SQRT_2; // sin(45°) = cos(45°)
        let zup_quat = [half, 0.0, 0.0, half]; // (w, x, y, z) = 90° around +Z
        let pose = NiQuatTransform {
            translation: NiPoint3 { x: 1.0, y: 2.0, z: 3.0 },
            rotation: zup_quat,
            scale: 0.75,
        };
        let lookat = NiLookAtInterpolator {
            flags: 0,
            look_at: BlockRef::NULL,
            look_at_name: None,
            transform: pose,
            interp_translation: BlockRef::NULL,
            interp_roll: BlockRef::NULL,
            interp_scale: BlockRef::NULL,
        };
        let scene = NifScene {
            blocks: vec![Box::new(lookat)],
            ..NifScene::default()
        };

        let mut cb = dummy_controlled_block();
        cb.interpolator_ref = BlockRef(0);

        let channel = extract_transform_channel(&scene, &cb)
            .expect("NiLookAtInterpolator must emit a constant transform channel");
        assert_eq!(channel.translation_keys.len(), 1);
        assert_eq!(channel.rotation_keys.len(), 1);
        assert_eq!(channel.scale_keys.len(), 1);

        // Translation Z-up → Y-up: (1, 2, 3) → (1, 3, -2).
        let t = channel.translation_keys[0].value;
        assert!((t[0] - 1.0).abs() < 1e-6);
        assert!((t[1] - 3.0).abs() < 1e-6);
        assert!((t[2] + 2.0).abs() < 1e-6);

        // Rotation: Z-up (w,x,y,z) = (√2/2, 0, 0, √2/2) → glam (x,y,z,w)
        // via zup_to_yup_quat = (0, √2/2, 0, √2/2).
        let r = channel.rotation_keys[0].value;
        assert!(r[0].abs() < 1e-6);
        assert!((r[1] - half).abs() < 1e-6);
        assert!(r[2].abs() < 1e-6);
        assert!((r[3] - half).abs() < 1e-6);

        // Scale passes through unchanged.
        assert!((channel.scale_keys[0].value - 0.75).abs() < 1e-6);

        // Time stamps default to 0 — single-key constant channel.
        assert_eq!(channel.translation_keys[0].time, 0.0);
        assert_eq!(channel.rotation_keys[0].time, 0.0);
        assert_eq!(channel.scale_keys[0].time, 0.0);
    }

    /// The resolver picks the item with the HIGHEST normalized_weight.
    /// Ties go to either item (we pick deterministically via
    /// `max_by` → first-max-wins-in-iteration-order) but the test
    /// asserts the non-tied case explicitly.
    #[test]
    fn resolve_blend_picks_highest_normalized_weight() {
        use crate::blocks::interpolator::{
            InterpBlendItem, NiBlendInterpolator, NiBlendTransformInterpolator,
        };
        use crate::types::BlockRef;

        let blend = NiBlendTransformInterpolator {
            base: NiBlendInterpolator {
                flags: 0,
                array_size: 3,
                weight_threshold: 0.0,
                manager_controlled: false,
                interp_count: 3,
                single_index: 0,
                items: vec![
                    InterpBlendItem {
                        interpolator_ref: BlockRef(10),
                        weight: 0.1,
                        normalized_weight: 0.1,
                        priority: 0,
                        ease_spinner: 0.0,
                    },
                    InterpBlendItem {
                        interpolator_ref: BlockRef(20),
                        weight: 0.9,
                        normalized_weight: 0.9, // dominant
                        priority: 0,
                        ease_spinner: 0.0,
                    },
                    InterpBlendItem {
                        interpolator_ref: BlockRef(30),
                        weight: 0.3,
                        normalized_weight: 0.3,
                        priority: 0,
                        ease_spinner: 0.0,
                    },
                ],
            },
        };
        let scene = NifScene {
            blocks: vec![Box::new(blend)],
            ..NifScene::default()
        };
        assert_eq!(resolve_blend_interpolator_target(&scene, 0), Some(20));
    }

    /// Manager-controlled blend (flag bit 0) has an empty `items`
    /// array — sub-interpolators are driven externally by the
    /// NiControllerManager via sibling ControlledBlocks. Resolver
    /// returns None so the caller cleanly produces no channel; the
    /// manager's other sequences supply the data through their own
    /// interpolator_refs.
    #[test]
    fn resolve_blend_returns_none_for_manager_controlled() {
        use crate::blocks::interpolator::{NiBlendInterpolator, NiBlendTransformInterpolator};

        let blend = NiBlendTransformInterpolator {
            base: NiBlendInterpolator {
                flags: 1, // bit 0 = manager_controlled
                array_size: 0,
                weight_threshold: 0.0,
                manager_controlled: true,
                interp_count: 0,
                single_index: 0,
                items: Vec::new(),
            },
        };
        let scene = NifScene {
            blocks: vec![Box::new(blend)],
            ..NifScene::default()
        };
        assert_eq!(resolve_blend_interpolator_target(&scene, 0), None);
    }

    /// Non-blend interpolators must not be touched by the resolver —
    /// it returns None so the caller falls through to the direct path.
    #[test]
    fn resolve_blend_returns_none_for_non_blend_interpolator() {
        use crate::blocks::interpolator::NiTransformInterpolator;
        use crate::types::{BlockRef, NiQuatTransform};

        let interp = NiTransformInterpolator {
            transform: NiQuatTransform::default(),
            data_ref: BlockRef::NULL,
        };
        let scene = NifScene {
            blocks: vec![Box::new(interp)],
            ..NifScene::default()
        };
        assert_eq!(resolve_blend_interpolator_target(&scene, 0), None);
    }

    /// Regression: #402. Oblivion-era `NiControllerSequence` blocks
    /// reference their node/controller strings through an
    /// `NiStringPalette` + byte offsets rather than the modern header
    /// string table. Before the fix, `resolve_cb_string` returned None
    /// for palette-backed ControlledBlocks → every `cb.node_name` guard
    /// in `import_sequence` short-circuited → zero clips imported on
    /// every Oblivion KF. This test builds a minimal scene with a
    /// palette-backed transform ControlledBlock and asserts the
    /// resolver returns the expected string.
    #[test]
    fn resolve_cb_string_reads_oblivion_palette() {
        use crate::blocks::properties::NiStringPalette;
        use crate::types::BlockRef;

        let palette = NiStringPalette {
            palette: "Bip01\0NiTransformController\0".to_string(),
        };
        let scene = NifScene {
            blocks: vec![Box::new(palette)],
            ..NifScene::default()
        };
        let mut cb = dummy_controlled_block();
        cb.string_palette_ref = BlockRef(0);
        cb.node_name_offset = 0;
        cb.controller_type_offset = 6;

        let node = resolve_cb_string(&scene, &cb, CbString::NodeName)
            .expect("palette-backed node_name must resolve");
        assert_eq!(&*node, "Bip01");
        let ctrl = resolve_cb_string(&scene, &cb, CbString::ControllerType)
            .expect("palette-backed controller_type must resolve");
        assert_eq!(&*ctrl, "NiTransformController");
    }

    /// #402 sibling: modern string-table-backed ControlledBlocks (Skyrim+
    /// and FNV) still resolve through the inline `Option<Arc<str>>`
    /// path. This makes sure the palette fallback doesn't shadow the
    /// fast path.
    #[test]
    fn resolve_cb_string_prefers_inline_when_present() {
        let scene = NifScene::default();
        let mut cb = dummy_controlled_block();
        cb.node_name = Some(Arc::from("Bip01 Head"));
        // Palette offset would point at a completely different string,
        // but the inline field takes precedence.
        cb.node_name_offset = 42;

        let node = resolve_cb_string(&scene, &cb, CbString::NodeName)
            .expect("inline name must win over palette fallback");
        assert_eq!(&*node, "Bip01 Head");
    }

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
        let scene = NifScene::default();
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
        assert!(
            (w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5,
            "quaternion should be unit"
        );
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
                FloatKey {
                    time: 0.0,
                    value: 0.0,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
                FloatKey {
                    time: 1.0,
                    value: 1.0,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
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
            keys: vec![FloatKey {
                time: 0.5,
                value: 42.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            }],
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
                FloatKey {
                    time: 0.0,
                    value: 0.0,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
                FloatKey {
                    time: 1.0,
                    value: FRAC_PI_2,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
            ],
        };
        let empty_keys = KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                FloatKey {
                    time: 0.0,
                    value: 0.0,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
                FloatKey {
                    time: 1.0,
                    value: 0.0,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
            ],
        };

        let data = NiTransformData {
            rotation_type: Some(KeyType::XyzRotation),
            rotation_keys: Vec::new(),
            xyz_rotations: Some([x_keys, empty_keys.clone(), empty_keys]),
            translations: KeyGroup {
                key_type: KeyType::Linear,
                keys: Vec::new(),
            },
            scales: KeyGroup {
                key_type: KeyType::Linear,
                keys: Vec::new(),
            },
        };

        let (keys, key_type) = convert_xyz_euler_keys(&data);
        assert_eq!(key_type, KeyType::Linear);
        assert_eq!(
            keys.len(),
            2,
            "should have 2 rotation keys (one per unique timestamp)"
        );

        // First key (t=0): identity → after Z-up to Y-up, glam format (x, y, z, w)
        let k0 = &keys[0];
        assert!((k0.time).abs() < 1e-5);
        // Identity quat in glam: (0, 0, 0, 1)
        assert!(
            (k0.value[3] - 1.0).abs() < 1e-4,
            "w should be ~1 for identity: {:?}",
            k0.value
        );

        // Second key (t=1): 90° around X in Z-up, then converted to Y-up
        let k1 = &keys[1];
        assert!((k1.time - 1.0).abs() < 1e-5);
        // Should be a unit quaternion
        let len_sq = k1.value.iter().map(|v| v * v).sum::<f32>();
        assert!(
            (len_sq - 1.0).abs() < 1e-4,
            "quaternion should be unit: {:?}",
            k1.value
        );
    }

    // ── B-spline evaluator tests (issue #155) ──────────────────────────

    #[test]
    fn bspline_dequant_midpoint() {
        // raw=0 → offset; raw=32767 → offset + half_range; raw=-32767 → offset - half_range
        assert!((dequant(0, 10.0, 5.0) - 10.0).abs() < 1e-5);
        assert!((dequant(32767, 10.0, 5.0) - 15.0).abs() < 1e-4);
        assert!((dequant(-32767, 10.0, 5.0) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn deboor_cubic_clamped_endpoints() {
        // With 4 control points on a single-scalar channel, the cubic
        // B-spline at u=0 should equal CP[0], at u=1 should equal CP[3]
        // because an open uniform knot vector is fully clamped at both
        // ends for the minimum degree-3 case.
        let cps = vec![1.0, 2.0, 3.0, 10.0];
        let v0 = deboor_cubic(&cps, 4, 1, 0.0);
        let v1 = deboor_cubic(&cps, 4, 1, 1.0);
        assert!(
            (v0[0] - 1.0).abs() < 1e-4,
            "u=0 should give CP[0], got {}",
            v0[0]
        );
        assert!(
            (v1[0] - 10.0).abs() < 1e-4,
            "u=1 should give CP[3], got {}",
            v1[0]
        );
    }

    #[test]
    fn deboor_cubic_monotone_between_endpoints() {
        // With a monotone CP sequence and a monotone knot parameter,
        // the evaluated curve should also be monotone (not strictly,
        // but the sign of successive differences should agree).
        let cps = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let n = 5;
        let u_max = (n - BSPLINE_DEGREE) as f32;
        let mut prev = f32::NEG_INFINITY;
        for i in 0..=10 {
            let u = u_max * (i as f32 / 10.0);
            let v = deboor_cubic(&cps, n, 1, u)[0];
            assert!(
                v >= prev - 1e-4,
                "non-monotone: v[{}] = {} < prev {}",
                i,
                v,
                prev
            );
            prev = v;
        }
    }

    #[test]
    fn bspline_channel_slice_invalid_handle() {
        let raw: Vec<i16> = vec![0; 100];
        assert!(channel_slice(u32::MAX, &raw, 4, 3, 0.0, 1.0).is_none());
    }

    #[test]
    fn bspline_channel_slice_out_of_bounds() {
        let raw: Vec<i16> = vec![0; 10];
        // Needs 4 * 3 = 12 slots starting at handle 0 → should fail (only 10).
        assert!(channel_slice(0, &raw, 4, 3, 0.0, 1.0).is_none());
    }

    #[test]
    fn bspline_channel_slice_dequantizes() {
        // 4 CPs × stride 1, raw values [0, 32767, -32767, 0]
        // with offset=10, half_range=5 → [10, 15, 5, 10]
        let raw: Vec<i16> = vec![0, 32767, -32767, 0];
        let out = channel_slice(0, &raw, 4, 1, 10.0, 5.0).unwrap();
        assert_eq!(out.len(), 4);
        assert!((out[0] - 10.0).abs() < 1e-4);
        assert!((out[1] - 15.0).abs() < 1e-4);
        assert!((out[2] - 5.0).abs() < 1e-4);
        assert!((out[3] - 10.0).abs() < 1e-4);
    }

    /// Regression: #261. A NiNode with a `NiTextureTransformController`
    /// on `controller_ref` must surface as a looping `AnimationClip`
    /// carrying a `FloatTarget::UvOffsetU` channel keyed by the node
    /// name. Pre-fix the controller_ref was dropped on the floor during
    /// import — water/lava meshes rendered static.
    #[test]
    fn import_embedded_animations_captures_texture_transform_controller() {
        use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
        use crate::blocks::controller::{NiTextureTransformController, NiTimeControllerBase};
        use crate::blocks::interpolator::{FloatKey, KeyGroup, KeyType};
        use crate::blocks::node::NiNode;
        use crate::types::{BlockRef, NiTransform};
        use std::sync::Arc;

        // Scene layout:
        //   [0] NiFloatData (two linear keys, value 0→0.5 over 1 s)
        //   [1] NiFloatInterpolator → [0]
        //   [2] NiTextureTransformController → interp=[1], operation=0 (UvOffsetU)
        //   [3] NiNode (name="WaterPlane") with controller_ref=[2]
        let data = NiFloatData {
            keys: KeyGroup {
                key_type: KeyType::Linear,
                keys: vec![
                    FloatKey {
                        time: 0.0,
                        value: 0.0,
                        tangent_forward: 0.0,
                        tangent_backward: 0.0,
                        tbc: None,
                    },
                    FloatKey {
                        time: 1.0,
                        value: 0.5,
                        tangent_forward: 0.0,
                        tangent_backward: 0.0,
                        tbc: None,
                    },
                ],
            },
        };
        let interp = NiFloatInterpolator {
            value: 0.0,
            data_ref: BlockRef(0),
        };
        let ctrl = NiTextureTransformController {
            base: NiTimeControllerBase {
                next_controller_ref: BlockRef::NULL,
                flags: 0,
                frequency: 1.0,
                phase: 0.0,
                start_time: 0.0,
                stop_time: 1.0,
                target_ref: BlockRef::NULL,
            },
            interpolator_ref: BlockRef(1),
            shader_map: false,
            texture_slot: 0,
            operation: 0, // UvOffsetU
        };
        let node = NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from("WaterPlane")),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef(2),
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        };
        let scene = NifScene {
            blocks: vec![
                Box::new(data),
                Box::new(interp),
                Box::new(ctrl),
                Box::new(node),
            ],
            ..NifScene::default()
        };

        let clip = import_embedded_animations(&scene).expect("expected embedded clip");
        assert_eq!(clip.cycle_type, CycleType::Loop);
        assert!((clip.frequency - 1.0).abs() < 1e-6);
        assert!((clip.duration - 1.0).abs() < 1e-6);
        assert_eq!(
            clip.float_channels.len(),
            1,
            "exactly one float channel expected"
        );
        let (node_name, ch) = &clip.float_channels[0];
        assert_eq!(&**node_name, "WaterPlane");
        assert!(
            matches!(ch.target, FloatTarget::UvOffsetU),
            "expected UvOffsetU, got {:?}",
            ch.target
        );
        assert_eq!(ch.keys.len(), 2);
        assert!((ch.keys[1].value - 0.5).abs() < 1e-6);
    }

    /// Regression: #261. A NiNode with no `controller_ref` must
    /// produce no clip — import_embedded_animations returns None and
    /// the scene loader skips the AnimationPlayer spawn.
    #[test]
    fn import_embedded_animations_returns_none_when_no_controllers() {
        use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
        use crate::blocks::node::NiNode;
        use crate::types::{BlockRef, NiTransform};
        use std::sync::Arc;

        let node = NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from("StaticCrate")),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        };
        let scene = NifScene {
            blocks: vec![Box::new(node)],
            ..NifScene::default()
        };

        assert!(
            import_embedded_animations(&scene).is_none(),
            "no-controller scene must yield no clip"
        );
    }
}
