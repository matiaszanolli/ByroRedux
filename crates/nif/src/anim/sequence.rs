//! Per-sequence import (`NiControllerSequence` → `AnimationClip`).
//!
//! Routes each controlled block through the right channel extractor based on
//! its interpolator type.

use super::*;
use crate::blocks::controller::NiControllerSequence;
use crate::blocks::extra_data::{BsAnimNote, BsAnimNotes};
use crate::blocks::interpolator::NiTextKeyExtraData;
use crate::scene::NifScene;
use std::collections::HashMap;
use std::sync::Arc;

pub fn import_sequence(scene: &NifScene, seq: &NiControllerSequence) -> AnimationClip {
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

