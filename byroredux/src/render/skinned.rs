//! Skinned-mesh palette pass — extracted from `build_render_data` per #1115.
//!
//! M29.6: walks `SkinnedMesh` entities and writes per-bone raw world
//! transforms into the sparse `bone_world_out` array at each entity's
//! persistent slot offset (stable across frames; assigned by
//! [`SkinSlotPool`]). Fresh slots queue a first-sight `bind_inverses`
//! upload onto the pool's pending list, which the renderer drains in
//! `draw_frame` and writes to the persistent `bind_inverses` SSBO. The
//! GPU `skin_palette.comp` does the per-slot
//! `palette[i] = bone_world[i] * bind_inverses[i]` multiply.
//!
//! Both queries are read-only (`GlobalTransform` lookup + `SkinnedMesh`
//! iteration), so two separate read queries give the correct lock
//! pattern — the previous `query_2_mut::<GT, SkinnedMesh>` took an
//! unnecessary write lock on SkinnedMesh. See #246.
//!
//! Pre-M29.6 `bone_world_out` was packed densely by iteration order
//! (slot offsets unstable across frames). M29.6 promotes it to a
//! sparse layout indexed by `slot_id × MAX_BONES_PER_MESH`; idle slots
//! retain stale data but no entity references them. The dispatch
//! covers `(max_used_slot + 1) × MAX_BONES_PER_MESH` slots — stale-slot
//! palettes are written but never read.

use std::collections::HashMap;
use std::sync::Once;

use byroredux_core::ecs::{
    resources::SkinSlotPool, EntityId, GlobalTransform, SkinnedMesh, World, MAX_BONES_PER_MESH,
};

/// M41.0 Phase 1b.x followup — frame-gated dump of any bone whose
/// entity resolved to `None` or whose `GlobalTransform` query returned
/// `None` (both fall back to `Mat4::IDENTITY` in the bone_world slot).
/// These cases produce the long-thin-ribbon vertex artifact: vertices
/// weighted to the IDENTITY slot land at NIF skin-space coords while
/// vertices weighted to well-resolved slots land at world coords, and
/// triangles span the gap.
static SKIN_DROPOUT_DUMPED: Once = Once::new();

