//! #1594 (FO4-D9-MEDIUM-02) — the FO4+ `BSConnectPoint` attach graph is
//! materialized onto the spawned placement root.
//!
//! Pre-fix the data was lifted to `ImportedScene` and then dropped at the
//! import boundary (no consumer). These tests pin the consumer half: the
//! `Imported*` → ECS-component conversion (name interning, parent-bone
//! handling) and the spawn-time stamping onto an entity, plus the
//! child-resolution-by-point-name path the #973 OMOD consumer will drive.

use super::nif_import_registry::CachedNifImport;
use super::references::{attach_points_component, child_attach_connections_component};
use super::spawn::stamp_attach_components;
use byroredux_core::ecs::components::AttachPoints;
use byroredux_core::ecs::World;
use byroredux_core::string::StringPool;
use byroredux_nif::import::{ImportedAttachPoint, ImportedChildAttachConnections};

fn cached_with_attach(
    attach_points: Option<AttachPoints>,
    child_attach_connections: Option<byroredux_core::ecs::components::ChildAttachConnections>,
) -> CachedNifImport {
    CachedNifImport {
        meshes: Vec::new(),
        collisions: Vec::new(),
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
        placement_root_billboard: None,
        bsx_flags: 0,
        root_flags: 0,
        flame_attach_offset: None,
        attach_points,
        child_attach_connections,
        furniture: None,
    }
}

/// The `Imported*` → `AttachPoints` conversion interns the names and maps an
/// empty `parent` to `None` (anchored on the host root, not a bone).
#[test]
fn attach_points_component_interns_and_maps_parent_bone() {
    let mut pool = StringPool::new();
    let imported = vec![
        ImportedAttachPoint {
            parent: "GunBoneReceiver".to_string(),
            name: "CON_Magazine".to_string(),
            rotation: [1.0, 0.0, 0.0, 0.0],
            translation: [0.0, 0.0, 1.5],
            scale: 1.0,
        },
        ImportedAttachPoint {
            parent: String::new(), // no bone → anchored on the host root
            name: "CON_Scope".to_string(),
            rotation: [1.0, 0.0, 0.0, 0.0],
            translation: [0.0, 2.0, 0.0],
            scale: 1.0,
        },
    ];
    let ap = attach_points_component(&imported, &mut pool);
    assert_eq!(ap.len(), 2);

    let con_mag = pool.intern("CON_Magazine");
    let mag = ap.find(con_mag).expect("CON_Magazine resolves");
    assert_eq!(mag.translation, [0.0, 0.0, 1.5]);
    assert!(mag.parent_bone.is_some(), "named parent → Some(bone)");
    assert_eq!(mag.parent_bone, Some(pool.intern("GunBoneReceiver")));

    let con_scope = pool.intern("CON_Scope");
    let scope = ap.find(con_scope).expect("CON_Scope resolves");
    assert!(scope.parent_bone.is_none(), "empty parent → None bone");
}

/// `stamp_attach_components` puts both components onto the placement root.
/// A cache entry with no connect-point data leaves the entity bare.
#[test]
fn stamp_attach_components_materializes_onto_root_entity() {
    let mut pool = StringPool::new();
    let imported = vec![ImportedAttachPoint {
        parent: String::new(),
        name: "CON_Scope".to_string(),
        rotation: [1.0, 0.0, 0.0, 0.0],
        translation: [0.0, 0.0, 2.0],
        scale: 1.0,
    }];
    let ap = attach_points_component(&imported, &mut pool);
    let cac = child_attach_connections_component(
        &ImportedChildAttachConnections {
            point_names: vec!["CON_Scope".to_string()],
            skinned: true,
        },
        &mut pool,
    );

    let mut world = World::new();
    let root = world.spawn();
    stamp_attach_components(&mut world, root, &cached_with_attach(Some(ap), Some(cac)));

    let con_scope = pool.intern("CON_Scope");
    {
        let stamped = world
            .get::<AttachPoints>(root)
            .expect("AttachPoints stamped onto the placement root");
        assert!(stamped.find(con_scope).is_some(), "CON_Scope reachable on the entity");

        let conns = world
            .get::<byroredux_core::ecs::components::ChildAttachConnections>(root)
            .expect("ChildAttachConnections stamped onto the placement root");
        assert!(conns.skinned);
        assert_eq!(conns.connect_names, vec![con_scope]);
    }

    // A bare cache entry stamps nothing.
    let bare = world.spawn();
    stamp_attach_components(&mut world, bare, &cached_with_attach(None, None));
    assert!(world.get::<AttachPoints>(bare).is_none());
}

/// Child resolution by point name: every name a `ChildAttachConnections`
/// references must resolve to an `AttachPoint` exposed on the parent's
/// `AttachPoints` — the lookup the #973 OMOD consumer performs to mount a
/// modular accessory.
#[test]
fn child_connections_resolve_against_parent_attach_points() {
    let mut pool = StringPool::new();
    let parent = attach_points_component(
        &[
            ImportedAttachPoint {
                parent: String::new(),
                name: "CON_Scope".to_string(),
                rotation: [1.0, 0.0, 0.0, 0.0],
                translation: [0.0, 0.0, 2.0],
                scale: 1.0,
            },
            ImportedAttachPoint {
                parent: String::new(),
                name: "CON_Magazine".to_string(),
                rotation: [1.0, 0.0, 0.0, 0.0],
                translation: [0.0, -1.5, 0.0],
                scale: 1.0,
            },
        ],
        &mut pool,
    );
    let child = child_attach_connections_component(
        &ImportedChildAttachConnections {
            point_names: vec!["CON_Scope".to_string()],
            skinned: false,
        },
        &mut pool,
    );

    for name in &child.connect_names {
        assert!(
            parent.find(*name).is_some(),
            "child connect name must resolve to a parent attach point"
        );
    }
    // A name the parent doesn't expose must NOT resolve.
    assert!(parent.find(pool.intern("CON_Stock")).is_none());
}
