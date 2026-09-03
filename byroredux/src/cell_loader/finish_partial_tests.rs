//! Regression tests for `finish_partial_import` — issue #864.
//!
//! The early-out at the top of `finish_partial_import` shorts the
//! main-thread material merge + clip-conversion + cache-insert pipeline when
//! `NifImportRegistry` already carries an entry for the model path.
//! Without it, a streaming-worker payload arriving for an already-
//! cached model (possible because the cached-keys snapshot in #862
//! lags the registry by the in-flight worker's parse latency) would
//! re-run `convert_nif_clip`, leak the previous clip handle into
//! `AnimationClipRegistry`, and overwrite the cache entry.

use super::*;
use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::ecs::World;
use byroredux_core::string::StringPool;
use byroredux_nif::{
    blocks::{
        base::{NiAVObjectData, NiObjectNETData},
        node::NiNode,
        tri_shape::{NiTriShape, NiTriShapeData},
        NiObject,
    },
    scene::NifScene,
    types::{BlockRef, NiPoint3, NiTransform},
};
use std::sync::Arc as StdArc;

fn dummy_cached() -> Arc<CachedNifImport> {
    Arc::new(CachedNifImport {
        meshes: Vec::new(),
        geometry_dedup: Vec::new(),
        collisions: Vec::new(),
        collision_authoring: Default::default(),
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
        placement_root_billboard: None,
        speedtree_wind: None,
        bsx_flags: 0,
        root_flags: 0,
        flame_attach_offset: None,
        attach_points: None,
        child_attach_connections: None,
        furniture: None,
    })
}

fn dummy_partial() -> crate::streaming::PartialNifImport {
    dummy_partial_with(0)
}

fn dummy_partial_with(bsx: u32) -> crate::streaming::PartialNifImport {
    crate::streaming::PartialNifImport {
        scene: byroredux_nif::scene::NifScene::default(),
        meshes: Vec::new(),
        collisions: Vec::new(),
        worker_pool: StringPool::new(),
        bsx,
        root_flags: 0,
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
    }
}

fn marker_scene(include_real_geometry: bool) -> NifScene {
    fn av(name: &str) -> NiAVObjectData {
        NiAVObjectData {
            net: NiObjectNETData {
                name: Some(StdArc::from(name)),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        }
    }
    fn shape(name: &str, data_ref: u32) -> NiTriShape {
        NiTriShape {
            av: av(name),
            data_ref: BlockRef(data_ref),
            skin_instance_ref: BlockRef::NULL,
            shader_property_ref: BlockRef::NULL,
            alpha_property_ref: BlockRef::NULL,
            num_materials: 0,
            active_material_index: 0,
        }
    }
    fn data() -> NiTriShapeData {
        NiTriShapeData {
            vertices: vec![
                NiPoint3::default(),
                NiPoint3 {
                    x: 1.0,
                    ..NiPoint3::default()
                },
                NiPoint3 {
                    y: 1.0,
                    ..NiPoint3::default()
                },
            ],
            normals: vec![
                NiPoint3 {
                    z: 1.0,
                    ..NiPoint3::default()
                };
                3
            ],
            center: NiPoint3 {
                x: 0.33,
                y: 0.33,
                z: 0.0,
            },
            radius: 1.0,
            vertex_colors: Vec::new(),
            uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
            triangles: vec![[0, 1, 2]],
        }
    }

    let children = if include_real_geometry {
        vec![BlockRef(1), BlockRef(3)]
    } else {
        vec![BlockRef(1)]
    };
    let root = NiNode {
        av: av("Scene Root"),
        children,
        effects: Vec::new(),
    };
    let mut blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(root),
        Box::new(shape("EditorMarker", 2)),
        Box::new(data()),
    ];
    if include_real_geometry {
        blocks.push(Box::new(shape("StoolGeometry", 4)));
        blocks.push(Box::new(data()));
    }
    NifScene {
        blocks,
        root_index: Some(0),
        bsver: 34,
        ..NifScene::default()
    }
}

