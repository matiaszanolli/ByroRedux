//! Collision-proxy synthesis + trimesh-ghost spawn tests.
//!
//! Extracted from `spawn.rs`'s inline test module (#2410 / TD1-007).

use super::{
    missing_collision_fallback, spawn_packed_havok_proxy, spawn_trimesh_collider_ghost,
    synthesize_packed_havok_proxy, synthesize_static_trimesh, transformed_mesh_aabb,
    MissingCollisionFallback, ProxyMeshGeometry,
};
use byroredux_core::{
    ecs::{
        components::{
            CollisionShape, MeshHandle, MotionType, PhysicsSourceForm, RenderLayer, RigidBodyData,
        },
        World,
    },
    form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId},
    math::{Quat, Vec3},
};
use byroredux_nif::import::collision::CollisionAuthoringSummary;

/// A single unit triangle synthesizes into a 1-triangle TriMesh
/// with a Static body. Baseline that the geometry round-trips.
#[test]
fn single_triangle_round_trips() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let indices = [0u32, 1, 2];
    let (shape, body) = synthesize_static_trimesh(&positions, &indices, 1.0).expect("one triangle");
    match shape {
        CollisionShape::TriMesh { vertices, indices } => {
            assert_eq!(vertices.len(), 3);
            assert_eq!(indices, vec![[0, 1, 2]]);
        }
        other => panic!("expected TriMesh, got {other:?}"),
    }
    assert_eq!(body.motion_type, MotionType::Static);
}

/// LAND terrain and NIF architecture both call this helper for missing
/// authored collision. Pin the shared floor contract: one static physics
/// proxy, with no render mesh/BLAS payload of its own.
#[test]
fn floor_collider_ghost_is_static_and_renderer_free() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let indices = [0u32, 1, 2];
    let mut world = World::new();

    assert!(spawn_trimesh_collider_ghost(
        &mut world,
        &positions,
        &indices,
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        None,
    ));

    let shape_q = world
        .query::<CollisionShape>()
        .expect("ghost must carry CollisionShape");
    let (entity, _) = shape_q.iter().next().expect("one collider ghost");
    assert_eq!(shape_q.iter().count(), 1);

    let body_q = world
        .query::<RigidBodyData>()
        .expect("ghost must carry RigidBodyData");
    assert_eq!(
        body_q
            .get(entity)
            .expect("same ghost owns the body")
            .motion_type,
        MotionType::Static
    );

    let mesh_q = world.query::<MeshHandle>();
    assert!(
        mesh_q.as_ref().is_none_or(|q| !q.contains(entity)),
        "physics floor proxy must not create an extra raster/TLAS instance"
    );
}

#[test]
fn placement_trimesh_ghost_keeps_reference_ownership_backlink() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let indices = [0u32, 1, 2];
    let mut world = World::new();
    let mut pool = FormIdPool::new();
    let source_form = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x1234),
    });

    assert!(spawn_trimesh_collider_ghost(
        &mut world,
        &positions,
        &indices,
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        Some(source_form),
    ));

    let backlink = world
        .query::<PhysicsSourceForm>()
        .and_then(|query| query.iter().next().map(|(_, source)| *source));
    assert_eq!(backlink, Some(PhysicsSourceForm(source_form)));
}

/// `world_scale` bakes into the vertex positions — the physics sync
/// ignores `GlobalTransform` scale, so the collider must carry it.
#[test]
fn world_scale_bakes_into_vertices() {
    let positions = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
    let indices = [0u32, 1, 2];
    let (shape, _) = synthesize_static_trimesh(&positions, &indices, 2.0).expect("scaled triangle");
    match shape {
        CollisionShape::TriMesh { vertices, .. } => {
            assert_eq!(vertices[0].to_array(), [2.0, 4.0, 6.0]);
            assert_eq!(vertices[2].to_array(), [14.0, 16.0, 18.0]);
        }
        other => panic!("expected TriMesh, got {other:?}"),
    }
}

