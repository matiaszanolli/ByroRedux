//! `NiTextureEffect` extraction (#3856, split from `walk/mod.rs`).
//!
//! `walk_node_texture_effects` is an independent entry point invoked from
//! `import/mod.rs`, not from the scene-graph walkers.

use crate::scene::NifScene;
use crate::types::NiTransform;

use super::super::coord::{zup_matrix_to_yup_quat, zup_point_to_yup};
use super::super::transform::compose_transforms;
use super::node_attrs::is_editor_marker;
use super::{as_ni_node, resolve_affected_node_names, switch_active_children};

/// Recursively walk the scene graph accumulating world-space transforms
/// and collecting any `NiTextureEffect` block encountered. Mirrors
/// [`walk_node_lights`] one-for-one — the only difference is the
/// downcast type and the data captured at the leaf. See #891.
pub(crate) fn walk_node_texture_effects(
    scene: &NifScene,
    block_idx: usize,
    parent_transform: &NiTransform,
    pool: &mut byroredux_core::string::StringPool,
    out: &mut Vec<crate::import::ImportedTextureEffect>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    // NiSwitchNode / NiLODNode: only walk the active children (#718).
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        for idx in active_children {
            walk_node_texture_effects(scene, idx, &world_transform, pool, out);
        }
        return;
    }

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_texture_effects(scene, idx, &world_transform, pool, out);
            }
        }
        return;
    }

    // NiTextureEffect leaf — extract using the world transform composed
    // from the parent chain plus the effect's own local transform.
    if let Some(eff) = block
        .as_any()
        .downcast_ref::<crate::blocks::texture::NiTextureEffect>()
    {
        let world = compose_transforms(parent_transform, &eff.av.transform);
        let translation = zup_point_to_yup(&world.translation);
        let rotation = zup_matrix_to_yup_quat(&world.rotation);
        let scale = world.scale;

        // Resolve source_texture_ref → NiSourceTexture → filename →
        // interned FixedString. Same `tex_desc_source_path` shape used
        // by material slots (#609 / D6-NEW-01); centralised here rather
        // than re-importing the helper because that one takes a TexDesc.
        let texture_path = eff
            .source_texture_ref
            .index()
            .and_then(|idx| scene.get_as::<crate::blocks::texture::NiSourceTexture>(idx))
            .and_then(|src| src.filename.as_deref())
            .and_then(|name| {
                if name.is_empty() {
                    None
                } else {
                    Some(pool.intern(name))
                }
            });

        let affected_node_names = resolve_affected_node_names(scene, &eff.affected_nodes);

        out.push(crate::import::ImportedTextureEffect {
            translation,
            rotation,
            scale,
            texture_path,
            texture_type: eff.texture_type,
            coordinate_generation_type: eff.coordinate_generation_type,
            affected_node_names,
        });
    }
}