fn partial_with_marker_scene(include_real_geometry: bool) -> crate::streaming::PartialNifImport {
    let scene = marker_scene(include_real_geometry);
    let mut worker_pool = StringPool::new();
    let (meshes, collisions) =
        byroredux_nif::import::import_nif_with_collision(&scene, &mut worker_pool);
    crate::streaming::PartialNifImport {
        scene,
        meshes,
        collisions,
        worker_pool,
        bsx: 0x20,
        root_flags: 0,
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
    }
}

fn world_with_registries() -> World {
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(AnimationClipRegistry::new());
    world.insert_resource(NifImportRegistry::new());
    world
}

/// A worker-local import produces `FixedString` symbols that cannot be
/// consumed with the world's pool until the drain re-interns them.
#[test]
fn finish_partial_import_reinterns_worker_material_symbols() {
    let mut worker_pool = StringPool::new();
    let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    mesh.material.material_path = Some(worker_pool.intern("materials\\worker.bgsm"));
    mesh.material.textures.base_color = Some(worker_pool.intern("textures\\worker.dds"));

    let partial = crate::streaming::PartialNifImport {
        scene: NifScene::default(),
        meshes: vec![mesh],
        collisions: Vec::new(),
        worker_pool,
        bsx: 0,
        root_flags: 0,
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
    };
    let mut world = world_with_registries();
    // Ensure worker and world symbol indices differ; the test must exercise
    // string resolution/re-interning rather than accidentally pass through
    // identical numeric handles.
    world
        .resource_mut::<StringPool>()
        .intern("world/preexisting-symbol");

    finish_partial_import(&mut world, None, "worker.nif", partial);

    let (material_path, base_color) = {
        let reg = world.resource::<NifImportRegistry>();
        let cached = reg
            .get("meshes\\worker.nif")
            .expect("worker import inserted into cache")
            .as_ref()
            .expect("worker import is positive");
        (
            cached.meshes[0]
                .material
                .material_path
                .expect("material path survived re-interning"),
            cached.meshes[0]
                .material
                .textures
                .base_color
                .expect("base-color path survived re-interning"),
        )
    };
    let world_pool = world.resource::<StringPool>();
    assert_eq!(
        world_pool.resolve(material_path),
        Some("materials\\worker.bgsm")
    );
    assert_eq!(world_pool.resolve(base_color), Some("textures\\worker.dds"));
}

/// Pre-cached positive entry — `finish_partial_import` must early-out
/// without touching `AnimationClipRegistry` or rebuilding the cached
/// import. The arc identity check verifies the cache entry wasn't
/// overwritten.
#[test]
fn finish_partial_import_early_outs_on_already_cached_positive_entry() {
    let mut world = world_with_registries();
    let original = dummy_cached();
    let original_ptr = Arc::as_ptr(&original) as usize;
    {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        let _ = reg.insert("meshes\\test.nif".to_string(), Some(original));
    }
    assert_eq!(world.resource::<NifImportRegistry>().len(), 1);
    assert_eq!(world.resource::<AnimationClipRegistry>().len(), 0);

    finish_partial_import(&mut world, None, "test.nif", dummy_partial());

    // Cache entry preserved (same Arc pointer — the early-out didn't
    // rebuild and overwrite).
    let reg = world.resource::<NifImportRegistry>();
    let entry = reg.get("meshes\\test.nif").expect("cache entry preserved");
    let cached = entry.as_ref().expect("positive cache hit preserved");
    assert_eq!(
        Arc::as_ptr(cached) as usize,
        original_ptr,
        "early-out must NOT overwrite the cached Arc",
    );
    drop(reg);
    // AnimationClipRegistry untouched — convert_nif_clip + clip_reg.add
    // were correctly skipped.
    assert_eq!(
        world.resource::<AnimationClipRegistry>().len(),
        0,
        "early-out must skip clip conversion",
    );
}