/// Fewer than 3 indices → no triangle → `None`.
#[test]
fn degenerate_index_count_returns_none() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
    assert!(synthesize_static_trimesh(&positions, &[0, 1], 1.0).is_none());
    assert!(synthesize_static_trimesh(&positions, &[], 1.0).is_none());
}

/// Triangles that reference out-of-range vertices are dropped (a
/// corrupt index buffer must not reach parry3d's trimesh builder,
/// which would panic). When every triangle is out of range the
/// result is `None`.
#[test]
fn out_of_range_indices_are_dropped() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    // Second triangle references vertex 9 (out of range, only 3
    // verts). First triangle is valid.
    let indices = [0u32, 1, 2, 0, 1, 9];
    let (shape, _) =
        synthesize_static_trimesh(&positions, &indices, 1.0).expect("one valid triangle");
    match shape {
        CollisionShape::TriMesh { indices, .. } => {
            assert_eq!(indices, vec![[0, 1, 2]], "out-of-range triangle dropped");
        }
        other => panic!("expected TriMesh, got {other:?}"),
    }
    // All-out-of-range → None.
    let all_bad = [9u32, 10, 11];
    assert!(synthesize_static_trimesh(&positions, &all_bad, 1.0).is_none());
}

#[test]
fn collision_authoring_selects_packed_proxy_only_for_safe_layers() {
    let packed = CollisionAuthoringSummary {
        new_physics: 1,
        ..Default::default()
    };
    assert_eq!(
        missing_collision_fallback(true, packed, RenderLayer::Clutter),
        MissingCollisionFallback::PackedAabbProxy,
    );
    assert_eq!(
        missing_collision_fallback(true, packed, RenderLayer::Actor),
        MissingCollisionFallback::PackedAabbProxy,
    );
    assert_eq!(
        missing_collision_fallback(true, packed, RenderLayer::Architecture),
        MissingCollisionFallback::ArchitectureTriMesh,
    );
    assert_eq!(
        missing_collision_fallback(true, packed, RenderLayer::Decal),
        MissingCollisionFallback::None,
    );
    assert_eq!(
        missing_collision_fallback(
            true,
            CollisionAuthoringSummary::default(),
            RenderLayer::Clutter,
        ),
        MissingCollisionFallback::None,
        "clutter with no authored packed collision must remain non-colliding",
    );
    assert_eq!(
        missing_collision_fallback(false, packed, RenderLayer::Clutter),
        MissingCollisionFallback::None,
        "decoded collision always wins over a compatibility proxy",
    );
}

#[test]
fn packed_proxy_aabb_unions_mesh_local_transforms() {
    let a = [[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]];
    let b = [[0.0, 0.0, 0.0], [2.0, 4.0, 6.0]];
    let (min, max) = transformed_mesh_aabb([
        ProxyMeshGeometry {
            positions: &a,
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 1.0,
        },
        ProxyMeshGeometry {
            positions: &b,
            translation: Vec3::new(10.0, 0.0, 0.0),
            rotation: Quat::IDENTITY,
            scale: 0.5,
        },
    ])
    .expect("finite mesh geometry must produce an AABB");

    assert_eq!(min, Vec3::new(-1.0, -2.0, -3.0));
    assert_eq!(max, Vec3::new(11.0, 2.0, 3.0));
}

