//! Skinned-mesh palette pass — extracted from `build_render_data` per #1115.
//!
//! M29.5 (#TBD): walks `SkinnedMesh` entities and pushes per-bone raw
//! world transforms + bind-inverse matrices into two parallel output
//! Vecs. The CPU no longer computes `palette[i] = bone_world × bind_inverses[i]`
//! — the GPU `skin_palette.comp` pass does that multiply on the
//! uploaded inputs and writes the existing palette SSBO. Downstream
//! consumers (`skin_vertices.comp` for RT, `triangle.vert:147-204`
//! inline-skinning for raster) are unchanged.
//!
//! Both queries are read-only (`GlobalTransform` lookup + `SkinnedMesh`
//! iteration), so two separate read queries give the correct lock
//! pattern — the previous `query_2_mut::<GT, SkinnedMesh>` took an
//! unnecessary write lock on SkinnedMesh. See #246.
//!
//! Every skinned mesh reserves exactly `MAX_BONES_PER_MESH` slots in
//! BOTH output Vecs so per-mesh bone-offset arithmetic stays trivial.
//! The two Vecs are kept strictly parallel — `bone_world_out[k]`
//! corresponds to `bind_inverses_out[k]` for every k, and the compute
//! shader's per-slot multiply does `palette[k] = bone_world[k] *
//! bind_inverses[k]`. Drift would silently corrupt every skinned vertex.

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
/// whose bone entity resolved to `None` or whose `GlobalTransform`
/// query returned `None` (both fall back to `Mat4::IDENTITY` in the
/// bone_world slot). Both cases produce the long-thin-ribbon vertex
/// artifact: identity-world × bind_inverse gives identity-palette,
/// vertices weighted to that slot land at NIF skin-space coords while
/// vertices weighted to well-resolved slots land at world coords, and
/// triangles span the gap.
///
/// Post-M29.5 the dropout is detected at the bone-world resolution
/// site rather than after the multiply — semantically equivalent
/// (identity multiplied against any bind_inverse can't make the slot
/// non-identity except when the bind_inverse itself is non-identity,
/// in which case the artifact still surfaces visually).
static SKIN_DROPOUT_DUMPED: Once = Once::new();

const IDENTITY_4X4: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Compute every skinned mesh's bone-world + bind-inverses contributions
/// and pad each mesh to `MAX_BONES_PER_MESH`. Mutates the caller-owned
/// `bone_world_out`, `bind_inverses_out` (appending per-mesh blocks),
/// and `skin_offsets` (entity → slot).
///
/// `frame_count` gates the debug-only IDENTITY-dropout dump: must be
/// ≥ 60 for the diagnostic to fire (gives the transform propagation
/// system time to resolve bone matrices on first cell load).
///
/// **Parallel invariant**: `bone_world_out` and `bind_inverses_out`
/// must be the same length on entry (caller seeds slot 0 of both with
/// identity); the function preserves the invariant on exit so the
/// GPU's `palette[k] = bone_world[k] * bind_inverses[k]` reads stay
/// well-defined.
pub(super) fn build_skinned_palettes(
    world: &World,
    frame_count: u64,
    bone_world_out: &mut Vec<[[f32; 4]; 4]>,
    bind_inverses_out: &mut Vec<[[f32; 4]; 4]>,
    skin_offsets: &mut HashMap<EntityId, u32>,
) {
    debug_assert_eq!(
        bone_world_out.len(),
        bind_inverses_out.len(),
        "M29.5 parallel-Vec invariant: bone_world_out and bind_inverses_out \
         must stay parallel; caller seeds slot 0 of both with identity"
    );
    let gt_q = world.query::<GlobalTransform>();
    let skin_q = world.query::<SkinnedMesh>();
    let (Some(gt_q), Some(skin_q)) = (gt_q, skin_q) else {
        return;
    };
    for (entity, skin) in skin_q.iter() {
        // M29 — defensive guard against silent palette truncation.
        // `bone_buffers` are sized for `MAX_TOTAL_BONES` slots and the
        // renderer's `upload_bone_inputs` clamps writes; every skinned
        // mesh past the ceiling silently falls back to bind pose with
        // no error. Today this is unreachable for most content, but it
        // fires on populated M41 cells. Log once per session and stop
        // padding so the renderer's clamp is never reached.
        if bone_world_out.len() + MAX_BONES_PER_MESH > MAX_TOTAL_BONES {
            BONE_PALETTE_OVERFLOW_WARNED.call_once(|| {
                log::warn!(
                    "bone_palette: skinned-mesh count exceeds MAX_TOTAL_BONES={} \
                     ({} bones already pushed); remaining skinned meshes silently \
                     fall back to bind pose. Bump MAX_TOTAL_BONES or implement \
                     variable-stride packing (M29.6).",
                    MAX_TOTAL_BONES,
                    bone_world_out.len(),
                );
            });
            break;
        }
        let offset = bone_world_out.len() as u32;
        let bone_count = skin.bones.len();
        // M41.0 Phase 1b.x followup — count IDENTITY-dropouts (bone
        // entity was None at skin attach time, or its GlobalTransform
        // query returned None). The diagnostic is gated on
        // `debug_assertions` so release builds pay zero cost (#929 /
        // PERF-CPU-01); the compiler folds `cfg!(debug_assertions)` to
        // a const and DCEs the entire branch.
        let mut dropout_count: u32 = 0;
        for (bone_entity, bind_inv) in skin.bones.iter().zip(skin.bind_inverses.iter()) {
            let world_mat = match bone_entity {
                Some(e) => match gt_q.get(*e) {
                    Some(gt) => gt.to_matrix(),
                    None => {
                        if cfg!(debug_assertions) {
                            dropout_count += 1;
                        }
                        Mat4::IDENTITY
                    }
                },
                None => {
                    if cfg!(debug_assertions) {
                        dropout_count += 1;
                    }
                    Mat4::IDENTITY
                }
            };
            bone_world_out.push(world_mat.to_cols_array_2d());
            bind_inverses_out.push(bind_inv.to_cols_array_2d());
        }
        // Pad both arrays to MAX_BONES_PER_MESH with identity so the
        // dense per-slot layout stays parallel and trivial per-mesh
        // bone offsets are `offset + local_index`. Identity × identity
        // = identity, so any draw whose `bone_offset` falls in the
        // padded tail samples the identity transform.
        for _ in bone_count..MAX_BONES_PER_MESH {
            bone_world_out.push(IDENTITY_4X4);
            bind_inverses_out.push(IDENTITY_4X4);
        }

        if cfg!(debug_assertions) && frame_count >= 60 && dropout_count > 0 {
            SKIN_DROPOUT_DUMPED.call_once(|| {
                log::warn!(
                    "Phase 1b.x DROPOUT — skinned mesh entity {:?}: {} of {} bones \
                     unresolved (frame {}). Cause: bone entity was None at skin \
                     attach time, or its GlobalTransform query returned None.",
                    entity,
                    dropout_count,
                    bone_count,
                    frame_count,
                );
            });
        }

        skin_offsets.insert(entity, offset);
    }
}
