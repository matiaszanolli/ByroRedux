use super::*;
use crate::components::CellRootIndex;
use rustc_hash::FxHashMap;

/// A job in the shape `begin_worldspace_persistent_cell` produces,
/// minus the ESM data the stamp path never reads.
fn job(world: &mut World, cell_root: EntityId) -> PersistentCellApplyJob {
    register_cell_root(world, cell_root);
    PersistentCellApplyJob {
        cell_root,
        cell_form_id: 0x0000_003C,
        authored_ref_count: 0,
        stamp_cursor: world.next_entity_id(),
        local_refs: Vec::new(),
        remote_actor_refs: Vec::new(),
        references: None,
        reference_entity_count: None,
        logical_stub_refs: Vec::new(),
        next_logical_stub: 0,
        logical_stub_scripts: 0,
        local_actor_3d: 0,
        local_actor_count: 0,
    }
}

/// #3379 — a multi-slice persistent apply must contribute each
/// entity to `CellRootIndex[cell_root]` exactly once.
///
/// Pre-fix the job stamped `constructor_first_entity..now` on every
/// yield *and* on completion, so slice 1's entities were pushed once
/// per remaining slice. `unload_cell` takes that vector as its
/// victim list and `drop_meshes` / `drop_textures` decrement once
/// per occurrence, over-dropping a mesh a resident cell may still
/// hold.
#[test]
fn every_slice_stamps_a_disjoint_range() {
    let mut world = World::new();
    world.insert_resource(CellRootIndex::new());
    let cell_root = world.spawn();
    let mut job = job(&mut world, cell_root);

    // Three cooperative slices, two entities each.
    let mut spawned = Vec::new();
    for _ in 0..3 {
        spawned.push(world.spawn());
        spawned.push(world.spawn());
        job.stamp_slice(&mut world);
    }

    let idx = world.resource::<CellRootIndex>();
    let entry = idx.map.get(&cell_root).expect("root registered");
    let mut distinct = entry.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        entry.len(),
        distinct.len(),
        "a three-slice apply duplicated its prefix: {entry:?}",
    );
    // The root itself is in the list (register_cell_root pushes it)
    // plus the six entities the slices created.
    assert_eq!(entry.len(), spawned.len() + 1);
    for eid in &spawned {
        assert!(entry.contains(eid), "entity {eid} was never stamped");
    }
}

/// #5483 — the cursor is reseeded at the top of every `advance`, so
/// an entity spawned while the job is parked between advances (the
/// bootstrap's camera/player, a crossing frame's LOD rings) is never
/// claimed. Pre-fix the cursor was seeded at construction and spanned
/// every inter-frame gap, so the first stamp adopted the camera, the
/// player capsule and the arrival cell into the persistent CELL —
/// and the worldspace teardown later despawned them.
#[test]
fn entities_spawned_between_advances_are_not_claimed() {
    let mut world = World::new();
    world.insert_resource(CellRootIndex::new());
    let cell_root = world.spawn();
    let mut job = job(&mut world, cell_root);

    // Foreign spawn while the job is parked (construction → first
    // advance, or any yield → next advance).
    let foreign = world.spawn();
    job.begin_advance(&world);
    // The job's own spawn inside the advance.
    let own = world.spawn();
    job.stamp_slice(&mut world);

    let idx = world.resource::<CellRootIndex>();
    let entry = &idx.map[&cell_root];
    assert!(
        !entry.contains(&foreign),
        "a foreign entity was claimed by the persistent CELL: {entry:?}",
    );
    assert!(entry.contains(&own), "the job's own entity was never stamped");
}

/// #3377 — cancelling an unfinished persistent apply must release
/// the clip handles its `ReferenceLoadJob` staged for cache-miss
/// REFRs.
///
/// Those handles are committed to `NifImportRegistry` only at
/// end-of-cell, so a cancelled cell never reaches the commit and
/// owes the registry an explicit release (the `#863` contract).
/// Pre-fix `PersistentCellApplyJob` had no `cancel` at all and
/// `drain_streaming_state` simply dropped it with the struct, so
/// every drain with an in-flight persistent apply pinned one
/// `AnimationClip` per staged handle for the process lifetime.
#[test]
fn cancel_releases_the_reference_jobs_staged_clip_handles() {
    use byroredux_core::animation::{AnimationClip, AnimationClipRegistry, CycleType};

    let mut world = World::new();
    world.insert_resource(CellRootIndex::new());
    world.insert_resource(AnimationClipRegistry::new());

    let handle = {
        let mut reg = world.resource_mut::<AnimationClipRegistry>();
        reg.add(AnimationClip {
            name: "staged".into(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: FxHashMap::default(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
            root_yaw: Vec::new(),
        })
    };
    assert!(
        world
            .resource::<AnimationClipRegistry>()
            .get(handle)
            .is_some_and(|clip| clip.duration > 0.0),
        "fixture clip must start populated",
    );

    let cell_root = world.spawn();
    let mut job = job(&mut world, cell_root);
    job.references = Some(ReferenceLoadJob::with_pending_clip_handles(
        [("meshes\\synthetic.nif".to_string(), handle)]
            .into_iter()
            .collect(),
    ));

    job.cancel(&world);

    assert!(
        world
            .resource::<AnimationClipRegistry>()
            .get(handle)
            .is_some_and(|clip| clip.duration == 0.0 && clip.channels.is_empty()),
        "the staged clip handle must be released back to an empty stub",
    );
}
