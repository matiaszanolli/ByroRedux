//! Cell teardown — despawn entities + free GPU resources.

use byroredux_core::ecs::components::{Inventory, ItemInstanceId};
use byroredux_core::ecs::resources::ItemInstancePool;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    MeshHandle, TextureHandle, World,
};
use byroredux_renderer::VulkanContext;
use std::collections::{HashMap, HashSet};

use crate::components::{
    CellLightingRes, CellRootIndex, DarkMapHandle, ExtraTextureMaps, NormalMapHandle,
    SkyParamsRes, TerrainTileSlot, WeatherDataRes, WeatherTransitionRes,
};

/// Tear down a cell: despawn every entity owned by `cell_root` and
/// release the mesh/BLAS/texture GPU resources they referenced.
///
/// Handles are not reused — dropped mesh/texture slots remain as
/// placeholders in the registries to guarantee that any dangling
/// `GpuInstance.mesh_id` / `texture_index` can't reappear pointing at
/// a new mesh or texture. Entity IDs likewise grow monotonically (see
/// `World::despawn` docs). See #372.
///
/// Texture handles are refcounted (#524): each `resolve_texture` acquisition
/// bumps the `TextureEntry.ref_count` inside the registry, and this
/// function calls `drop_texture` once per entity-held handle. Shared
/// textures across still-resident cells survive an unload because the
/// remaining holders keep the refcount positive. M40 doorwalking needs
/// this — without it, cell A's unload would flip cell B's shared
/// clutter textures to the checkerboard.
#[allow(dead_code)] // exposed for scripting / doorwalking wiring (M40)
#[tracing::instrument(name = "unload_cell", skip_all, fields(cell_root = ?cell_root))]
pub fn unload_cell(world: &mut World, ctx: &mut VulkanContext, cell_root: EntityId) {
    // Drain victims from the `CellRootIndex` inverted map (#791). Pre-#791
    // this filtered the entire `CellRoot` SparseSet to find victims of a
    // single cell, scaling O(total resident entities); the index makes
    // lookup O(victims). If the resource is absent (test fixtures that
    // don't register it) or the cell isn't tracked, fall through with
    // an empty victim set — `unload_cell` is idempotent.
    let victims: Vec<EntityId> = world
        .try_resource_mut::<CellRootIndex>()
        .and_then(|mut idx| idx.map.remove(&cell_root))
        .unwrap_or_default();

    // Gather mesh handles. Pre-#879 this used a `HashSet<u32>` because
    // every REFR placement uploaded its own `MeshHandle` and one
    // `drop_mesh` per unique handle was sufficient. Post-#879 the
    // refcounted dedup means N placements share one handle: each
    // holder must contribute one decrement so the registry can free
    // the GPU buffers exactly when the last placement releases.
    // Mirrors the existing texture-handle pattern (#524) which has
    // always used a Vec for the same reason.
    let mut mesh_drops: Vec<u32> = Vec::new();
    let mut texture_drops: Vec<u32> = Vec::new();
    let mut terrain_tile_slots: HashSet<u32> = HashSet::new();
    let fallback_tex = ctx.texture_registry.fallback();
    let push_tex_drop = |handle: u32, sink: &mut Vec<u32>| {
        if handle != 0 && handle != fallback_tex {
            sink.push(handle);
        }
    };
    // #883 / CELL-PERF-06 — single victim walk that fans out to
    // every per-component lookup. Pre-fix this was six independent
    // `for &eid in &victims` loops, each re-acquiring a read lock on
    // a different SparseSet header. The per-victim inner cost is
    // unchanged (still six hash lookups per entity), but the
    // SparseSet header walk happens once instead of six times.
    //
    // Holding six read locks across the walk is safe — they're
    // independent SparseSets (different component TypeIds) and
    // `unload_cell` takes `&mut World`, so no concurrent writer can
    // exist. The TypeId-sort lock-order invariant (CLAUDE.md #4) is
    // about combined read+write multi-component queries where a
    // mixed acquire order could deadlock; six pure reads have no
    // such risk.
    let mq = world.query::<MeshHandle>();
    let tq = world.query::<TextureHandle>();
    let nq = world.query::<NormalMapHandle>();
    let dq = world.query::<DarkMapHandle>();
    let eq = world.query::<ExtraTextureMaps>();
    let ttq = world.query::<TerrainTileSlot>();
    for &eid in &victims {
        if let Some(mq) = &mq {
            if let Some(mh) = mq.get(eid) {
                mesh_drops.push(mh.0);
            }
        }
        if let Some(tq) = &tq {
            if let Some(th) = tq.get(eid) {
                push_tex_drop(th.0, &mut texture_drops);
            }
        }
        if let Some(nq) = &nq {
            if let Some(nh) = nq.get(eid) {
                push_tex_drop(nh.0, &mut texture_drops);
            }
        }
        if let Some(dq) = &dq {
            if let Some(dh) = dq.get(eid) {
                push_tex_drop(dh.0, &mut texture_drops);
            }
        }
        if let Some(eq) = &eq {
            if let Some(extra) = eq.get(eid) {
                push_tex_drop(extra.glow, &mut texture_drops);
                push_tex_drop(extra.detail, &mut texture_drops);
                push_tex_drop(extra.gloss, &mut texture_drops);
                push_tex_drop(extra.parallax, &mut texture_drops);
                push_tex_drop(extra.env, &mut texture_drops);
                push_tex_drop(extra.env_mask, &mut texture_drops);
            }
        }
        if let Some(ttq) = &ttq {
            if let Some(slot) = ttq.get(eid) {
                terrain_tile_slots.insert(slot.0);
            }
        }
    }
    // Drop query guards before the texture/mesh registry mutations
    // below — those don't touch component SparseSets but releasing
    // the locks early keeps the lock-hold window scoped to the
    // walk.
    drop((mq, tq, nq, dq, eq, ttq));

    // Sky textures live on `SkyParamsRes` (a Resource), not an ECS
    // component, so the per-victim sweep above can't reach them. The
    // bindless indices were acquired via `texture_registry.load_dds`
    // (sun) and `acquire_by_path` (cloud layers) at scene load time —
    // each bumped the registry refcount once. Without symmetric drops
    // every cell-cell transition leaks 4 cloud + 1 sun texture (#626).
    // The slot list is owned by `SkyParamsRes::texture_indices` so adding
    // a new slot updates both sites in lockstep.
    if let Some(sky) = world.try_resource::<SkyParamsRes>() {
        for idx in sky.texture_indices() {
            push_tex_drop(idx, &mut texture_drops);
        }
    }
    // Cell-scoped state resources hold no texture refs but get replaced
    // on the next `world.insert_resource` at cell load — clear them on
    // unload so a between-load query doesn't see stale state.
    world.remove_resource::<SkyParamsRes>();
    world.remove_resource::<CellLightingRes>();
    world.remove_resource::<WeatherDataRes>();
    world.remove_resource::<WeatherTransitionRes>();

    // Free terrain tile slots FIRST — late frames-in-flight reading the
    // SSBO then see either stale-but-valid data (if the slot was
    // reallocated) or the same data (no reuse this frame), rather than
    // undefined. See #470.
    //
    // Each slot owns 8 layer texture refcounts that `resolve_texture`
    // bumped via `acquire_by_path` at allocation time. The slot itself
    // isn't an ECS component, so the per-victim `TextureHandle` sweep
    // above can't reach those refs; capture them from the freed slot
    // and add them to `texture_drops` so the GPU release loop below
    // hands them off to `texture_registry.drop_texture`. Without this,
    // a 7×7 WastelandNV reload leaks ~150 texture refcounts (#627).
    for &slot in &terrain_tile_slots {
        if let Some(layer_indices) = ctx.free_terrain_tile(slot) {
            for idx in layer_indices {
                push_tex_drop(idx, &mut texture_drops);
            }
        }
    }

    // Free GPU resources. With refcounted mesh dedup (#879), a handle
    // shared across N placements must receive N drops before its
    // VkBuffer is freed. Identify the handles whose refcount will
    // reach zero after this cell releases its share — those are the
    // ones whose BLAS we drop. Cross-cell shared handles (refcount >
    // count) keep their BLAS so the resident cell still renders.
    //
    // Order matters: BLAS must be detached from any TLAS before its
    // mesh's VkBuffer is queued for destruction — both use the same
    // MAX_FRAMES_IN_FLIGHT countdown, which covers the overlap. We
    // keep the original (drop_blas, then drop_mesh) order; the pre-
    // pass tells us *which* handles to drop_blas without yet mutating
    // the mesh refcounts.
    let mut handle_drop_count: HashMap<u32, u32> = HashMap::new();
    for &mh in &mesh_drops {
        *handle_drop_count.entry(mh).or_insert(0) += 1;
    }
    let freed_meshes: Vec<u32> = handle_drop_count
        .iter()
        .filter_map(|(&h, &c)| match ctx.mesh_registry.refcount(h) {
            Some(rc) if rc == c => Some(h),
            _ => None,
        })
        .collect();
    if let Some(ref mut accel) = ctx.accel_manager {
        for &mh in &freed_meshes {
            accel.drop_blas(mh);
        }
        // #495 — the shared BLAS build scratch buffer is grow-only
        // across the process lifetime; a single peek at an 80–200 MB
        // scratch mesh (FO4 LOD terrain, Skyrim draugr skeletons,
        // Starfield `Saturn.nif`) permanently pins that much
        // DEVICE_LOCAL VRAM. Cell unload is a safe boundary — no BLAS
        // builds are in flight here — so shrink the scratch to the
        // new post-drop peak. SAFETY: we're on the main thread and no
        // BLAS build command buffer is currently referencing the
        // shared scratch (builds run synchronously through fenced
        // one-time command buffers). Skip when the allocator hasn't
        // been initialised yet (headless / pre-init test paths).
        if let Some(allocator) = ctx.allocator.as_ref() {
            unsafe {
                accel.shrink_blas_scratch_to_fit(&ctx.device, allocator);
            }
        }
    }
    // One drop per holder. The handles in `freed_meshes` will hit
    // refcount 0 on their final drop and queue their VkBuffers for
    // deferred destruction; cross-cell shared handles stay live with
    // a positive refcount.
    for &mh in &mesh_drops {
        ctx.mesh_registry.drop_mesh(mh);
    }

    // #1003 / #1004 — skin slot + failed-slot cache cleanup on cell
    // unload. Pre-fix the per-frame eviction pass at the top of
    // `draw_frame` was the only path that reclaimed SkinSlots (after
    // ~3 idle frames) and cleared `failed_skin_slots` (only when an
    // active slot was evicted). Cell unload without a subsequent
    // render tick — headless smoke tests, paused world, or
    // `draw_frame` early-return — silently retained both forever.
    // Queue victims here for the eviction pass to drain post-fence-
    // wait (deferred because `destroy_slot` is synchronous and cell
    // unload runs outside the per-frame fence boundary).
    queue_skin_unload_victims(
        &victims,
        |eid| ctx.skin_slots.contains_key(&eid),
        &mut ctx.pending_skin_unload_victims,
        &mut ctx.failed_skin_slots,
    );
    for &th in &texture_drops {
        ctx.texture_registry.drop_texture(&ctx.device, th);
    }

    // #896 DROP — release per-ItemStack `ItemInstancePool` slots so
    // they return to the free-list ahead of the entity despawn. The
    // common stack-only case (`instance: None` — stimpaks, ammo) is a
    // no-op; only stacks that allocated divergent state (named items,
    // modded weapons, partial-condition armor) reach the release call.
    // Skipped silently when the pool resource isn't registered (test
    // fixtures); production registers it at App init. Without this
    // wiring the pool's `instances` Vec grows monotonically across
    // cell crossings, defeating the bounded-arena guarantee that's
    // the whole point of the M45 save-shape design.
    release_victim_item_instances(world, &victims);

    // Remove every surviving component row for the victim entities.
    let victim_count = victims.len();
    for eid in victims {
        world.despawn(eid);
    }

    log::info!(
        "Cell unload: {} entities, {} mesh refs ({} freed), {} texture refs released (cell_root {})",
        victim_count,
        mesh_drops.len(),
        freed_meshes.len(),
        texture_drops.len(),
        cell_root,
    );
}