/// Pre-cached NEGATIVE entry (failed parse memo) — same early-out
/// applies. Re-attempting the parse path would also be wasted work,
/// AND inserting a positive entry over the negative would let the
/// cache thrash between the two on alternating re-parses.
#[test]
fn finish_partial_import_early_outs_on_already_cached_negative_entry() {
    let mut world = world_with_registries();
    {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        let _ = reg.insert("meshes\\broken.nif".to_string(), None);
    }
    assert_eq!(world.resource::<NifImportRegistry>().len(), 1);

    finish_partial_import(&mut world, None, "broken.nif", dummy_partial());

    // Cache entry stays negative — the worker's payload (which would
    // have produced a positive entry) is dropped silently.
    let reg = world.resource::<NifImportRegistry>();
    let entry = reg
        .get("meshes\\broken.nif")
        .expect("cache entry preserved");
    assert!(entry.is_none(), "negative cache stays negative");
    drop(reg);
    assert_eq!(world.resource::<AnimationClipRegistry>().len(), 0);
}

/// Path-case round-trip: the cache key is lowercased on insert and on
/// lookup, so a model_path with mixed case still hits the early-out.
/// Catches any regression where `to_ascii_lowercase()` migration
/// breaks the key normalisation contract.
#[test]
fn finish_partial_import_early_outs_with_mixed_case_model_path() {
    let mut world = world_with_registries();
    {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        let _ = reg.insert(
            "meshes\\clutter\\rock_cliff.nif".to_string(),
            Some(dummy_cached()),
        );
    }
    finish_partial_import(
        &mut world,
        None,
        "Meshes/Clutter/Rock_Cliff.NIF",
        dummy_partial(),
    );
    let reg = world.resource::<NifImportRegistry>();
    assert_eq!(
        reg.len(),
        1,
        "early-out must not append a duplicate-case entry"
    );
    assert!(reg.get("meshes\\clutter\\rock_cliff.nif").is_some());
}

// ── #3036 / #3102 — BSXFlags bit 5 is presence, not identity ──

/// FO4-era content (BSVER >= FALLOUT4) with bit 5 set must NOT be
/// treated as an editor marker — the cache entry must be a POSITIVE
/// hit (imported), not the `None` skip-marker.
#[test]
fn finish_partial_import_fo4_bsx_bit5_is_not_editor_marker() {
    let mut world = world_with_registries();
    let partial = dummy_partial_with(0xA2);

    finish_partial_import(&mut world, None, "hitfloorsolidfull01.nif", partial);

    let reg = world.resource::<NifImportRegistry>();
    let entry = reg
        .get("meshes\\hitfloorsolidfull01.nif")
        .expect("cache entry inserted");
    assert!(
        entry.is_some(),
        "FO4 BSXFlags bit 5 (MultiBoundNode) must NOT be classified as an \
         editor marker — the NIF must still be imported (#2046)"
    );
}

/// A marker-only subtree naturally imports to zero meshes because the NIF
/// walker culls that child. Bit 5 must not turn the cache entry into a
/// whole-file parse failure.
#[test]
fn finish_partial_import_marker_only_scene_imports_empty() {
    let mut world = world_with_registries();
    let partial = partial_with_marker_scene(false);

    finish_partial_import(&mut world, None, "xmarkerheading.nif", partial);

    let reg = world.resource::<NifImportRegistry>();
    let entry = reg
        .get("meshes\\xmarkerheading.nif")
        .expect("cache entry inserted");
    let cached = entry
        .as_ref()
        .expect("marker-only scene parsed successfully");
    assert!(cached.meshes.is_empty(), "marker child must stay culled");
}

