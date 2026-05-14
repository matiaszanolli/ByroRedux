//! Public entry points for KF + embedded clip import.
//!
//! `import_kf` walks a parsed `NifScene` and emits one clip per controller
//! sequence. `import_embedded_animations` extracts the implicit animation
//! tree from a single mesh.

use super::*;
use crate::blocks::controller::{
    NiControllerManager, NiControllerSequence, NiGeomMorpherController,
};
use crate::scene::NifScene;
use std::collections::HashMap;
use std::sync::Arc;

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

pub fn clip_has_data(clip: &AnimationClip) -> bool {
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
        BsShaderController, NiFlipController, NiLightColorController, NiLightFloatController,
        NiMaterialColorController, NiSingleInterpController, NiTextureTransformController,
    };
    use crate::blocks::light::{NiAmbientLight, NiDirectionalLight, NiPointLight, NiSpotLight};
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
        // #983 — NiLight subtypes carry their controller chain on
        // the NiObjectNET inside `NiLightBase.av.net`. Walking it
        // surfaces the four `NiLight*Controller` types into the
        // embedded clip so torches / lanterns / plasma weapons get
        // their authored flicker / dim / pulse instead of emitting
        // constant light.
        if let Some(l) = any.downcast_ref::<NiPointLight>() {
            return Some(&l.base.av.net);
        }
        if let Some(l) = any.downcast_ref::<NiSpotLight>() {
            return Some(&l.point.base.av.net);
        }
        if let Some(l) = any.downcast_ref::<NiAmbientLight>() {
            return Some(&l.base.av.net);
        }
        if let Some(l) = any.downcast_ref::<NiDirectionalLight>() {
            return Some(&l.base.av.net);
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
            } else if let Some(c) = any.downcast_ref::<NiLightColorController>() {
                // #983 — NiLightColorController inherits
                // NiPoint3InterpController which is a
                // NiSingleInterpController pass-through. The
                // next_controller_ref sits on its
                // NiTimeControllerBase (one `.base` hop).
                c.base.next_controller_ref
            } else if let Some(c) = any.downcast_ref::<NiLightFloatController>() {
                // #983 — NiLightFloatController is the typed alias
                // covering Dimmer / Intensity / Radius. It's a
                // NiSingleInterpController + type_name tag; the
                // chain advance is two hops (`.base.base`) to
                // reach NiTimeControllerBase.
                c.base.base.next_controller_ref
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
                // #983 — Four NiLight*Controller types animate the
                // light's color / dimmer / intensity / radius slots.
                // All four follow the standard
                // NiSingleInterpController shape (interpolator_ref
                // → NiFloatInterpolator / NiPoint3Interpolator →
                // NiFloatData / NiPosData keys), so the extract
                // helpers used elsewhere in this match work
                // directly. See the dispatch in `blocks/mod.rs`
                // and the FloatTarget::Light* / ColorTarget::Light*
                // sinks in `core/src/animation/types.rs`.
                "NiLightColorController" => {
                    if let Some(c) = any.downcast_ref::<NiLightColorController>() {
                        if let Some(idx) = c.interpolator_ref.index() {
                            let keys = resolve_color_keys_at(scene, idx);
                            if !keys.is_empty() {
                                // target_color: 0 = Diffuse, 1 = Ambient
                                // (per nif.xml line 1241 LightColor enum).
                                let target = if c.target_color == 1 {
                                    ColorTarget::LightAmbient
                                } else {
                                    ColorTarget::LightDiffuse
                                };
                                clip.color_channels
                                    .push((Arc::clone(&node_name), ColorChannel { target, keys }));
                            }
                        }
                    }
                }
                "NiLightDimmerController"
                | "NiLightIntensityController"
                | "NiLightRadiusController" => {
                    if let Some(c) = any.downcast_ref::<NiLightFloatController>() {
                        let target = match ctrl_type {
                            "NiLightDimmerController" => FloatTarget::LightDimmer,
                            "NiLightIntensityController" => FloatTarget::LightIntensity,
                            "NiLightRadiusController" => FloatTarget::LightRadius,
                            _ => unreachable!(),
                        };
                        if let Some(idx) = c.base.interpolator_ref.index() {
                            if let Some(ch) = extract_float_channel_at(scene, idx, target) {
                                clip.float_channels.push((Arc::clone(&node_name), ch));
                            }
                        }
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
