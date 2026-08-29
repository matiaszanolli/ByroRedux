//! Cell teardown — despawn entities + free GPU resources.

use byroredux_core::ecs::components::{CellRoot, Children, Inventory, ItemInstanceId};
use byroredux_core::ecs::resources::ItemInstancePool;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{MeshHandle, TextureHandle, World};
use byroredux_renderer::VulkanContext;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::components::{
    CellRootIndex, MaterialTextureHandles, NormalMapHandle, TerrainTileSlot, WaterNoiseMapHandles,
};

/// Active native vehicle chains have crossed their source cell boundary by
/// design. Promote their roots and complete render hierarchies out of cell
/// ownership so exterior streaming cannot despawn a moving cinematic.
fn cinematic_retained_entities(world: &World) -> HashSet<EntityId> {
    let mut retained = HashSet::new();
    if let Some(tethers) = world.query::<byroredux_scripting::HorseTetherState>() {
        for (cart, tether) in tethers.iter() {
            retained.insert(cart);
            retained.insert(tether.horse);
        }
    }
    if let Some(states) = world.query::<byroredux_scripting::ActorCinematicState>() {
        for (actor, state) in states.iter() {
            if let Some(vehicle) = state.vehicle {
                retained.insert(actor);
                retained.insert(vehicle);
            }
        }
    }
    let Some(children) = world.query::<Children>() else {
        return retained;
    };
    let mut stack: Vec<_> = retained.iter().copied().collect();
    while let Some(parent) = stack.pop() {
        let Some(row) = children.get(parent) else {
            continue;
        };
        for &child in &row.0 {
            if retained.insert(child) {
                stack.push(child);
            }
        }
    }
    retained
}

/// Bounded phase timings for one logical cell-unload batch.
///
/// Exterior streaming records these in its existing constant-memory latency
/// summaries. Keeping the phases here makes the ownership/GPU boundary
/// explicit and lets the live benchmark identify the next resumable unit
/// without changing teardown order merely to profile it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnloadPhaseTimings {
    pub ownership_index: Duration,
    pub handle_collection: Duration,
    pub gpu_release: Duration,
    pub owned_state_release: Duration,
    pub despawn: Duration,
    pub finalization: Duration,
}

impl UnloadPhaseTimings {
    fn absorb(&mut self, other: Self) {
        self.ownership_index = self.ownership_index.saturating_add(other.ownership_index);
        self.handle_collection = self
            .handle_collection
            .saturating_add(other.handle_collection);
        self.gpu_release = self.gpu_release.saturating_add(other.gpu_release);
        self.owned_state_release = self
            .owned_state_release
            .saturating_add(other.owned_state_release);
        self.despawn = self.despawn.saturating_add(other.despawn);
        self.finalization = self.finalization.saturating_add(other.finalization);
    }
}

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
/// function batch-drops one reference per entity-held handle. Shared
/// textures across still-resident cells survive an unload because the
/// remaining holders keep the refcount positive. M40 doorwalking needs
/// this — without it, cell A's unload would flip cell B's shared
/// clutter textures to the checkerboard.
#[tracing::instrument(name = "unload_cell", skip_all, fields(cell_root = ?cell_root))]
pub fn unload_cell(world: &mut World, ctx: &mut VulkanContext, cell_root: EntityId) {
    let _ = unload_cell_inner(world, ctx, cell_root);
    let _ = finish_unload_batch(world, ctx);
}

/// Tear down several cells as one boundary batch.
///
/// Per-cell ownership release remains identical to [`unload_cell`], while the
/// global sparse-storage compaction and BLAS scratch shrink run once after the
/// final victim set. Exterior hysteresis normally evicts three cells at once;
/// repeating those global passes per cell only multiplies the boundary hitch.
#[tracing::instrument(name = "unload_cells", skip_all, fields(cell_count = cell_roots.len()))]
pub fn unload_cells(
    world: &mut World,
    ctx: &mut VulkanContext,
    cell_roots: &[EntityId],
) -> UnloadPhaseTimings {
    let mut timings = UnloadPhaseTimings::default();
    for &cell_root in cell_roots {
        timings.absorb(unload_cell_inner(world, ctx, cell_root));
    }
    if !cell_roots.is_empty() {
        timings.finalization = finish_unload_batch(world, ctx);
    }
    timings
}

