//! World-bound propagation — bottom-up fold of `LocalBound` + child bounds.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    Children, GlobalTransform, HierarchyTraversalGuard, LocalBound, Parent, SkinnedMesh, World,
    WorldBound,
};
use byroredux_core::math::Mat4;
use rustc_hash::FxHashSet;

fn transform_sphere(local: &LocalBound, transform: Mat4) -> WorldBound {
    let max_scale = transform
        .x_axis
        .truncate()
        .length()
        .max(transform.y_axis.truncate().length())
        .max(transform.z_axis.truncate().length());
    WorldBound::new(
        transform.transform_point3(local.center),
        local.radius * max_scale,
    )
}

/// Conservatively encloses a linearly-skinned local sphere in world space.
///
/// A skinned vertex is a convex combination of the same local point passed
/// through its bone palettes. Enclosing the sphere transformed by every
/// palette therefore also encloses every blended result. This matters for
/// Skyrim's pre-baked FaceGen NIFs: most facial leaves have an identity node
/// transform at the actor root while their skin palette places them roughly
/// 120 units higher at `NPC Head`. Using the leaf `GlobalTransform` for those
/// bounds culls mouth/eye/brow/hair pieces independently as the camera moves.
fn skinned_world_bound(
    local: &LocalBound,
    skin: &SkinnedMesh,
    mut bone_world: impl FnMut(EntityId) -> Option<Mat4>,
) -> WorldBound {
    let mut merged = WorldBound::ZERO;
    for (bone, bind_inverse) in skin.bones.iter().zip(&skin.bind_inverses) {
        // Mirror the renderer's unresolved-bone fallback exactly. Keeping the
        // identity contribution is conservative for the triangle-ribbon error
        // case and prevents culling from hiding a separate resolution bug.
        let palette = bone
            .and_then(&mut bone_world)
            .map(|world| world * *bind_inverse)
            .unwrap_or(Mat4::IDENTITY);
        merged = merged.merge(&transform_sphere(local, palette));
    }
    merged
}

/// Did anything this skin's world bound depends on move this frame?
///
/// `g_dirty` is the sorted + deduped `GlobalTransform` dirty set, so each
/// probe is a `binary_search`. A skinned bound is a function of the bone
/// palettes alone (see [`skinned_world_bound`]) — the mesh entity's own
/// `GlobalTransform` never enters it — but the mesh is probed too so a
/// re-placed actor whose bones happen to be identity still refolds.
fn skin_pose_dirty(entity: EntityId, skin: &SkinnedMesh, g_dirty: &[EntityId]) -> bool {
    g_dirty.binary_search(&entity).is_ok()
        || skin
            .bones
            .iter()
            .flatten()
            .any(|bone| g_dirty.binary_search(bone).is_ok())
}

