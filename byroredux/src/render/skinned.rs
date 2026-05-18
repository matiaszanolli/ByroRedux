//! Skinned-mesh palette pass — extracted from `build_render_data` per #1115.
//!
//! Walks `SkinnedMesh` entities, computes each mesh's bone palette
//! slice via the `compute_palette_into` closure (resolving each bone's
//! world-space matrix through the `GlobalTransform` query), and
//! records `entity → bone_offset` so the static-mesh draw loop can
//! stamp it onto the matching DrawCommand. Every skinned mesh reserves
//! exactly `MAX_BONES_PER_MESH` slots so per-mesh bone-offset
//! arithmetic stays trivial.
//!
//! Both queries are read-only (the palette closure dereferences
//! `GlobalTransform::to_matrix()` and the skin iter borrows each
//! `SkinnedMesh` immutably), so two separate read queries give the
//! correct lock pattern — the previous `query_2_mut::<GT, SkinnedMesh>`
//! took an unnecessary write lock on SkinnedMesh. See #246.

use std::collections::HashMap;
use std::sync::Once;

use byroredux_core::ecs::{EntityId, GlobalTransform, SkinnedMesh, World, MAX_BONES_PER_MESH};
use byroredux_core::math::Mat4;
use byroredux_renderer::vulkan::scene_buffer::MAX_TOTAL_BONES;

/// Once-per-session gate for the bone-palette overflow warn. Keeps the
/// log out of the per-frame hot path so the warn fires exactly the
/// first time a cell's skinned-mesh count exceeds `MAX_TOTAL_BONES`.
static BONE_PALETTE_OVERFLOW_WARNED: Once = Once::new();

/// M41.0 Phase 1b.x followup — frame-gated dump of any palette slot
/// that resolved to `Mat4::IDENTITY` after propagation.
/// `compute_palette_into` returns IDENTITY when (a) the bone entity
/// was `None` at skin attach time (bone-name not in the external
/// skeleton map and not in the local `node_by_name` either) or (b)
/// the `world_transform_of` closure returned `None` (entity has no
/// `GlobalTransform`). Both cases produce the long-thin-ribbon vertex
/// artifact: vertices weighted to the IDENTITY slot land at NIF
/// skin-space coords, vertices weighted to well-resolved slots land
/// at world coords, and triangles span the gap.
static SKIN_DROPOUT_DUMPED: Once = Once::new();

/// Compute every skinned mesh's bone palette and pad to
/// `MAX_BONES_PER_MESH`. Mutates the caller-owned `bone_palette`
/// (appending per-mesh blocks), `skin_offsets` (entity → slot),
/// and `palette_scratch` (reused buffer for `compute_palette_into`;
/// cleared internally before each refill, see #509).
///
/// `frame_count` gates the debug-only IDENTITY-dropout dump: must be
/// ≥ 60 for the diagnostic to fire (gives the transform propagation
/// system time to resolve bone matrices on first cell load).
pub(super) fn build_skinned_palettes(
    world: &World,
    frame_count: u64,
    bone_palette: &mut Vec<[[f32; 4]; 4]>,
    skin_offsets: &mut HashMap<EntityId, u32>,
    palette_scratch: &mut Vec<Mat4>,
) {
    let gt_q = world.query::<GlobalTransform>();
    let skin_q = world.query::<SkinnedMesh>();
    let (Some(gt_q), Some(skin_q)) = (gt_q, skin_q) else {
        return;
    };
    for (entity, skin) in skin_q.iter() {
        // M29 — defensive guard against silent palette truncation.
        // `bone_buffers` are sized for `MAX_TOTAL_BONES` slots and the
        // renderer's `upload_bones` clamps writes
        // (scene_buffer.rs:982); every skinned mesh past the ceiling
        // silently falls back to bind pose with no error. Today this
        // is unreachable for most content, but it fires on populated
        // M41 cells. Log once per session and stop padding the
        // palette so the renderer's clamp is never reached.
        if bone_palette.len() + MAX_BONES_PER_MESH > MAX_TOTAL_BONES {
            BONE_PALETTE_OVERFLOW_WARNED.call_once(|| {
                log::warn!(
                    "bone_palette: skinned-mesh count exceeds MAX_TOTAL_BONES={} \
                     ({} bones already pushed); remaining skinned meshes silently \
                     fall back to bind pose. Bump MAX_TOTAL_BONES or implement \
                     variable-stride packing (M29.5).",
                    MAX_TOTAL_BONES,
                    bone_palette.len(),
                );
            });
            break;
        }
        let offset = bone_palette.len() as u32;
        // World-lookup closure — reads GlobalTransform for each bone
        // entity through the same query guard. Missing bones fall
        // back to identity inside compute_palette_into.
        skin.compute_palette_into(palette_scratch, |bone_entity| {
            gt_q.get(bone_entity).map(|gt| gt.to_matrix())
        });
        // M41.0 Phase 1b.x followup — flag any palette slot that
        // resolved to identity post-propagation. These slots cause
        // the ribbon-vertex artifact described on SKIN_DROPOUT_DUMPED.
        //
        // Gated on `debug_assertions` (#929 / PERF-CPU-01): the outer
        // `SKIN_DROPOUT_DUMPED.call_once` short-circuits the log after
        // the first hit, but the Vec allocation + per-bone identity
        // check still ran every frame for every skinned mesh in
        // release. The compiler folds `cfg!(debug_assertions)` to a
        // const and DCEs the entire branch in release, restoring
        // zero-cost. Debug builds (developer + CI test profile) keep
        // the diagnostic for any future regression investigation.
        if cfg!(debug_assertions) && frame_count >= 60 {
            let mut dropout_slots: Vec<(usize, bool)> = Vec::new();
            for (i, ((bone_e, _bind), pal)) in skin
                .bones
                .iter()
                .zip(skin.bind_inverses.iter())
                .zip(palette_scratch.iter())
                .enumerate()
            {
                let m = *pal;
                let is_identity = (m.x_axis - byroredux_core::math::Vec4::X).length_squared()
                    < 1e-6
                    && (m.y_axis - byroredux_core::math::Vec4::Y).length_squared() < 1e-6
                    && (m.z_axis - byroredux_core::math::Vec4::Z).length_squared() < 1e-6
                    && (m.w_axis - byroredux_core::math::Vec4::W).length_squared() < 1e-6;
                if is_identity {
                    dropout_slots.push((i, bone_e.is_none()));
                }
            }
            if !dropout_slots.is_empty() {
                SKIN_DROPOUT_DUMPED.call_once(|| {
                    log::warn!(
                        "Phase 1b.x DROPOUT — skinned mesh entity {:?}: {} of {} palette \
                         slots are IDENTITY (frame {}). Sample (slot, bone_was_None): {:?}",
                        entity,
                        dropout_slots.len(),
                        skin.bones.len(),
                        frame_count,
                        &dropout_slots[..dropout_slots.len().min(8)],
                    );
                });
            }
        }
        // Pad every skinned mesh to MAX_BONES_PER_MESH so per-mesh
        // bone offsets are trivially `offset + local_index` and the
        // shader doesn't need a per-mesh bone count.
        for mat in palette_scratch.iter() {
            bone_palette.push(mat.to_cols_array_2d());
        }
        for _ in palette_scratch.len()..MAX_BONES_PER_MESH {
            bone_palette.push([
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]);
        }
        skin_offsets.insert(entity, offset);
        let _ = entity; // silence unused if debug_assertions off
    }
}