#[test]
fn packed_proxy_bakes_outer_scale_into_cuboid_extent() {
    let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
        vec![[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    mesh.translation = [4.0, 5.0, 6.0];

    let (center, shape) = synthesize_packed_havok_proxy(&[mesh], 2.0)
        .expect("finite render geometry must produce a packed-Havok proxy");
    assert_eq!(center, Vec3::new(4.0, 5.0, 6.0));
    match shape {
        CollisionShape::Cuboid { half_extents } => {
            assert_eq!(half_extents, Vec3::new(2.0, 4.0, 6.0));
        }
        other => panic!("expected Cuboid, got {other:?}"),
    }
}

/// Regression for #2543: an extreme-but-finite `ref_scale` (unclamped
/// REFR `XSCL` read straight off disk) must not hand back a
/// `Cuboid` whose half-extents dwarf the world — it must clamp to
/// `RT_ABSOLUTE_PRECISION_CEILING` instead of multiplying straight
/// through.
#[test]
fn packed_proxy_clamps_extreme_finite_scale() {
    let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
        vec![[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    mesh.translation = [4.0, 5.0, 6.0];

    // Large enough to blow well past `RT_ABSOLUTE_PRECISION_CEILING`
    // (~1e6) but nowhere near `f32::MAX` (~3.4e38), so the product
    // stays finite — this is the "corrupt-but-finite" case the debug
    // assert alone can't catch in release builds.
    let (_, shape) = synthesize_packed_havok_proxy(&[mesh], 1.0e30)
        .expect("a finite (if extreme) scale must still produce a clamped proxy");
    match shape {
        CollisionShape::Cuboid { half_extents } => {
            assert!(
                half_extents.is_finite(),
                "clamped half_extents must stay finite: {half_extents:?}"
            );
            assert!(
                half_extents.x <= super::super::references::RT_ABSOLUTE_PRECISION_CEILING
                    && half_extents.y <= super::super::references::RT_ABSOLUTE_PRECISION_CEILING
                    && half_extents.z <= super::super::references::RT_ABSOLUTE_PRECISION_CEILING,
                "half_extents {half_extents:?} must be clamped to the sane-magnitude ceiling"
            );
        }
        other => panic!("expected Cuboid, got {other:?}"),
    }
}

/// Regression for #2543: a non-finite product (e.g. an `f32` overflow
/// during the scale multiply) must reject the proxy outright rather
/// than propagate `Infinity`/`NaN` half-extents into the ECS.
#[test]
fn packed_proxy_rejects_non_finite_half_extents() {
    let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
        vec![[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    mesh.translation = [4.0, 5.0, 6.0];
    // Finite input scale, but `f32::MAX * f32::MAX` overflows to
    // `Infinity` inside `synthesize_packed_havok_proxy`'s multiply —
    // this must not slip through as a literal-infinite collider.
    mesh.scale = f32::MAX;

    assert!(
        synthesize_packed_havok_proxy(&[mesh], f32::MAX).is_none(),
        "an overflowing (non-finite) half-extents product must reject the proxy"
    );
}

/// Regression for #2531 / NIFAL-D6-NEW-01: a skinned mesh's bind-pose
/// `positions` (splayed T-pose-wide) must NOT be unioned directly into
/// the proxy AABB — the mesh's own pose-independent `local_bound_*`
/// (deliberately tight here) must be used instead. Proves the fix by
/// using positions wide enough that, if they leaked through unfiltered,
/// the resulting cuboid would be far larger than the tight bound allows.
#[test]
fn packed_proxy_uses_local_bound_not_bind_pose_positions_for_skinned_mesh() {
    let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
        // T-pose-wide bind-pose positions — splayed limbs, ±50 units.
        vec![[-50.0, 0.0, 0.0], [50.0, 0.0, 0.0]],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    mesh.skin = Some(byroredux_nif::import::ImportedSkin::default());
    // Tight, pose-independent local bound — what the fix must use instead.
    mesh.local_bound_center = [0.0, 0.0, 0.0];
    mesh.local_bound_radius = 2.0;

    let (center, shape) = synthesize_packed_havok_proxy(&[mesh], 1.0)
        .expect("a skinned-only mesh set must still produce a proxy (not #2355's regression)");
    assert_eq!(center, Vec3::ZERO);
    match shape {
        CollisionShape::Cuboid { half_extents } => {
            assert!(
                half_extents.x <= 2.0 && half_extents.y <= 2.0 && half_extents.z <= 2.0,
                "half_extents {half_extents:?} must come from the tight local_bound_radius \
                 (2.0), not the ±50 bind-pose positions — a T-pose leaking through would \
                 produce half_extents around 50.0"
            );
        }
        other => panic!("expected Cuboid, got {other:?}"),
    }
}

/// Companion: a rigid (non-skinned) mesh and a skinned mesh together
/// must union BOTH contributions — the rigid mesh's real positions and
/// the skinned mesh's local bound sphere — into one proxy, proving the
/// skin filter doesn't silently drop the rigid geometry it sits
/// alongside (e.g. a creature's non-skinned prop/weapon submesh).
#[test]
fn packed_proxy_unions_rigid_positions_and_skinned_local_bound() {
    let rigid = byroredux_nif::import::ImportedMesh::from_geometry(
        vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let mut skinned = byroredux_nif::import::ImportedMesh::from_geometry(
        vec![[-50.0, -50.0, -50.0], [50.0, 50.0, 50.0]],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    skinned.skin = Some(byroredux_nif::import::ImportedSkin::default());
    skinned.translation = [10.0, 0.0, 0.0];
    skinned.local_bound_center = [0.0, 0.0, 0.0];
    skinned.local_bound_radius = 1.0;

    let (_, shape) = synthesize_packed_havok_proxy(&[rigid, skinned], 1.0)
        .expect("mixed rigid + skinned mesh set must produce a proxy");
    match shape {
        CollisionShape::Cuboid { half_extents } => {
            // Rigid mesh spans x in [0, 1]; skinned sphere (radius 1,
            // translated +10) spans x in [9, 11] — union half-extent
            // on X must reach at least (11 - 0) / 2 = 5.5, and must NOT
            // reach anywhere near the skinned mesh's raw ±50 extent
            // (which would push it to ~30).
            assert!(
                half_extents.x >= 5.0,
                "union must include both meshes' contributions: {half_extents:?}"
            );
            assert!(
                half_extents.x < 15.0,
                "the skinned mesh's raw ±50 bind-pose positions must not leak into \
                 the union: {half_extents:?}"
            );
        }
        other => panic!("expected Cuboid, got {other:?}"),
    }
}

#[test]
fn packed_proxy_is_keyframed_and_parented_to_visual_placement() {
    use crate::cell_loader::nif_import_registry::CachedNifImport;
    use byroredux_core::ecs::{GlobalTransform, Parent, Transform};

    let mesh = byroredux_nif::import::ImportedMesh::from_geometry(
        vec![[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let cached = CachedNifImport {
        meshes: vec![mesh],
        collisions: Vec::new(),
        collision_authoring: CollisionAuthoringSummary {
            new_physics: 1,
            ..Default::default()
        },
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
        placement_root_billboard: None,
        bsx_flags: 0,
        root_flags: 0,
        flame_attach_offset: None,
        attach_points: None,
        child_attach_connections: None,
        furniture: None,
    };
    let mut world = World::new();
    let root = world.spawn();
    world.insert(root, Transform::default());
    world.insert(root, GlobalTransform::default());

    assert!(spawn_packed_havok_proxy(
        &mut world,
        &cached,
        root,
        Vec3::new(10.0, 20.0, 30.0),
        Quat::IDENTITY,
        1.0,
        None,
        RenderLayer::Clutter,
    ));

    let bodies = world
        .query::<RigidBodyData>()
        .expect("proxy must carry a rigid body");
    let (proxy, body) = bodies.iter().next().expect("one proxy body");
    assert_eq!(body.motion_type, MotionType::Keyframed);
    let parents = world.query::<Parent>().expect("proxy must be parented");
    assert_eq!(parents.get(proxy).map(|p| p.0), Some(root));
    let meshes = world.query::<MeshHandle>();
    assert!(meshes.as_ref().is_none_or(|q| !q.contains(proxy)));
}