/// Walk `victims` for [`Inventory`] components and release every
/// `ItemStack.instance: Some(_)` slot back to the [`ItemInstancePool`]
/// free-list. Called from [`unload_cell`] before the victim despawn
/// loop runs (#896 DROP completeness check).
///
/// Two-phase to satisfy the lock-order invariant: read the Inventory
/// SparseSet first (collecting instance IDs into a scratch Vec), drop
/// the query guard, then take the resource write-lock and release.
/// Holding both simultaneously would cross-lock a SparseSet read and a
/// Resource write — not deadlocking per the TypeId-sort rule (different
/// kinds of storage), but the collect-first pattern is what the rest of
/// `unload_cell` already uses and keeps the lock-hold window short.
pub(crate) fn release_victim_item_instances(world: &mut World, victims: &[EntityId]) {
    let mut to_release: Vec<ItemInstanceId> = Vec::new();
    {
        let Some(inv_q) = world.query::<Inventory>() else {
            return;
        };
        for &eid in victims {
            let Some(inv) = inv_q.get(eid) else { continue };
            for stack in &inv.items {
                if let Some(id) = stack.instance {
                    to_release.push(id);
                }
            }
        }
    }
    if to_release.is_empty() {
        return;
    }
    let Some(mut pool) = world.try_resource_mut::<ItemInstancePool>() else {
        return;
    };
    for id in to_release {
        pool.release(id);
    }
}