/// Take `cell_root`'s entity list out of the [`CellRootIndex`] as a
/// duplicate-free victim set.
///
/// The index half of `stamp_cell_root_range` is a plain
/// `entry.extend(first..last)` — no dedup, no `contains` check — so the
/// list is only as distinct as its producers are disciplined about
/// stamping disjoint ranges. #3379 was one producer that wasn't, and the
/// consequences landed here: `unload_cell_inner` hands this vector
/// verbatim to five consumers, two of which count occurrences rather
/// than membership. `mesh_registry.drop_meshes` and
/// `texture_registry.drop_textures` decrement once per pushed handle, so
/// a repeated victim over-drops its meshes and textures — the handle
/// reaches zero while a still-resident cell is drawing it, and the
/// `rc == c` test that decides which BLAS to drop fails, retaining an
/// acceleration structure over a buffer already queued for destruction.
///
/// Deduping once, here, makes that a property of the reclaim path rather
/// than of every producer: any future stamp-site bug costs wasted work
/// instead of GPU-lifetime corruption. Sorting is incidental (it is how
/// the dedup is done) but harmless — every consumer is order-independent
/// and `despawn_batch` sorts again anyway.
///
/// Returns empty when the resource isn't registered (test fixtures that
/// drive reduced setups) or the cell isn't tracked — `unload_cell` is
/// idempotent, and that was the pre-#791 behaviour for a cell whose
/// query found no rows.
pub(super) fn drain_cell_victims(world: &mut World, cell_root: EntityId) -> Vec<EntityId> {
    let mut victims: Vec<EntityId> = world
        .try_resource_mut::<CellRootIndex>()
        .and_then(|mut idx| idx.map.remove(&cell_root))
        .unwrap_or_default();
    victims.sort_unstable();
    victims.dedup();
    victims
}