/// Compute each entity's world-space `WorldBound`.
///
/// Two passes:
///
/// 1. **Leaf bounds** — for every entity with a `LocalBound` (set at import
///    time from NIF `NiBound`), compose it with `GlobalTransform` to
///    produce a world-space sphere. The center is rotated and translated
///    by the entity's world transform; the radius is scaled uniformly
///    by the world scale.
///
/// 2. **Parent bounds** — for every entity that has `Children` but no
///    `LocalBound` (i.e. pure scene-graph nodes), fold the children's
///    `WorldBound`s into a single enclosing sphere via
///    [`WorldBound::merge`]. Runs bottom-up through post-order traversal
///    so each parent sees its descendants' final bounds. Walks the
///    hierarchy reusing the same `queue` vec so we don't allocate per
///    frame; the initial root set is also reused across frames.
///
/// Runs after `transform_propagation_system` and `billboard_system`
/// (scheduled as an exclusive PostUpdate step) so both leaf transforms
/// and billboard overrides are final. See issue #217.
pub(crate) fn make_world_bound_propagation_system() -> impl FnMut(&World, f32) + Send + Sync {
    // Roots that actually have children — the only entities pass 2 needs to
    // fold. Cached across frames; rebuilt only on a structural change. The
    // flat statics that make up the bulk of an exterior cell are childless
    // roots whose bound is final after pass 1, so they never enter this list
    // (pre-#perf the post-order walk covered *every* root every frame).
    let mut child_roots: Vec<EntityId> = Vec::new();
    // Scratch: the child-having roots whose subtree actually moved this
    // frame (reused to avoid a per-frame allocation).
    let mut dirty_roots: Vec<EntityId> = Vec::new();
    let mut post_order: Vec<EntityId> = Vec::new();
    let mut stack: Vec<(EntityId, bool)> = Vec::new();
    let mut post_seen: FxHashSet<EntityId> = FxHashSet::default();
    let mut climb_seen: FxHashSet<EntityId> = FxHashSet::default();
    // Structural-generation key over the BOUND-RELEVANT structure only:
    // (LocalBound gen, Parent gen, Children gen, SkinnedMesh gen). A move changes
    // GlobalTransform *values* (caught by the dirty set), not these. Keyed on
    // these — not `next_entity_id` / `GlobalTransform::len` — so that
    // unbounded per-frame spawns (particles, transient event markers) don't
    // churn the key and defeat the fast path; only adding/removing a bounded
    // entity or reparenting forces a full rebuild. Requires
    // LocalBound/Parent/Children `TRACK_CHANGES`.
    let mut last_key: Option<(u64, u64, u64, u64)> = None;
    // Persistent scratch for the GlobalTransform dirty-entity list (#1371).
    // Reusing across frames avoids the 0→N re-growth that take_dirty causes.
    let mut g_dirty: Vec<EntityId> = Vec::new();

    move |world: &World, _dt: f32| {
        // Drain the GlobalTransform change-tracking dirty set: the entities
        // whose world transform was (re)written this frame by transform
        // propagation / billboard_system. Empty in steady state (nothing
        // moved) → the fast path below skips both passes entirely. Requires
        // GlobalTransform::TRACK_CHANGES; bounds is its sole drainer.
        {
            let Some(mut gq) = world.query_mut::<GlobalTransform>() else {
                return;
            };
            gq.storage_mut().drain_dirty_into(&mut g_dirty);
        }

        // Acquire Parent, Children, GlobalTransform, SkinnedMesh and
        // LocalBound once — used by both passes (#250), in the canonical
        // hierarchy-cluster order documented in `docs/engine/ecs.md`
        // (#313, #2388): Transform → Parent → Children → GlobalTransform →
        // SkinnedMesh → LocalBound → WorldBound. LocalBound used to be taken
        // first here, inverting `Parent↔LocalBound`, `Children↔LocalBound`
        // and `GlobalTransform↔LocalBound` against
        // `ragdoll_writeback_system`, which walks the same cluster.
        let parent_q = world.query::<Parent>();
        let children_q = world.query::<Children>();
        let Some(g_q) = world.query::<GlobalTransform>() else {
            return;
        };
        let child_reference_count = children_q
            .as_ref()
            .map(|q| q.iter().map(|(_, children)| children.0.len()).sum())
            .unwrap_or(0);
        let mut cycle_reported = false;
        let traversal_entity_count = world.next_entity_id() as usize;
        let skin_q = world.query::<SkinnedMesh>();
        let local_q = world.query::<LocalBound>();
        let Some(ref lb_q) = local_q else {
            return;
        };

        // Structural key + change flag. A structural change (or the first
        // frame, when `last_key` is None) forces a full rebuild; otherwise
        // only the dirty entities are touched.
        let key = (
            lb_q.storage().structural_generation(),
            parent_q
                .as_ref()
                .map(|q| q.storage().structural_generation())
                .unwrap_or(0),
            children_q
                .as_ref()
                .map(|q| q.storage().structural_generation())
                .unwrap_or(0),
            skin_q
                .as_ref()
                .map(|q| q.storage().structural_generation())
                .unwrap_or(0),
        );
        let structural_changed = last_key != Some(key);
        last_key = Some(key);

        // FAST PATH: nothing moved and no structural change → every bound is
        // already correct from a prior frame. This is the steady state for a
        // static exterior cell (only the camera moves, and it has no bound).
        if g_dirty.is_empty() && !structural_changed {
            return;
        }

        let Some(mut wb_q) = world.query_mut::<WorldBound>() else {
            return;
        };

        // ── Pass 1: leaf bounds from LocalBound + GlobalTransform ──────────
        // Recompute only the leaves whose GlobalTransform changed. New
        // entities arrive dirty (their GlobalTransform insert marks them), so
        // spawns are covered; on a structural change we fall back to all
        // leaves as a cheap correctness floor for rare reparent/despawn.
        if structural_changed {
            for (entity, local) in lb_q.iter() {
                if let Some(global) = g_q.get(entity) {
                    let center =
                        global.translation + global.rotation * (local.center * global.scale);
                    if let Some(wb) = wb_q.get_mut(entity) {
                        *wb = WorldBound::new(center, local.radius * global.scale);
                    }
                }
            }
        } else {
            g_dirty.sort_unstable();
            g_dirty.dedup();
            for &entity in &g_dirty {
                let (Some(local), Some(global)) = (lb_q.get(entity), g_q.get(entity)) else {
                    continue;
                };
                let center = global.translation + global.rotation * (local.center * global.scale);
                if let Some(wb) = wb_q.get_mut(entity) {
                    *wb = WorldBound::new(center, local.radius * global.scale);
                }
            }
        }

        // A mesh's own GlobalTransform does not describe its final skinned
        // position, so this leaf pass is keyed on the skin's *bones* rather
        // than on the mesh entity.
        //
        // #2677 / PERF-D1-01 — pre-fix this block was the one ungated pass in
        // an otherwise fully dirty-gated system: it refolded every
        // `SkinnedMesh` whenever *anything* in the world moved, and camera
        // motion alone writes `GlobalTransform` (fly camera → billboards), so
        // "anything moved" is every frame the player moves. On the repo's own
        // `fnv-FreesideAtomicWrangler` baseline that is 677 live skins ×
        // bones-per-skin `Mat4 × Mat4` + three `Vec3::length()` per bone, paid
        // for actors that are asleep. `g_dirty` is already sorted + deduped by
        // pass 1's incremental branch, so the gate is a `binary_search` per
        // bone — cheaper than one palette multiply. The structural branch stays
        // ungated: a skin can gain bones (or a `LocalBound`) without any of
        // them being written this frame, and that branch is the correctness
        // floor for exactly those cases.
        if let Some(ref sq) = skin_q {
            for (entity, skin) in sq.iter() {
                if !structural_changed && !skin_pose_dirty(entity, skin, &g_dirty) {
                    continue;
                }
                let Some(local) = lb_q.get(entity) else {
                    continue;
                };
                let bound = skinned_world_bound(local, skin, |bone| {
                    g_q.get(bone).map(GlobalTransform::to_matrix)
                });
                if bound.radius > 0.0 {
                    if let Some(wb) = wb_q.get_mut(entity) {
                        *wb = bound;
                    }
                }
            }
        }

        // ── Pass 2: fold children into child-having parents ────────────────
        // Rebuild the child-root cache only on a structural change. Flat
        // statics (no Children) are skipped — their bound is final after
        // pass 1 — so this list holds only the few hierarchical roots
        // (skinned actors, multi-node placements). Pre-#perf the post-order
        // walk covered every root including the thousands of flat statics.
        if structural_changed {
            child_roots.clear();
            for (entity, _) in g_q.iter() {
                let is_root = parent_q
                    .as_ref()
                    .map(|pq| pq.get(entity).is_none())
                    .unwrap_or(true);
                let has_children = children_q
                    .as_ref()
                    .and_then(|cq| cq.get(entity))
                    .map(|c| !c.0.is_empty())
                    .unwrap_or(false);
                if is_root && has_children {
                    child_roots.push(entity);
                }
            }
        }

        // Flat scene (the typical exterior cell) — pass 1 was the whole job.
        if child_roots.is_empty() {
            return;
        }
        let Some(ref cq) = children_q else {
            return;
        };

        // Which child-having roots actually need re-folding this frame? On a
        // structural change, all of them; otherwise only the roots whose
        // subtree contains a moved entity — walk each dirty entity up to its
        // root and keep it if that root has children. Flat dynamics (player
        // capsule, falling debris) are their own childless roots, so a frame
        // where only they moved folds nothing — the steady state once actors
        // stop animating. This is what keeps pass 2 off the per-frame budget.
        let fold_set: &[EntityId] = if structural_changed {
            &child_roots
        } else {
            dirty_roots.clear();
            for &e in &g_dirty {
                let mut cur = e;
                climb_seen.clear();
                let mut traversal_guard = HierarchyTraversalGuard::new(
                    traversal_entity_count,
                    child_reference_count,
                );
                while let Some(p) = parent_q.as_ref().and_then(|pq| pq.get(cur)).map(|p| p.0) {
                    if !traversal_guard.step() {
                        log::error!(
                            "world-bound propagation aborted after exhausting the hierarchy traversal budget while climbing from entity {e}"
                        );
                        return;
                    }
                    if !climb_seen.insert(cur) {
                        if !cycle_reported {
                            log::error!(
                                "world-bound propagation stopped revisiting hierarchy entity {cur} while climbing from {e}; cyclic Parent edge"
                            );
                            cycle_reported = true;
                        }
                        break;
                    }
                    cur = p;
                }
                if cq.get(cur).map(|c| !c.0.is_empty()).unwrap_or(false) {
                    dirty_roots.push(cur);
                }
            }
            dirty_roots.sort_unstable();
            dirty_roots.dedup();
            &dirty_roots
        };
        if fold_set.is_empty() {
            return;
        }

        // Post-order over the affected roots' subtrees, then fold children
        // into parents (children first). The merge is idempotent.
        post_order.clear();
        stack.clear();
        post_seen.clear();
        let mut traversal_guard = HierarchyTraversalGuard::new(
            traversal_entity_count,
            child_reference_count,
        );
        for &root in fold_set {
            stack.push((root, false));
            while let Some((entity, visited)) = stack.pop() {
                if visited {
                    post_order.push(entity);
                    continue;
                }
                if !traversal_guard.step() {
                    log::error!(
                        "world-bound propagation aborted after exhausting the hierarchy traversal budget at entity {entity}"
                    );
                    return;
                }
                if !post_seen.insert(entity) {
                    if !cycle_reported {
                        log::error!(
                            "world-bound propagation skipped repeated hierarchy entity {entity}; cyclic or duplicate Children edge"
                        );
                        cycle_reported = true;
                    }
                    continue;
                }
                stack.push((entity, true));
                if let Some(children) = cq.get(entity) {
                    for &child in &children.0 {
                        stack.push((child, false));
                    }
                }
            }
        }

        for &entity in &post_order {
            // Leaves keep their pass-1 bound.
            if lb_q.get(entity).is_some() {
                continue;
            }
            let Some(children) = cq.get(entity) else {
                continue;
            };
            let mut merged = WorldBound::ZERO;
            for &child in &children.0 {
                // Copy the child's bound out through the mutable query — safe,
                // we don't alias across iterations.
                if let Some(child_bound) = wb_q.get_mut(child).map(|b| *b) {
                    merged = merged.merge(&child_bound);
                }
            }
            if let Some(wb) = wb_q.get_mut(entity) {
                *wb = merged;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for `make_world_bound_propagation_system` — issue #217.
    //! These cover leaf derivation, parent merging, and the scale path.

    use super::*;
    use byroredux_core::ecs::World;
    use byroredux_core::ecs::{
        Children, GlobalTransform, LocalBound, Parent, SkinnedMesh, WorldBound,
    };
    use byroredux_core::math::{Mat4, Quat, Vec3};

    /// Spawn an entity with a LocalBound + GlobalTransform + empty WorldBound.
    fn spawn_leaf(
        world: &mut World,
        translation: Vec3,
        scale: f32,
        local_center: Vec3,
        local_radius: f32,
    ) -> byroredux_core::ecs::storage::EntityId {
        let e = world.spawn();
        world.insert(e, GlobalTransform::new(translation, Quat::IDENTITY, scale));
        world.insert(e, LocalBound::new(local_center, local_radius));
        world.insert(e, WorldBound::ZERO);
        e
    }

    #[test]
    fn leaf_bound_composes_local_with_global_transform() {
        let mut world = World::new();
        let e = spawn_leaf(&mut world, Vec3::new(10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 2.0);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.center - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-5);
        assert!((wb.radius - 2.0).abs() < 1e-5);
    }

    #[test]
    fn leaf_bound_scale_multiplies_radius() {
        let mut world = World::new();
        let e = spawn_leaf(&mut world, Vec3::ZERO, 3.0, Vec3::ZERO, 1.0);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.radius - 3.0).abs() < 1e-5);
    }

    #[test]
    fn leaf_bound_nonzero_local_center_is_offset() {
        let mut world = World::new();
        // Mesh sits at world origin, scale 2, but its local sphere is
        // centered at (1, 0, 0) local. World center should be (2, 0, 0).
        let e = spawn_leaf(&mut world, Vec3::ZERO, 2.0, Vec3::new(1.0, 0.0, 0.0), 0.5);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.center - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5);
        assert!((wb.radius - 1.0).abs() < 1e-5);
    }

    #[test]
    fn skinned_leaf_bound_uses_bone_palette_instead_of_mesh_transform() {
        let mut world = World::new();
        let bone = world.spawn();
        world.insert(
            bone,
            GlobalTransform::new(Vec3::new(5.0, 120.0, -3.0), Quat::IDENTITY, 1.0),
        );
        let mesh = spawn_leaf(&mut world, Vec3::ZERO, 1.0, Vec3::ZERO, 2.0);
        world.insert(
            mesh,
            SkinnedMesh::new_with_global(
                None,
                vec![Some(bone)],
                vec![Mat4::IDENTITY],
                Mat4::IDENTITY,
            ),
        );

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let bound = *world.query::<WorldBound>().unwrap().get(mesh).unwrap();
        assert!((bound.center - Vec3::new(5.0, 120.0, -3.0)).length() < 1e-5);
        assert!((bound.radius - 2.0).abs() < 1e-5);
    }

    #[test]
    fn skinned_leaf_bound_updates_when_only_its_bone_moves() {
        let mut world = World::new();
        let bone = world.spawn();
        world.insert(bone, GlobalTransform::IDENTITY);
        let mesh = spawn_leaf(&mut world, Vec3::ZERO, 1.0, Vec3::ZERO, 1.0);
        world.insert(
            mesh,
            SkinnedMesh::new_with_global(
                None,
                vec![Some(bone)],
                vec![Mat4::IDENTITY],
                Mat4::IDENTITY,
            ),
        );

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);
        world
            .query_mut::<GlobalTransform>()
            .unwrap()
            .get_mut(bone)
            .unwrap()
            .translation = Vec3::new(0.0, 120.0, 0.0);
        sys(&world, 0.016);

        let bound = *world.query::<WorldBound>().unwrap().get(mesh).unwrap();
        assert!((bound.center - Vec3::new(0.0, 120.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn skinned_leaf_bound_encloses_all_bone_palette_spheres() {
        let local = LocalBound::new(Vec3::ZERO, 1.0);
        let skin = SkinnedMesh::new_with_global(
            None,
            vec![Some(10), Some(20)],
            vec![Mat4::IDENTITY, Mat4::IDENTITY],
            Mat4::IDENTITY,
        );
        let bound = skinned_world_bound(&local, &skin, |bone| match bone {
            10 => Some(Mat4::from_translation(Vec3::new(-10.0, 0.0, 0.0))),
            20 => Some(Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0))),
            _ => None,
        });

        assert!((bound.center - Vec3::ZERO).length() < 1e-5);
        assert!((bound.radius - 11.0).abs() < 1e-5);
    }

    /// #2677 / PERF-D1-01 — a skin whose bones did not move must not be
    /// refolded just because some unrelated entity did (camera motion alone
    /// dirties `GlobalTransform` every frame the player moves). Observed by
    /// poking a sentinel into the mesh's `WorldBound`: if the skinned block
    /// runs it overwrites the sentinel with the real palette bound.
    #[test]
    fn clean_skin_is_not_refolded_when_an_unrelated_entity_moves() {
        let mut world = World::new();
        let bone = world.spawn();
        world.insert(bone, GlobalTransform::IDENTITY);
        let mesh = spawn_leaf(&mut world, Vec3::ZERO, 1.0, Vec3::ZERO, 1.0);
        world.insert(
            mesh,
            SkinnedMesh::new_with_global(
                None,
                vec![Some(bone)],
                vec![Mat4::IDENTITY],
                Mat4::IDENTITY,
            ),
        );
        let other = spawn_leaf(&mut world, Vec3::new(50.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016); // structural rebuild seeds every bound

        const SENTINEL: f32 = 77.0;
        {
            let mut wb = world.query_mut::<WorldBound>().unwrap();
            *wb.get_mut(mesh).unwrap() = WorldBound::new(Vec3::ZERO, SENTINEL);
        }
        {
            let mut gq = world.query_mut::<GlobalTransform>().unwrap();
            gq.get_mut(other).unwrap().translation = Vec3::new(60.0, 0.0, 0.0);
        }
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        assert_eq!(
            wb_q.get(mesh).unwrap().radius,
            SENTINEL,
            "a clean skin must be skipped when only an unrelated entity moved",
        );
        assert!(
            (wb_q.get(other).unwrap().center - Vec3::new(60.0, 0.0, 0.0)).length() < 1e-5,
            "the moved entity itself must still refold",
        );
    }

    #[test]
    fn parent_bound_unions_child_bounds() {
        // Parent at origin (no LocalBound) with two leaf children at ±10
        // along x, each with local radius 1. The parent WorldBound should
        // be the smallest sphere enclosing both — center at origin, r=11.
        let mut world = World::new();
        let parent = world.spawn();
        world.insert(parent, GlobalTransform::IDENTITY);
        world.insert(parent, WorldBound::ZERO);

        let left = spawn_leaf(&mut world, Vec3::new(-10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        let right = spawn_leaf(&mut world, Vec3::new(10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);

        // Wire the hierarchy: both leaves are children of `parent`.
        world.insert(left, Parent(parent));
        world.insert(right, Parent(parent));
        world.insert(parent, Children(vec![left, right]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();

        let left_wb = *wb_q.get(left).unwrap();
        assert!((left_wb.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);
        let right_wb = *wb_q.get(right).unwrap();
        assert!((right_wb.center - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-5);

        let parent_wb = *wb_q.get(parent).unwrap();
        assert!(
            (parent_wb.center - Vec3::ZERO).length() < 1e-5,
            "parent center should be midpoint, got {:?}",
            parent_wb.center,
        );
        assert!(
            (parent_wb.radius - 11.0).abs() < 1e-5,
            "parent radius should enclose both leaves, got {}",
            parent_wb.radius,
        );
        // Contains-check both leaves' centers.
        assert!(parent_wb.contains_point(left_wb.center));
        assert!(parent_wb.contains_point(right_wb.center));
    }

    #[test]
    fn pure_parent_with_no_children_keeps_zero_bound() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, GlobalTransform::IDENTITY);
        world.insert(e, WorldBound::ZERO);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert_eq!(wb.radius, 0.0);
    }

    /// Regression test for #826: the cached root set must invalidate
    /// when the scene-graph topology changes between frames. Mirrors
    /// the sibling test for `transform_propagation_system` (#825). All
    /// three transitions (new root spawned, child-gains-Parent,
    /// child-loses-Parent) move the
    /// `(GlobalTransform::len, Parent::len, next_entity_id)` key, so
    /// each must trigger a rescan and a corrected post-order walk.
    #[test]
    fn root_cache_invalidates_on_topology_change() {
        let mut world = World::new();
        // Initial: one root with one child leaf at (-10, 0, 0) r=1.
        let parent = world.spawn();
        world.insert(parent, GlobalTransform::IDENTITY);
        world.insert(parent, WorldBound::ZERO);

        let leaf = spawn_leaf(&mut world, Vec3::new(-10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        world.insert(leaf, Parent(parent));
        world.insert(parent, Children(vec![leaf]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);
        let parent_wb_initial = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!((parent_wb_initial.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);

        // 1) Spawn a NEW top-level root (unrelated). Cache key
        //    (GlobalTransform::len) bumps; rescan must include it
        //    even though `parent` already had an entry.
        let new_root = spawn_leaf(&mut world, Vec3::new(50.0, 0.0, 0.0), 1.0, Vec3::ZERO, 2.0);
        sys(&world, 0.016);
        let new_root_wb = *world.query::<WorldBound>().unwrap().get(new_root).unwrap();
        assert!(
            (new_root_wb.center - Vec3::new(50.0, 0.0, 0.0)).length() < 1e-5,
            "new root must be discovered after cache invalidation, got {:?}",
            new_root_wb.center
        );

        // 2) Add a SECOND child to `parent` — Parent::len bumps. The
        //    parent's WorldBound must re-fold to enclose both leaves.
        let leaf2 = spawn_leaf(&mut world, Vec3::new(10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        world.insert(leaf2, Parent(parent));
        world.insert(parent, Children(vec![leaf, leaf2]));
        sys(&world, 0.016);
        let parent_wb = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!(
            (parent_wb.center - Vec3::ZERO).length() < 1e-5,
            "parent center should be midpoint after second child added, got {:?}",
            parent_wb.center
        );
        assert!(
            (parent_wb.radius - 11.0).abs() < 1e-5,
            "parent radius should enclose both leaves (r=11), got {}",
            parent_wb.radius
        );

        // 3) Promote `leaf2` to root by removing its Parent. Parent::len
        //    drops; rescan must include it. After the walk, `parent`'s
        //    WorldBound should fall back to enclosing only `leaf`.
        world.remove::<Parent>(leaf2);
        world.insert(parent, Children(vec![leaf]));
        sys(&world, 0.016);
        let parent_wb_after = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!(
            (parent_wb_after.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5,
            "parent should re-fold to single child after promote, got {:?}",
            parent_wb_after.center
        );
    }

    /// Steady-state cache hit: with no topology change, the cached
    /// root set must still drive a correct post-order walk so leaf
    /// transform changes propagate up to parent bounds (the
    /// counterpart to `root_cache_steady_state_still_runs_propagation`
    /// in #825 — confirms cache hits don't stall pass 2).
    #[test]
    fn root_cache_steady_state_still_refolds_parent_bounds() {
        let mut world = World::new();
        let parent = world.spawn();
        world.insert(parent, GlobalTransform::IDENTITY);
        world.insert(parent, WorldBound::ZERO);

        let leaf = spawn_leaf(&mut world, Vec3::new(-10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        world.insert(leaf, Parent(parent));
        world.insert(parent, Children(vec![leaf]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);
        let initial = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!((initial.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);

        // Move the leaf without any topology change — the cache key
        // stays valid, but pass 1 (leaf bound) and pass 2 (parent
        // fold) must still re-execute against the cached root.
        {
            let mut gq = world.query_mut::<GlobalTransform>().unwrap();
            let g = gq.get_mut(leaf).unwrap();
            g.translation = Vec3::new(20.0, 0.0, 0.0);
        }
        sys(&world, 0.016);
        let after = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!(
            (after.center - Vec3::new(20.0, 0.0, 0.0)).length() < 1e-5,
            "parent bound must re-fold against cached root after leaf moved, got {:?}",
            after.center
        );
    }

    /// FAST PATH (incremental rewrite): once bounds are computed, a frame
    /// with nothing moved and no topology change must leave every bound
    /// intact — the steady state for a static exterior cell. The second
    /// call drains an empty GlobalTransform dirty set and early-returns.
    #[test]
    fn fast_path_preserves_bounds_when_nothing_moves() {
        let mut world = World::new();
        let e = spawn_leaf(&mut world, Vec3::new(7.0, 0.0, 0.0), 1.0, Vec3::ZERO, 3.0);
        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016); // full rebuild (first frame, last_key None)
        sys(&world, 0.016); // fast path: nothing dirty, no structural change
        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.center - Vec3::new(7.0, 0.0, 0.0)).length() < 1e-5);
        assert!((wb.radius - 3.0).abs() < 1e-5);
    }

    /// A flat leaf (no parent/children — the bulk of an exterior cell) that
    /// moves between frames must have its bound recomputed via the
    /// GlobalTransform dirty set, even though no topology key changed. This
    /// is the incremental pass-1 path that depends on
    /// `GlobalTransform::TRACK_CHANGES`.
    #[test]
    fn flat_leaf_move_updates_bound_via_dirty_set() {
        let mut world = World::new();
        let e = spawn_leaf(&mut world, Vec3::new(1.0, 0.0, 0.0), 1.0, Vec3::ZERO, 2.0);
        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);
        // Move the leaf — get_mut marks GlobalTransform dirty (TRACK_CHANGES).
        {
            let mut gq = world.query_mut::<GlobalTransform>().unwrap();
            gq.get_mut(e).unwrap().translation = Vec3::new(100.0, 0.0, 0.0);
        }
        sys(&world, 0.016);
        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!(
            (wb.center - Vec3::new(100.0, 0.0, 0.0)).length() < 1e-5,
            "flat leaf bound must track its moved GlobalTransform, got {:?}",
            wb.center
        );
    }

    #[test]
    fn cyclic_parent_and_children_walks_terminate() {
        let mut world = World::new();
        let root = world.spawn();
        world.insert(root, GlobalTransform::IDENTITY);
        world.insert(root, WorldBound::ZERO);

        let leaf = spawn_leaf(&mut world, Vec3::new(5.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        let a = world.spawn();
        let b = world.spawn();
        for entity in [a, b] {
            world.insert(entity, GlobalTransform::IDENTITY);
            world.insert(entity, WorldBound::ZERO);
        }

        // Simulate a corrupt but structurally populated hierarchy. The root
        // reaches the a↔b Children cycle, while the Parent cycle is also
        // reachable from the dirty-root climb on the following frame.
        world.insert(leaf, Parent(root));
        world.insert(a, Parent(b));
        world.insert(b, Parent(a));
        world.insert(root, Children(vec![leaf, a]));
        world.insert(a, Children(vec![b]));
        world.insert(b, Children(vec![a]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        // Mark a node in the Parent cycle dirty without changing topology so
        // the incremental parent climb is exercised as well as post-order.
        {
            let mut gq = world.query_mut::<GlobalTransform>().unwrap();
            gq.get_mut(a).unwrap().translation.x = 1.0;
        }
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        assert!(wb_q.get(root).unwrap().center.is_finite());
    }
}