const IDENTITY_4X4: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Walk every `SkinnedMesh` entity and write per-bone world transforms
/// into `bone_world_out` at the entity's persistent slot offset. Fresh
/// slots get their `bind_inverses` pushed onto the pool's pending
/// upload queue (drained by the renderer in `draw_frame`).
///
/// On exit:
/// - `bone_world_out` is sized to `(pool.max_used_slot() + 1) × MAX_BONES_PER_MESH`
///   and contains: slot 0 = identity (caller-seeded), allocated slots
///   filled with per-bone world matrices, unused slots filled with
///   identity (padded by this fn).
/// - `skin_offsets[entity] = slot_id × MAX_BONES_PER_MESH` for every
///   `SkinnedMesh` entity that successfully allocated a slot.
/// - `pool.pending_uploads` contains entries for entities whose slots
///   were freshly allocated this frame.
///
/// `frame_count` drives the pool's `mark_seen` for steady-state
/// entries and the dropout-debug-dump gate (≥ 60).
pub(super) fn build_skinned_palettes(
    world: &World,
    frame_count: u64,
    bone_world_out: &mut Vec<[[f32; 4]; 4]>,
    skin_offsets: &mut HashMap<EntityId, u32>,
    pool: &mut SkinSlotPool,
) {
    let gt_q = world.query::<GlobalTransform>();
    let skin_q = world.query::<SkinnedMesh>();
    let (Some(gt_q), Some(skin_q)) = (gt_q, skin_q) else {
        return;
    };

    // Pass 1: pool allocation + dropout scan. Reads each
    // SkinnedMesh, ensures it has a slot, marks it seen, records the
    // slot offset on `skin_offsets`. Counts IDENTITY-dropouts (bone
    // entity unresolved) for the debug dump.
    let mut total_dropouts: u32 = 0;
    let mut sample_entity: Option<(EntityId, u32, u32)> = None; // (entity, dropouts, bone_count)
    for (entity, skin) in skin_q.iter() {
        let Some(slot) = pool.allocate(entity, frame_count) else {
            // Pool full — entity rendered in bind pose this frame
            // (skin_offsets stays unset; static_meshes draw loop
            // falls through to bone_offset = 0 = identity slot).
            continue;
        };
        skin_offsets.insert(entity, slot * (MAX_BONES_PER_MESH as u32));

        if cfg!(debug_assertions) && frame_count >= 60 {
            let mut dropout_count: u32 = 0;
            for bone_entity in &skin.bones {
                let resolved = match bone_entity {
                    Some(e) => gt_q.get(*e).is_some(),
                    None => false,
                };
                if !resolved {
                    dropout_count += 1;
                }
            }
            if dropout_count > 0 && sample_entity.is_none() {
                sample_entity = Some((entity, dropout_count, skin.bones.len() as u32));
            }
            total_dropouts += dropout_count;
        }
    }

    if cfg!(debug_assertions) && frame_count >= 60 && total_dropouts > 0 {
        if let Some((entity, dropouts, bone_count)) = sample_entity {
            SKIN_DROPOUT_DUMPED.call_once(|| {
                log::warn!(
                    "Phase 1b.x DROPOUT — sample skinned mesh entity {:?}: {} of {} bones \
                     unresolved (frame {}, total dropouts this frame: {}). Cause: bone \
                     entity was None at skin attach time, or its GlobalTransform query \
                     returned None.",
                    entity,
                    dropouts,
                    bone_count,
                    frame_count,
                    total_dropouts,
                );
            });
        }
    }

    // Pass 2: resize bone_world_out to cover slots 0..=max_used_slot
    // and fill every slot with identity (caller seeded slot 0 already;
    // we fill from current length up to the required size).
    let required_slots = (pool.max_used_slot() as usize + 1) * MAX_BONES_PER_MESH;
    if bone_world_out.len() < required_slots {
        bone_world_out.resize(required_slots, IDENTITY_4X4);
    }

    // Pass 3: write per-entity bone_world ranges. Skipping `take`
    // costs O(MAX_BONES_PER_MESH) per entity but the alternative
    // (random index writes) bypasses the bounds check the
    // resize+slice approach already paid for.
    //
    // #1195 / PERF-DIM7-01 — at the same time, compute a per-entity
    // FNV-1a hash over the freshly-written bone slice and feed it to
    // `pool.try_mark_pose_dirty`. Idle skinned NPCs (no bone movement
    // frame-to-frame) converge to "not dirty" on the second
    // consecutive frame; the renderer's dispatch + refit loops then
    // skip the GPU work for those entities. First-sight always hits
    // the dirty branch so the output buffer + BLAS get populated.
    //
    // `clear_pose_dirty` MUST run before this loop so each frame
    // starts with an empty dirty set (we only re-mark entities whose
    // hash actually changed).
    pool.clear_pose_dirty();
    for (entity, skin) in skin_q.iter() {
        let Some(&offset) = skin_offsets.get(&entity) else {
            continue; // pool was full for this entity (rare)
        };
        let start = offset as usize;
        let end = start + MAX_BONES_PER_MESH.min(skin.bones.len());
        for (i, bone_entity) in skin.bones.iter().enumerate().take(MAX_BONES_PER_MESH) {
            let world_mat = match bone_entity {
                Some(e) => match gt_q.get(*e) {
                    Some(gt) => gt.to_matrix(),
                    None => byroredux_core::math::Mat4::IDENTITY,
                },
                None => byroredux_core::math::Mat4::IDENTITY,
            };
            bone_world_out[start + i] = world_mat.to_cols_array_2d();
        }
        // Pad any per-mesh tail (when skin.bones.len() < MBPM) with
        // identity. The resize already filled with identity, so this
        // is a no-op; the `end` binding above is kept for clarity.
        let _ = end;

        // #1195 / PERF-DIM7-01 — hash exactly the per-entity slice
        // we just wrote. Using `end` (which clamps to the actual bone
        // count) instead of `start + MBPM` avoids hashing the
        // identity-padded tail; per-entity hash stays stable across
        // frames as long as the actual bone matrices don't change.
        let hash = pose_hash(&bone_world_out[start..end]);
        let _dirty = pool.try_mark_pose_dirty(entity, hash);
    }

    // Pass 4: sweep idle entries from the pool. min_idle =
    // MAX_FRAMES_IN_FLIGHT + 1 = 3 so a slot is only reclaimed after
    // no in-flight command buffer could reference it.
    let _freed = pool.sweep(frame_count, 3);
}

/// FNV-1a hash over the f32 bits of a bone-matrix slice. Stable across
/// frames as long as the underlying matrix values are unchanged; ~16
/// ops per matrix (~512 ops for a 32-bone NPC). At ~1 ns/op that's
/// ~0.5 µs per entity — well below the ~5 µs GPU dispatch cost it
/// avoids on idle entities. #1195 / PERF-DIM7-01.
///
/// Uses `to_bits()` rather than raw byte cast so the hash is
/// endian-independent and avoids `unsafe`. NaN matrices hash by their
/// raw bit pattern — different NaN encodings would mismatch and
/// re-trigger a dispatch (correct fail-open behaviour).
fn pose_hash(mats: &[[[f32; 4]; 4]]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut h: u64 = FNV_OFFSET;
    for mat in mats {
        for row in mat {
            for &f in row {
                h ^= f.to_bits() as u64;
                h = h.wrapping_mul(FNV_PRIME);
            }
        }
    }
    h
}

#[cfg(test)]
mod pose_hash_tests {
    use super::pose_hash;

    #[test]
    fn identical_slices_hash_identically() {
        let a = [[[1.0, 2.0, 3.0, 4.0]; 4]; 2];
        let b = [[[1.0, 2.0, 3.0, 4.0]; 4]; 2];
        assert_eq!(pose_hash(&a), pose_hash(&b));
    }

    #[test]
    fn single_bit_change_changes_hash() {
        let mut a = [[[0.0; 4]; 4]; 1];
        a[0][0][0] = 1.0;
        let mut b = a;
        b[0][0][0] = 1.000_000_1; // tiny f32 perturbation
        assert_ne!(
            pose_hash(&a),
            pose_hash(&b),
            "f32::to_bits captures sub-epsilon changes"
        );
    }

    #[test]
    fn empty_slice_yields_offset_basis() {
        // `pose_hash(&[])` should equal the FNV-1a offset basis — the
        // zero-pass case. Documents that an entity with zero bones
        // (degenerate but possible during construction) gets a
        // well-defined hash that's stable across frames.
        let h = pose_hash(&[]);
        assert_eq!(h, 0xcbf29ce484222325);
    }
}