/// The #3036 headline regression: a pre-FO4 bit-5 scene can contain a
/// marker child and real geometry. Cull the former and preserve the latter.
#[test]
fn finish_partial_import_bsx_bit5_keeps_real_geometry_sibling() {
    let mut world = world_with_registries();
    let partial = partial_with_marker_scene(true);

    finish_partial_import(&mut world, None, "stool01.nif", partial);

    let reg = world.resource::<NifImportRegistry>();
    let cached = reg
        .get("meshes\\stool01.nif")
        .expect("cache entry inserted")
        .as_ref()
        .expect("bit 5 must not reject the whole NIF");
    assert_eq!(cached.meshes.len(), 1, "only real geometry survives");
}

// ── #3072 / #3074 — furniture + flame-attach offset survive the
// streaming-partial path ─────────────────────────────────────────────

/// A scene carrying a flame-marker `NiNode` child and a `BSFurnitureMarker`
/// block — everything `finish_partial_import` needs to populate both
/// fields via the streaming-partial path.
fn furniture_and_flame_scene() -> NifScene {
    use byroredux_nif::blocks::extra_data::{
        BsFurnitureMarker, FurniturePosition, FurniturePositionData,
    };
    use byroredux_nif::types::NiMatrix3;

    fn av(name: &str) -> NiAVObjectData {
        NiAVObjectData {
            net: NiObjectNETData {
                name: Some(StdArc::from(name)),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        }
    }

    let mut flame_av = av("AttachFire");
    flame_av.transform = NiTransform {
        translation: NiPoint3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        rotation: NiMatrix3::default(),
        scale: 1.0,
    };
    let flame_node = NiNode {
        av: flame_av,
        children: Vec::new(),
        effects: Vec::new(),
    };

    let marker = BsFurnitureMarker {
        type_name: "BSFurnitureMarker",
        name: None,
        positions: vec![FurniturePosition {
            offset: [1.0, 0.0, 0.0],
            data: FurniturePositionData::Modern {
                heading: 0.0,
                animation_type: 1, // Sit
                entry_properties: 0,
            },
        }],
    };

    let root = NiNode {
        av: av("Scene Root"),
        children: vec![BlockRef(1)],
        effects: Vec::new(),
    };

    NifScene {
        blocks: vec![Box::new(root), Box::new(flame_node), Box::new(marker)],
        root_index: Some(0),
        bsver: 34,
        ..NifScene::default()
    }
}

/// #3072 / #3074 — the streaming-partial path used to hardcode both
/// `furniture` and `flame_attach_offset` to `None`, one citing a false
/// blocker (the helper needs `&ImportedScene`, when it actually takes
/// `&NifScene`) and the other citing that blocker as precedent. Both are
/// available from the `&NifScene` `finish_partial_import` already holds.
#[test]
fn finish_partial_import_populates_furniture_and_flame_offset() {
    let mut world = world_with_registries();
    let partial = crate::streaming::PartialNifImport {
        scene: furniture_and_flame_scene(),
        meshes: Vec::new(),
        collisions: Vec::new(),
        worker_pool: StringPool::new(),
        bsx: 0,
        root_flags: 0,
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
    };

    finish_partial_import(&mut world, None, "furnace01.nif", partial);

    let reg = world.resource::<NifImportRegistry>();
    let cached = reg
        .get("meshes\\furnace01.nif")
        .expect("cache entry inserted")
        .as_ref()
        .expect("scene must import successfully");

    let furniture = cached
        .furniture
        .as_ref()
        .expect("BSFurnitureMarker must survive the streaming-partial path (#3072)");
    assert_eq!(furniture.markers.len(), 1);
    assert_eq!(
        furniture.markers[0].kind,
        byroredux_core::ecs::components::FurnitureMarkerKind::Sit
    );

    let offset = cached
        .flame_attach_offset
        .expect("flame-marker offset must survive the streaming-partial path (#3074)");
    // zup_to_yup_pos([1,2,3]) = [1,3,-2].
    assert_eq!(offset, [1.0, 3.0, -2.0]);
}
