//! Regression tests for #544 — embedded animation-clip channels must
//! resolve through the per-REFR placement-root subtree.
//!
//! These tests don't exercise `spawn_placed_instances` directly (it
//! requires a `VulkanContext` for mesh upload, which CI can't stand
//! up), but they pin the **shape** of the entity graph the cell
//! loader must produce: a placement-root entity with `Children` →
//! per-mesh entities carrying `Name` + `Parent`. The
//! `build_subtree_name_map` walker — the same one
//! `AnimationStack` consumes through `SubtreeCache` to bind
//! channel keys — must rediscover the named meshes through the
//! root.
//!
//! Pre-#544 the cell loader spawned every mesh as a flat,
//! independently-anchored entity with no `Name`, so even if an
//! `AnimationPlayer` had been wired the subtree walk would have
//! found zero named entities under any root and silently no-op'd.

use byroredux_core::ecs::{Children, Name, Parent, Transform, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;

use crate::anim_convert::build_subtree_name_map;
use crate::helpers::add_child;

/// Spawn a placement root + N named mesh children mimicking the
/// `spawn_placed_instances` output, then verify the subtree walker
/// discovers every named mesh by its `FixedString` symbol — the
/// exact lookup the animation system performs at frame time.
#[test]
fn subtree_walker_finds_named_meshes_under_placement_root() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());

    // Spawn the placement root at an arbitrary REFR transform; the
    // animation system never reads the root's transform itself, only
    // its `Children`.
    let placement_root = world.spawn();
    world.insert(
        placement_root,
        Transform::new(
            Vec3::new(100.0, 50.0, -25.0),
            Quat::IDENTITY,
            1.5,
        ),
    );

    // Three mesh children with distinct names — represents a NIF
    // whose embedded clip targets three named NiObjectNETs (e.g.
    // water plane + torch flame + flickering bulb on a single
    // tavern mesh).
    let names = ["WaterPlane", "TorchFlame", "FlickerBulb"];
    let mut mesh_entities = Vec::with_capacity(names.len());
    for name in names {
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        let sym = {
            let mut pool = world.resource_mut::<StringPool>();
            pool.intern(name)
        };
        world.insert(entity, Name(sym));
        world.insert(entity, Parent(placement_root));
        add_child(&mut world, placement_root, entity);
        mesh_entities.push((name, entity, sym));
    }

    let map = build_subtree_name_map(&world, placement_root);

    assert_eq!(
        map.len(),
        names.len(),
        "subtree walker must rediscover every named mesh under the \
         placement root — pre-#544 cell-loader-spawned meshes had \
         no Name, no Parent, and no Children edges, so this map \
         was always empty"
    );
    for (name, entity, sym) in &mesh_entities {
        assert_eq!(
            map.get(sym).copied(),
            Some(*entity),
            "channel keyed on '{name}' must resolve to its mesh entity"
        );
    }
}

/// The placement root itself doesn't need a Name for its children
/// to be discoverable — the walker's BFS starts at `root` and pushes
/// children onto the queue regardless of whether the root entity has
/// its own `Name` component. This locks in the contract that the
/// cell loader can leave the root unnamed without breaking
/// animation binding.
#[test]
fn unnamed_placement_root_still_yields_named_children() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());

    let placement_root = world.spawn();
    world.insert(placement_root, Transform::IDENTITY);
    // Deliberately no Name on the root.

    let child = world.spawn();
    world.insert(child, Transform::IDENTITY);
    let child_sym = {
        let mut pool = world.resource_mut::<StringPool>();
        pool.intern("AnimatedDoorPanel")
    };
    world.insert(child, Name(child_sym));
    world.insert(child, Parent(placement_root));
    add_child(&mut world, placement_root, child);

    let map = build_subtree_name_map(&world, placement_root);
    assert_eq!(map.len(), 1, "root carries no Name; one child does");
    assert_eq!(map.get(&child_sym).copied(), Some(child));
}

/// Two REFRs of the same model produce two independent subtrees.
/// Each `AnimationPlayer.root_entity` indexes the right subtree —
/// crucially, the same `mesh.name` shared across the two NIFs maps
/// to *different* entities depending on which placement root the
/// walker starts from. This is what lets the audit's "every torch
/// in the cell animates independently" outcome work; a global
/// name index would collide and reuse the same channel target for
/// every torch.
#[test]
fn two_placement_roots_produce_independent_subtrees() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());

    let sym = {
        let mut pool = world.resource_mut::<StringPool>();
        pool.intern("FlameNode")
    };

    // Placement A — "torch in the corner".
    let root_a = world.spawn();
    world.insert(root_a, Transform::IDENTITY);
    let mesh_a = world.spawn();
    world.insert(mesh_a, Transform::IDENTITY);
    world.insert(mesh_a, Name(sym));
    world.insert(mesh_a, Parent(root_a));
    add_child(&mut world, root_a, mesh_a);

    // Placement B — "torch by the door". Same model, different
    // location → two distinct entities sharing one `Name` symbol.
    let root_b = world.spawn();
    world.insert(root_b, Transform::IDENTITY);
    let mesh_b = world.spawn();
    world.insert(mesh_b, Transform::IDENTITY);
    world.insert(mesh_b, Name(sym));
    world.insert(mesh_b, Parent(root_b));
    add_child(&mut world, root_b, mesh_b);

    let map_a = build_subtree_name_map(&world, root_a);
    let map_b = build_subtree_name_map(&world, root_b);

    assert_eq!(
        map_a.get(&sym).copied(),
        Some(mesh_a),
        "subtree A must resolve `FlameNode` to its own mesh"
    );
    assert_eq!(
        map_b.get(&sym).copied(),
        Some(mesh_b),
        "subtree B must resolve `FlameNode` to its own mesh — NOT \
         the one from A. Without per-REFR scoping the audit's \
         multi-torch case would all animate as one"
    );
    assert_ne!(
        map_a.get(&sym),
        map_b.get(&sym),
        "the two roots must be fully independent"
    );
}