fn unload_cell_inner(
    world: &mut World,
    ctx: &mut VulkanContext,
    cell_root: EntityId,
) -> UnloadPhaseTimings {
    let mut timings = UnloadPhaseTimings::default();
    let phase_started = Instant::now();
    // Drain victims from the `CellRootIndex` inverted map (#791). Pre-#791
    // this filtered the entire `CellRoot` SparseSet to find victims of a
    // single cell, scaling O(total resident entities); the index makes
    // lookup O(victims). If the resource is absent (test fixtures that
    // don't register it) or the cell isn't tracked, fall through with
    // an empty victim set — `unload_cell` is idempotent.
    let mut victims: Vec<EntityId> = drain_cell_victims(world, cell_root);
    let retained = cinematic_retained_entities(world);
    victims.retain(|entity| !retained.contains(entity));
    if !retained.is_empty() {
        if let Some(mut roots) = world.query_mut::<CellRoot>() {
            for entity in retained {
                roots.remove(entity);
            }
        }
    }
    timings.ownership_index = phase_started.elapsed();

    // Collect every GPU handle the victims hold (mesh / texture /
    // terrain-tile slot) in one fan-out walk, then release them below.
    // Extracted into a pure fn over the `World` so its handle-coverage
    // contract — every texture-handle component type must be swept —
    // is unit-testable without a `VulkanContext` (#1341). Mirrors the
    // `release_victim_item_instances` (#896) extraction.
    let fallback_tex = ctx.texture_registry.fallback();
    let phase_started = Instant::now();
    let (mesh_drops, mut texture_drops, terrain_tile_slots) =
        collect_victim_gpu_handles(world, &victims, fallback_tex);
    timings.handle_collection = phase_started.elapsed();

    // `SkyParamsRes` / `CellLightingRes` / `WeatherDataRes` /
    // `WeatherTransitionRes` and the bindless texture handles on
    // `SkyParamsRes::texture_indices()` are worldspace-scoped — acquired
    // once by `apply_worldspace_weather` (scene/world_setup.rs) at
    // streaming bootstrap, not per cell load. The pre-#1199 pattern
    // released them on every cell unload, expecting cell-load to
    // re-acquire; `load_one_exterior_cell` never did. The first
    // cell-out-of-range event over-released the texture refcount
    // (bindless slot redirected to the fallback checkerboard) and
    // wiped `WeatherDataRes`, silently freezing exterior lighting for
    // the rest of the session. Their lifetime now matches the World; a
    // future door-walking worldspace transition will release them at
    // the boundary. See #1199.

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
    let phase_started = Instant::now();
    for &slot in &terrain_tile_slots {
        if let Some(tile) = ctx.free_terrain_tile(slot) {
            for idx in tile.texture_indices() {
                // Same skip rule as `collect_victim_gpu_handles`'
                // `push_tex_drop`: never drop the placeholder (0) or the
                // shared registry fallback slot.
                if idx != 0 && idx != fallback_tex {
                    texture_drops.push(idx);
                }
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
    }
    // One drop per holder. Batch release keeps duplicate handles (one
    // decrement per placement) but purges the path cache once after every
    // zero-ref slot is known, avoiding O(freed × cache) unload work.
    let freed_mesh_count = ctx.mesh_registry.drop_meshes(&mesh_drops);
    debug_assert_eq!(freed_mesh_count, freed_meshes.len());

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
    // #3231 — same leak this cell-unload-without-a-render-tick fix
    // closed for `skin_slots` (#1003) applies to `morph_slots`: it has
    // no lazy retry cache to reconcile, so a direct filter+push is
    // enough (no `queue_skin_unload_victims` reuse needed).
    for &eid in &victims {
        if ctx.morph_slots.contains_key(&eid) {
            ctx.pending_morph_unload_victims.push(eid);
        }
    }
    // Same cache-shape fix for textures. Descriptor fallback writes still run
    // once per texture that actually reaches zero; holder refcounts and
    // deferred GPU destruction are unchanged.
    ctx.texture_registry
        .drop_textures(&ctx.device, &texture_drops);
    timings.gpu_release = phase_started.elapsed();

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
    let phase_started = Instant::now();
    release_victim_item_instances(world, &victims);

    // #1520 DROP — remove each victim's Rapier body + colliders from the
    // `PhysicsWorld` before the despawn loop drops the `RapierHandles`
    // ECS row. `World::despawn` frees only the component row; the body
    // and colliders it points at have no Drop tied to the ECS, so without
    // this they accumulate in `RigidBodySet` / `ColliderSet` (and the
    // broad-phase / query-pipeline BVH) on every cell crossing — an
    // unbounded leak, worst under exterior radius streaming which never
    // resets the PhysicsWorld. Skipped silently when the resource isn't
    // registered (loose-NIF demo / test fixtures that opt out of physics).
    release_victim_rapier_bodies(world, &victims);
    timings.owned_state_release = phase_started.elapsed();

    // Remove every surviving component row for the victim entities.
    let victim_count = victims.len();
    let phase_started = Instant::now();
    world.despawn_batch(victims);
    if victim_count > 0 {
        // Quest-alias bindings may point at actor candidates owned by this
        // cell. Rebuild lazily on the next scene tick so stale EntityIds
        // cannot survive an interior transition or exterior stream-out.
        byroredux_scripting::mark_scene_actor_bindings_dirty(world);
    }
    timings.despawn = phase_started.elapsed();

    log::info!(
        "Cell unload: {} entities, {} mesh refs ({} freed), {} texture refs released (cell_root {})",
        victim_count,
        mesh_drops.len(),
        freed_meshes.len(),
        texture_drops.len(),
        cell_root,
    );
    timings
}

fn finish_unload_batch(world: &mut World, ctx: &mut VulkanContext) -> Duration {
    let started = Instant::now();
    // #2148 / ECS-2507-02 — hand back sparse-index tails after every victim
    // set in this logical boundary has been despawned. Running this after each
    // of the usual three exterior cells repeats the backwards scan and may
    // reallocate a storage that the next cell immediately mutates again.
    world.shrink_storages();

    // #495 — shrink the shared BLAS scratch buffer against the final post-drop
    // peak once per logical unload.
    // SAFETY: `ctx.device`/`allocator` are the same pair that allocated the
    // current scratch buffer (both come from this same `&mut VulkanContext`).
    // Retiring the old scratch allocation is deferred for frames-in-flight by
    // AccelerationManager (#1782), so this is safe from the about_to_wait
    // streaming path.
    if let (Some(accel), Some(allocator)) = (ctx.accel_manager.as_mut(), ctx.allocator.as_ref()) {
        unsafe {
            accel.shrink_blas_scratch_to_fit(&ctx.device, allocator);
        }
    }
    started.elapsed()
}

/// Collect every GPU handle the cell's `victims` hold so [`unload_cell`]
/// can pair each with its release. Pure over the `World` — no
/// `VulkanContext` — so the handle-coverage contract is unit-testable
/// (see `unload_greyscale_lut_tests`), mirroring the
/// [`release_victim_item_instances`] (#896) extraction.
///
/// Returns `(mesh_drops, texture_drops, terrain_tile_slots)`:
/// - `mesh_drops` — one `MeshHandle` per holder (refcounted dedup #879:
///   each holder contributes one decrement so the registry frees the GPU
///   buffers exactly when the last placement releases).
/// - `texture_drops` — the base [`TextureHandle`], specialized water
///   [`NormalMapHandle`], and every secondary role in
///   [`MaterialTextureHandles`]. Handle `0` and `fallback_tex` are skipped —
///   those are
///   the shared placeholder / neutral-fallback slots that are never
///   per-cell refcounted.
/// - `terrain_tile_slots` — `TerrainTileSlot` IDs; the caller frees each
///   slot's 8 layer refcounts via `free_terrain_tile` (#627).
///
/// # Adding a texture-handle component
/// Every component that carries a `resolve_texture`-acquired bindless
/// handle MUST be swept here or its refcount leaks on cell unload (the
/// #1341 / D3-05 bug was exactly such an omission — the greyscale LUT
/// was attached at spawn but never collected). The unit test pins every
/// semantic role in the common set; extend it when adding a new role.
pub(crate) fn collect_victim_gpu_handles(
    world: &World,
    victims: &[EntityId],
    fallback_tex: u32,
) -> (Vec<u32>, Vec<u32>, HashSet<u32>) {
    let mut mesh_drops: Vec<u32> = Vec::new();
    let mut texture_drops: Vec<u32> = Vec::new();
    let mut terrain_tile_slots: HashSet<u32> = HashSet::new();
    let push_tex_drop = |handle: u32, sink: &mut Vec<u32>| {
        if handle != 0 && handle != fallback_tex {
            sink.push(handle);
        }
    };
    // #883 / CELL-PERF-06 — single victim walk that fans out to every
    // per-component lookup. Pre-fix this was independent `for &eid in
    // victims` loops, each re-acquiring a read lock on a different
    // SparseSet header. The per-victim inner cost is unchanged (one hash
    // lookup per component), but the SparseSet header walk happens once.
    //
    // Holding the read locks across the walk is safe — they're
    // independent SparseSets (different component TypeIds) and the caller
    // holds `&mut World`, so no concurrent writer can exist. The
    // TypeId-sort lock-order invariant (CLAUDE.md #4) is about combined
    // read+write multi-component queries where a mixed acquire order
    // could deadlock; pure reads have no such risk.
    let mq = world.query::<MeshHandle>();
    let tq = world.query::<TextureHandle>();
    let nq = world.query::<NormalMapHandle>();
    let wnq = world.query::<WaterNoiseMapHandles>();
    let mtq = world.query::<MaterialTextureHandles>();
    let ttq = world.query::<TerrainTileSlot>();
    for &eid in victims {
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
        if let Some(wnq) = &wnq {
            if let Some(handles) = wnq.get(eid) {
                for &handle in &handles.0 {
                    push_tex_drop(handle, &mut texture_drops);
                }
            }
        }
        if let Some(mtq) = &mtq {
            if let Some(handles) = mtq.get(eid) {
                let maps = &handles.textures;
                // Base color is released through TextureHandle above. Every
                // secondary semantic role was independently acquired.
                for &handle in maps.secondary_values() {
                    push_tex_drop(handle, &mut texture_drops);
                }
            }
        }
        if let Some(ttq) = &ttq {
            if let Some(slot) = ttq.get(eid) {
                terrain_tile_slots.insert(slot.0);
            }
        }
    }
    // Query guards drop here at fn return — before the caller's GPU
    // registry mutations — keeping the lock-hold window scoped to the walk.
    (mesh_drops, texture_drops, terrain_tile_slots)
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

/// Walk `victims` for [`RapierHandles`] **and** [`Ragdoll`] components and
/// remove each entity's rigid bodies + attached colliders + multibody
/// joints from the [`PhysicsWorld`] before the victim despawn loop runs
/// (#1520 / #1531 DROP completeness check).
///
/// Two component classes feed the solver and must both be swept:
/// - `RapierHandles` — the single character/physics-sync body+collider
///   pair registered by `physics_sync_system` (#1520).
/// - `Ragdoll` — the N-body humanoid ragdoll the `ragdoll <id>` command
///   attaches via `build_ragdoll`, whose bodies/colliders/multibody joints
///   are inserted directly into the solver sets, *not* through
///   `RapierHandles`. Without this branch a cell unloading with a
///   ragdolling actor in it orphaned every ragdoll body+collider+joint in
///   the Rapier sets and broad-phase / query-pipeline BVH — the exact
///   unbounded-leak shape #1520 closed, re-introduced for the new
///   component (#1531). `PhysicsWorld::remove_ragdoll` cascades the
///   colliders + joints out via `remove_body`.
///
/// Same two-phase shape as [`release_victim_item_instances`]: read the
/// component SparseSets first (collecting handles into scratch Vecs), drop
/// the query guards, then take the `PhysicsWorld` resource write-lock and
/// remove. Keeps the lock-hold window short and avoids holding a component
/// read lock across the resource write.
///
/// No-op (returns early) when no victim carries a `RapierHandles` or
/// `Ragdoll` row, or when the `PhysicsWorld` resource isn't registered (the
/// loose-NIF demo path opts out of physics — see `byroredux_physics` crate
/// docs).
///
/// **`victims` may repeat; removal is idempotent** (#3380). A repeated
/// entity collects its handle twice and issues a second `remove_body` on
/// an already-freed slot, which rapier absorbs via its generational
/// handles — the arena stays correct, only the work is wasted. Stated
/// here because the property was previously inherited from a dependency
/// with nothing in our code or tests holding it: the sibling
/// `release_victim_item_instances` path is explicitly hardened against
/// duplicates while `collect_victim_gpu_handles` is deliberately *not*
/// (it counts occurrences, one refcount decrement per placement), so a
/// caller cannot infer the convention from its neighbours.
/// [`drain_cell_victims`] is the production producer and dedups, so this
/// tolerance is a safety net rather than a licence.
pub(crate) fn release_victim_rapier_bodies(world: &mut World, victims: &[EntityId]) {
    use byroredux_physics::{PhysicsWorld, Ragdoll, RapierHandles};

    let mut to_remove: Vec<RapierHandles> = Vec::new();
    let mut ragdolls: Vec<Ragdoll> = Vec::new();
    {
        if let Some(handles_q) = world.query::<RapierHandles>() {
            for &eid in victims {
                if let Some(h) = handles_q.get(eid) {
                    to_remove.push(*h);
                }
            }
        }
        if let Some(ragdoll_q) = world.query::<Ragdoll>() {
            for &eid in victims {
                if let Some(r) = ragdoll_q.get(eid) {
                    ragdolls.push(r.clone());
                }
            }
        }
    }
    if to_remove.is_empty() && ragdolls.is_empty() {
        return;
    }
    let Some(mut pw) = world.try_resource_mut::<PhysicsWorld>() else {
        return;
    };
    for h in to_remove {
        pw.remove_body(h.body);
    }
    for r in &ragdolls {
        pw.remove_ragdoll(r);
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
    // #3061 — `FxHashSet`, matching `VulkanContext::failed_skin_slots`. The
    // set is probed once per skinned entity per frame on the renderer side;
    // this unload path only mutates it, but the type is the renderer's.
    failed: &mut rustc_hash::FxHashSet<EntityId>,
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
    let victim_set: rustc_hash::FxHashSet<EntityId> = victims.iter().copied().collect();
    failed.retain(|eid| !victim_set.contains(eid));
}

#[cfg(test)]
mod cinematic_retention_tests {
    use super::*;
    use byroredux_core::math::{Quat, Vec3};
    use byroredux_scripting::{ActorCinematicState, HorseTetherState};

    #[test]
    fn active_tether_retains_horse_cart_rider_and_hierarchy() {
        let mut world = World::new();
        world.register::<Children>();
        world.register::<HorseTetherState>();
        world.register::<ActorCinematicState>();
        let horse = world.spawn();
        let bone = world.spawn();
        let cart = world.spawn();
        let rider = world.spawn();
        let unrelated = world.spawn();
        world.insert(horse, Children(vec![bone]));
        world.insert(
            cart,
            HorseTetherState {
                horse,
                horse_local_translation: Vec3::ZERO,
                horse_local_rotation: Quat::IDENTITY,
                route_target_form_id: None,
            },
        );
        world.insert(
            rider,
            ActorCinematicState {
                vehicle: Some(cart),
                ..Default::default()
            },
        );

        let retained = cinematic_retained_entities(&world);
        assert!(retained.contains(&horse));
        assert!(retained.contains(&bone));
        assert!(retained.contains(&cart));
        assert!(retained.contains(&rider));
        assert!(!retained.contains(&unrelated));
    }
}

#[cfg(test)]
mod victim_drain_tests {
    use super::*;
    use crate::components::CellRootIndex;
    use byroredux_core::ecs::World;

    /// #3379 belt-and-braces — the reclaim path must not depend on every
    /// stamp producer keeping its ranges disjoint.
    ///
    /// `stamp_cell_root_range`'s index half is a bare
    /// `entry.extend(first..last)`, so a producer that re-stamps a range
    /// it already covered pushes the same entity twice. Two of the five
    /// victim consumers count occurrences instead of membership
    /// (`drop_meshes` / `drop_textures`, one refcount decrement per
    /// pushed handle), so a duplicate there is a GPU-lifetime bug, not
    /// wasted work. Deduping in the drain contains the whole class.
    #[test]
    fn drain_returns_each_victim_once() {
        let mut world = World::new();
        world.insert_resource(CellRootIndex::new());
        let root = world.spawn();
        let a = world.spawn();
        let b = world.spawn();
        {
            let mut idx = world.resource_mut::<CellRootIndex>();
            // The shape a three-slice re-stamping producer leaves behind.
            idx.map.insert(root, vec![a, b, a, b, a]);
        }

        let victims = drain_cell_victims(&mut world, root);

        assert_eq!(victims, vec![a, b], "victims must be a set, not a bag");
        assert!(
            world.resource::<CellRootIndex>().map.get(&root).is_none(),
            "the drain must take the entry, not copy it",
        );
    }

    /// An unregistered resource or an untracked cell yields an empty set
    /// — `unload_cell` stays idempotent, which is what test fixtures
    /// with reduced setups depend on.
    #[test]
    fn drain_is_empty_for_an_untracked_cell() {
        let mut world = World::new();
        let root = world.spawn();
        assert!(drain_cell_victims(&mut world, root).is_empty());

        world.insert_resource(CellRootIndex::new());
        assert!(drain_cell_victims(&mut world, root).is_empty());
    }
}
