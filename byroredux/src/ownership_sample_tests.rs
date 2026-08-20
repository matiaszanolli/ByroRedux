//! Collection-side tests for the EX-08 ownership sampler (#2374).
//!
//! These run against a bare `World` with no Vulkan device, which is exactly
//! the property that makes the ECS half of the sampler worth splitting out:
//! most of EX-08's owner classes are ECS rows or resources, so most of the
//! collection contract is verifiable in `cargo test`.

use super::*;
use byroredux_core::ecs::components::{
    CellRoot, PrecombinedMesh, WaterKind, WaterMaterial, WaterPlane,
};

/// `WaterPlane` has no `Default` (its `kind` has no meaningful neutral value),
/// so the row-counting tests build the calm variant explicitly.
fn calm_water() -> WaterPlane {
    WaterPlane {
        kind: WaterKind::Calm,
        material: WaterMaterial::default(),
    }
}

#[test]
fn empty_world_samples_all_zero_except_the_id_watermark() {
    let world = World::new();
    let mut snap = OwnershipSnapshot::default();
    sample_ecs_owners(&world, &mut snap);

    // Every class must read zero on an empty world; a non-zero here would mean
    // the sampler is reporting capacity rather than occupancy, which would put
    // a permanent false floor under the soak baseline.
    for class in snap.classes() {
        assert_eq!(
            class.value, 0,
            "class {} non-zero on empty world",
            class.name
        );
    }
}

#[test]
fn absent_resources_read_zero_rather_than_panicking() {
    // The loose-NIF demo path installs neither physics nor audio, and headless
    // CI has no audio device. Sampling must survive that — a soak harness that
    // panicked on a subsystem-less run would be useless in CI.
    let world = World::new();
    let mut snap = OwnershipSnapshot::default();
    sample_ecs_owners(&world, &mut snap);
    assert_eq!(snap.physics_bodies, 0);
    assert_eq!(snap.audio_active_sounds, 0);
    assert_eq!(snap.audio_pending_oneshots, 0);
    assert_eq!(snap.sound_cache_entries, 0);
    assert_eq!(snap.cell_root_index_entries, 0);
}

#[test]
fn component_rows_are_counted_per_class() {
    let mut world = World::new();
    let root = world.spawn();

    let a = world.spawn();
    world.insert(a, Transform::default());
    world.insert(a, CellRoot(root));
    world.insert(a, calm_water());
    world.insert(a, PrecombinedMesh);

    let b = world.spawn();
    world.insert(b, Transform::default());
    world.insert(b, CellRoot(root));

    let mut snap = OwnershipSnapshot::default();
    sample_ecs_owners(&world, &mut snap);

    assert_eq!(snap.transform_rows, 2);
    assert_eq!(snap.cell_root_rows, 2);
    assert_eq!(snap.water_planes, 1);
    // Only `a` is precombine-owned; `b` is an ordinary per-REFR entity —
    // pins that the class counts the marker, not `RenderLayer::Architecture`
    // or `CellRoot` membership in general (EX-15 / #2369).
    assert_eq!(snap.precombine_mesh_rows, 1);
    // `next_entity_id` counts the root too — it is the allocator watermark,
    // not a live count, which is why its reclaim policy is Bounded.
    assert_eq!(snap.entities_spawned, 3);
}

#[test]
fn despawn_returns_ecs_classes_to_baseline() {
    // The core EX-08 shape in miniature: sample, load, unload, re-sample. The
    // exact-return classes must come back; the id watermark must not.
    let mut world = World::new();
    let mut before = OwnershipSnapshot::default();
    sample_ecs_owners(&world, &mut before);

    let root = world.spawn();
    let mut victims = Vec::new();
    for i in 0..8 {
        let e = world.spawn();
        world.insert(e, Transform::default());
        world.insert(e, CellRoot(root));
        if i < 3 {
            world.insert(e, PrecombinedMesh);
        }
        victims.push(e);
    }
    let mut loaded = OwnershipSnapshot::default();
    sample_ecs_owners(&world, &mut loaded);
    assert_eq!(loaded.transform_rows, 8);
    assert_eq!(loaded.precombine_mesh_rows, 3);

    world.despawn_batch(victims);
    world.despawn(root);

    let mut after = OwnershipSnapshot::default();
    sample_ecs_owners(&world, &mut after);
    assert_eq!(after.transform_rows, before.transform_rows);
    assert_eq!(after.cell_root_rows, before.cell_root_rows);
    // The EX-15 / #2369 shape: precombine-owned entities must return to
    // baseline exactly like every other `Exact` class — no residue from
    // being a distinct component rather than folded into `cell_root_rows`.
    assert_eq!(after.precombine_mesh_rows, before.precombine_mesh_rows);
    assert!(
        after.entities_spawned > before.entities_spawned,
        "entity ids must stay monotonic across despawn"
    );
}

#[test]
fn cell_root_index_entries_track_resident_cells_not_entities() {
    // The index is keyed by cell root. Ten entities in one cell is one entry —
    // getting this backwards would make the class scale with cell population
    // and drown the actual signal (a root that outlived its unload).
    let mut world = World::new();
    world.insert_resource(CellRootIndex::new());
    let root_a = world.spawn();
    let root_b = world.spawn();
    {
        let mut idx = world.resource_mut::<CellRootIndex>();
        idx.map.insert(root_a, (0..10).map(|_| 1_u32).collect());
        idx.map.insert(root_b, vec![2]);
    }

    let mut snap = OwnershipSnapshot::default();
    sample_ecs_owners(&world, &mut snap);
    assert_eq!(snap.cell_root_index_entries, 2);
}

#[test]
fn handle_counts_dedup_and_skip_zero() {
    // Must match `DebugStats::meshes_in_use` semantics exactly: distinct,
    // non-zero. Handle 0 is the shared placeholder and is never per-cell
    // refcounted, so counting it would put a constant offset on the class.
    let mut world = World::new();
    for handle in [0_u32, 5, 5, 7] {
        let e = world.spawn();
        world.insert(e, MeshHandle(handle));
        world.insert(e, TextureHandle(handle));
    }
    let (meshes, textures) = count_handles_in_use(&world);
    assert_eq!(meshes, 2, "expected distinct non-zero handles {{5,7}}");
    assert_eq!(textures, 2);
}

#[test]
fn sample_all_without_a_renderer_leaves_gpu_classes_zero() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Transform::default());
    world.insert(e, MeshHandle(9));

    let snap = sample_all(&world, None, 0, 0);
    assert_eq!(snap.transform_rows, 1);
    // No context — the GPU half must stay untouched rather than guessing from
    // the ECS side, or a headless run would report a phantom registry.
    assert_eq!(snap.meshes_registry, 0);
    assert_eq!(snap.blas_entries, 0);
    assert_eq!(snap.terrain_tiles, 0);
}