/// Queue cell-unload victims for skin-slot teardown and prune the
/// `failed_skin_slots` host-side cache. Extracted from `unload_cell`
/// so the host-side state transformation is unit-testable without a
/// Vulkan device. See #1003 / #1004.
///
/// - `victims`: every entity owned by the unloading cell root.
/// - `slot_present`: predicate over EntityId — `true` when the entity
///   has a live `SkinSlot` (passed in this shape so tests can fake the
///   HashMap without depending on `VulkanContext`).
/// - `pending`: `VulkanContext::pending_skin_unload_victims` queue,
///   drained by the renderer's eviction pass next frame.
/// - `failed`: `VulkanContext::failed_skin_slots` set; entries for
///   victim EntityIds removed in place. Host-side state only — safe
///   to mutate without GPU sync.
pub(super) fn queue_skin_unload_victims<F>(
    victims: &[EntityId],
    slot_present: F,
    pending: &mut Vec<EntityId>,
    failed: &mut std::collections::HashSet<EntityId>,
) where
    F: Fn(EntityId) -> bool,
{
    for &eid in victims {
        if slot_present(eid) {
            pending.push(eid);
        }
    }
    if failed.is_empty() {
        return;
    }
    let victim_set: std::collections::HashSet<EntityId> = victims.iter().copied().collect();
    failed.retain(|eid| !victim_set.contains(eid));
}
