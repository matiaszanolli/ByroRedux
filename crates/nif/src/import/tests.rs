//! Unit tests for the NIF→ECS import pipeline. Split out of
//! `mod.rs` (#refactor) to keep the production code under ~1000
//! lines; pulled in as a child module via `#[cfg(test)] mod tests;`.

use super::*;
use crate::blocks::tri_shape::NiTriShapeData;
use crate::types::{BlockRef, NiColor, NiMatrix3, NiPoint3, NiTransform};

/// Helper: build a minimal NifScene with the given blocks.
fn scene_from_blocks(blocks: Vec<Box<dyn crate::blocks::NiObject>>) -> NifScene {
    let root_index = if blocks.is_empty() { None } else { Some(0) };
    NifScene {
        blocks,
        root_index,
        ..NifScene::default()
    }
}

fn identity_transform() -> NiTransform {
    NiTransform::default()
}

fn translated(x: f32, y: f32, z: f32) -> NiTransform {
    NiTransform {
        translation: NiPoint3 { x, y, z },
        ..NiTransform::default()
    }
}

fn make_tri_shape_data() -> NiTriShapeData {
    NiTriShapeData {
        vertices: vec![
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            NiPoint3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            NiPoint3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ],
        normals: vec![
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
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

fn make_ni_node(
    transform: NiTransform,
    children: Vec<BlockRef>,
) -> crate::blocks::node::NiNode {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("TestNode")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform,
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children,
        effects: Vec::new(),
    }
}

fn make_ni_tri_shape(
    name: &str,
    transform: NiTransform,
    data_ref: u32,
    properties: Vec<BlockRef>,
) -> crate::blocks::tri_shape::NiTriShape {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    crate::blocks::tri_shape::NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from(name)),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform,
            properties,
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef(data_ref),
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    }
}

#[test]
fn import_empty_scene() {
    let scene = NifScene::default();
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    assert!(meshes.is_empty());
}

#[test]
fn import_single_shape_under_root() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Triangle",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert_eq!(m.name, Some(Arc::from("Triangle")));
    assert_eq!(m.positions.len(), 3);
    assert_eq!(m.indices, vec![0, 1, 2]);
    assert_eq!(m.uvs.len(), 3);
    assert_eq!(m.translation, [0.0, 0.0, 0.0]);
    assert_eq!(m.scale, 1.0);
}

#[test]
fn import_inherits_parent_translation() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(translated(10.0, 0.0, 0.0), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Mesh",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert!((m.translation[0] - 10.0).abs() < 1e-6);
    assert!((m.translation[1]).abs() < 1e-6);
    assert!((m.translation[2]).abs() < 1e-6);
}

#[test]
fn import_composes_nested_transforms() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(translated(5.0, 0.0, 0.0), vec![BlockRef(1)])),
        Box::new(make_ni_node(translated(0.0, 3.0, 0.0), vec![BlockRef(2)])),
        Box::new(make_ni_tri_shape(
            "Deep",
            identity_transform(),
            3,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert!((m.translation[0] - 5.0).abs() < 1e-6);
    assert!((m.translation[1] - 0.0).abs() < 1e-6);
    assert!((m.translation[2] - -3.0).abs() < 1e-6);
}

#[test]
fn import_composes_scale() {
    let root_transform = NiTransform {
        scale: 2.0,
        ..NiTransform::default()
    };
    let shape_transform = NiTransform {
        translation: NiPoint3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 3.0,
        ..NiTransform::default()
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(root_transform, vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape("Scaled", shape_transform, 2, Vec::new())),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert!((m.scale - 6.0).abs() < 1e-6);
    assert!((m.translation[0] - 2.0).abs() < 1e-6);
}

#[test]
fn import_multiple_shapes() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(
            identity_transform(),
            vec![BlockRef(1), BlockRef(3)],
        )),
        Box::new(make_ni_tri_shape(
            "A",
            translated(1.0, 0.0, 0.0),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
        Box::new(make_ni_tri_shape(
            "B",
            translated(-1.0, 0.0, 0.0),
            4,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 2);
    assert_eq!(meshes[0].name, Some(Arc::from("A")));
    assert_eq!(meshes[1].name, Some(Arc::from("B")));
}

#[test]
fn import_uses_vertex_colors_when_available() {
    let mut data = make_tri_shape_data();
    data.vertex_colors = vec![
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
    ];

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Colored",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(data),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes[0].colors[0], [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(meshes[0].colors[1], [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(meshes[0].colors[2], [0.0, 0.0, 1.0, 1.0]);
}

/// Regression test for #618 — the alpha lane on per-vertex colours
/// must survive `extract_vertex_colors`. Pre-fix the importer ran
/// `[c[0], c[1], c[2]]` on the way in, dropping the value silently.
/// Hair-tip cards, eyelash strips, and BSEffectShader meshes use
/// non-1.0 alpha as a per-vertex modulation; without this lane the
/// renderer can't reach the data even when the shader wants it.
#[test]
fn import_preserves_per_vertex_alpha_through_nitrishape_path() {
    let mut data = make_tri_shape_data();
    data.vertex_colors = vec![
        [1.0, 1.0, 1.0, 0.25], // hair tip: low alpha
        [1.0, 1.0, 1.0, 0.50], // mid-strand
        [1.0, 1.0, 1.0, 1.00], // root: opaque
    ];

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "HairCard",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(data),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 1, "expected exactly one mesh");
    let alphas: Vec<f32> = meshes[0].colors.iter().map(|c| c[3]).collect();
    assert_eq!(
        alphas,
        vec![0.25, 0.50, 1.00],
        "alpha lane must survive extract_vertex_colors (#618)"
    );
}

/// Regression test for issue #131: Oblivion meshes store their
/// tangent-space normal maps in `NiTexturingProperty.bump_texture`
/// (the dedicated `normal_texture` slot landed in FO3). The
/// importer must follow the `bump_texture.source_ref` through
/// the scene to the referenced `NiSourceTexture.filename` and
/// populate `ImportedMesh.normal_map`.
#[test]
fn import_extracts_oblivion_bump_texture_as_normal_map() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};
    use crate::blocks::texture::NiSourceTexture;
    use std::sync::Arc;

    // Block layout:
    //  0: root NiNode
    //  1: NiTriShape referencing data at 2 and property at 3
    //  2: NiTriShapeData
    //  3: NiTexturingProperty with bump_texture → block 4
    //  4: NiSourceTexture for the bump map
    //  5: NiSourceTexture for the base texture (referenced too)
    let tex_prop = NiTexturingProperty {
        net: crate::blocks::base::NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        flags: 0,
        texture_count: 6,
        base_texture: Some(TexDesc {
            source_ref: BlockRef(5),
            flags: 0,
            transform: None,
        }),
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: Some(TexDesc {
            source_ref: BlockRef(4),
            flags: 0,
            transform: None,
        }),
        normal_texture: None,
        parallax_texture: None,
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };
    let bump_src = NiSourceTexture {
        net: crate::blocks::base::NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        use_external: true,
        filename: Some(Arc::from("textures\\architecture\\wall01_n.dds")),
        pixel_data_ref: BlockRef::NULL,
        pixel_layout: 0,
        use_mipmaps: 0,
        alpha_format: 0,
        is_static: true,
    };
    let base_src = NiSourceTexture {
        net: crate::blocks::base::NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        use_external: true,
        filename: Some(Arc::from("textures\\architecture\\wall01.dds")),
        pixel_data_ref: BlockRef::NULL,
        pixel_layout: 0,
        use_mipmaps: 0,
        alpha_format: 0,
        is_static: true,
    };

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Wall",
            identity_transform(),
            2,
            vec![BlockRef(3)], // property: texturing
        )),
        Box::new(make_tri_shape_data()),
        Box::new(tex_prop),
        Box::new(bump_src),
        Box::new(base_src),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert_eq!(
        test_support::resolve_path(&pool, m.texture_path),
        Some("textures\\architecture\\wall01.dds"),
        "base_texture should still be extracted"
    );
    assert_eq!(
        test_support::resolve_path(&pool, m.normal_map),
        Some("textures\\architecture\\wall01_n.dds"),
        "bump_texture slot should populate normal_map for Oblivion meshes"
    );
}

/// When both `bump_texture` and `normal_texture` slots are populated
/// (an FO3/FNV mesh exported by a tool that kept the legacy slot
/// filled), the importer should prefer `normal_texture` — it's the
/// dedicated field and more likely to contain the current asset.
#[test]
fn import_prefers_normal_texture_over_bump_texture() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};
    use crate::blocks::texture::NiSourceTexture;
    use std::sync::Arc;

    let make_src = |name: &str| NiSourceTexture {
        net: crate::blocks::base::NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        use_external: true,
        filename: Some(Arc::from(name)),
        pixel_data_ref: BlockRef::NULL,
        pixel_layout: 0,
        use_mipmaps: 0,
        alpha_format: 0,
        is_static: true,
    };

    let tex_prop = NiTexturingProperty {
        net: crate::blocks::base::NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        flags: 0,
        texture_count: 7,
        base_texture: None,
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: Some(TexDesc {
            source_ref: BlockRef(4),
            flags: 0,
            transform: None,
        }),
        normal_texture: Some(TexDesc {
            source_ref: BlockRef(5),
            flags: 0,
            transform: None,
        }),
        parallax_texture: None,
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Wall",
            identity_transform(),
            2,
            vec![BlockRef(3)],
        )),
        Box::new(make_tri_shape_data()),
        Box::new(tex_prop),
        Box::new(make_src("legacy_bump.dds")),
        Box::new(make_src("modern_normal.dds")),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(
        test_support::resolve_path(&pool, meshes[0].normal_map),
        Some("modern_normal.dds"),
        "normal_texture should win when both slots are populated"
    );
}

#[test]
fn import_falls_back_to_material_diffuse() {
    use crate::blocks::properties::NiMaterialProperty;

    let mat = NiMaterialProperty {
        net: crate::blocks::base::NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        ambient: NiColor {
            r: 0.2,
            g: 0.2,
            b: 0.2,
        },
        diffuse: NiColor {
            r: 0.8,
            g: 0.4,
            b: 0.2,
        },
        specular: NiColor::default(),
        emissive: NiColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        },
        shininess: 10.0,
        alpha: 1.0,
        emissive_mult: 1.0,
    };

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Mat",
            identity_transform(),
            2,
            vec![BlockRef(3)],
        )),
        Box::new(make_tri_shape_data()),
        Box::new(mat),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    for color in &meshes[0].colors {
        assert!((color[0] - 0.8).abs() < 1e-6);
        assert!((color[1] - 0.4).abs() < 1e-6);
        assert!((color[2] - 0.2).abs() < 1e-6);
    }
}

#[test]
fn import_defaults_to_white_without_material() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "NoMat",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    for color in &meshes[0].colors {
        assert_eq!(*color, [1.0, 1.0, 1.0, 1.0]);
    }
}

#[test]
fn import_shape_with_no_data_ref_is_skipped() {
    let mut shape = make_ni_tri_shape("NoData", identity_transform(), 0, Vec::new());
    shape.data_ref = BlockRef::NULL;

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(shape),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    assert!(meshes.is_empty());
}

#[test]
fn compose_transforms_identity() {
    let a = NiTransform::default();
    let b = NiTransform::default();
    let c = transform::compose_transforms(&a, &b);
    assert_eq!(c.scale, 1.0);
    assert!((c.translation.x).abs() < 1e-6);
}

#[test]
fn compose_transforms_translation_only() {
    let a = translated(1.0, 2.0, 3.0);
    let b = translated(4.0, 5.0, 6.0);
    let c = transform::compose_transforms(&a, &b);
    assert!((c.translation.x - 5.0).abs() < 1e-6);
    assert!((c.translation.y - 7.0).abs() < 1e-6);
    assert!((c.translation.z - 9.0).abs() < 1e-6);
}

#[test]
fn zup_to_yup_vertex_positions() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Test",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    let m = &meshes[0];

    assert_eq!(m.positions[0], [0.0, 0.0, 0.0]);
    assert_eq!(m.positions[1], [1.0, 0.0, 0.0]);
    assert_eq!(m.positions[2], [0.0, 0.0, -1.0]);
}

#[test]
fn zup_to_yup_vertex_normals() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Test",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    for n in &meshes[0].normals {
        assert_eq!(*n, [0.0, 1.0, 0.0]);
    }
}

#[test]
fn zup_to_yup_translation() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(translated(0.0, 0.0, 5.0), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape("Up", identity_transform(), 2, Vec::new())),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert!((meshes[0].translation[0]).abs() < 1e-6);
    assert!((meshes[0].translation[1] - 5.0).abs() < 1e-6);
    assert!((meshes[0].translation[2]).abs() < 1e-6);
}

#[test]
fn zup_to_yup_translation_forward() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(translated(0.0, 7.0, 0.0), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Fwd",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert!((meshes[0].translation[0]).abs() < 1e-6);
    assert!((meshes[0].translation[1]).abs() < 1e-6);
    assert!((meshes[0].translation[2] - -7.0).abs() < 1e-6);
}

#[test]
fn zup_to_yup_identity_rotation_stays_identity() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape("Id", identity_transform(), 2, Vec::new())),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    let q = &meshes[0].rotation;
    assert!(q[0].abs() < 1e-4, "qx={}", q[0]);
    assert!(q[1].abs() < 1e-4, "qy={}", q[1]);
    assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
    assert!((q[3].abs() - 1.0).abs() < 1e-4, "qw={}", q[3]);
}

#[test]
fn zup_to_yup_winding_order_preserved() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Wind",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes[0].indices, vec![0, 1, 2]);
}

#[test]
fn compose_degenerate_zero_matrix_uses_identity() {
    // Since #277, degenerate rotations are repaired at parse time
    // (read_ni_transform → sanitize_rotation). This test mirrors that
    // pipeline by sanitizing manually before composition.
    let zero_rot = NiMatrix3 {
        rows: [[0.0; 3]; 3],
    };
    let parent = NiTransform {
        rotation: crate::rotation::sanitize_rotation(zero_rot),
        translation: NiPoint3 {
            x: 10.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 1.0,
    };
    let child = translated(5.0, 0.0, 0.0);
    let result = transform::compose_transforms(&parent, &child);

    assert!((result.translation.x - 15.0).abs() < 1e-4);
    assert!((result.translation.y).abs() < 1e-4);
    assert!((result.translation.z).abs() < 1e-4);
}

#[test]
fn compose_degenerate_scaled_rotation_uses_svd() {
    let scaled_identity = NiMatrix3 {
        rows: [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]],
    };
    let parent = NiTransform {
        rotation: crate::rotation::sanitize_rotation(scaled_identity),
        translation: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 1.0,
    };
    let child = translated(3.0, 4.0, 5.0);
    let result = transform::compose_transforms(&parent, &child);

    assert!((result.translation.x - 3.0).abs() < 1e-4);
    assert!((result.translation.y - 4.0).abs() < 1e-4);
    assert!((result.translation.z - 5.0).abs() < 1e-4);
}

#[test]
fn compose_degenerate_scaled_rotation_rotates_child() {
    let scaled_rot_z90 = NiMatrix3 {
        rows: [[0.0, -2.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]],
    };
    let parent = NiTransform {
        rotation: crate::rotation::sanitize_rotation(scaled_rot_z90),
        translation: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 1.0,
    };
    let child = translated(1.0, 0.0, 0.0);
    let result = transform::compose_transforms(&parent, &child);

    assert!(
        (result.translation.x).abs() < 1e-4,
        "x={}",
        result.translation.x
    );
    assert!(
        (result.translation.y - 1.0).abs() < 1e-4,
        "y={}",
        result.translation.y
    );
    assert!(
        (result.translation.z).abs() < 1e-4,
        "z={}",
        result.translation.z
    );
}

#[test]
fn zup_to_yup_90deg_ccw_rotation_around_z() {
    let rot_z90 = NiMatrix3 {
        rows: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let q = coord::zup_matrix_to_yup_quat(&rot_z90);
    let sin45 = std::f32::consts::FRAC_PI_4.sin();
    let cos45 = std::f32::consts::FRAC_PI_4.cos();
    assert!(q[0].abs() < 1e-4, "qx={}", q[0]);
    assert!((q[1].abs() - sin45).abs() < 1e-4, "qy={}", q[1]);
    assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
    assert!((q[3].abs() - cos45).abs() < 1e-4, "qw={}", q[3]);
}

/// Regression: #333 / D4-05. Export-tool drift can produce matrices
/// whose determinant is in the (1.0, 1.07] window that the fast-path
/// gate admits; without normalisation the Shepperd extraction
/// produced a quaternion up to ~3.5% off unity, which downstream
/// consumers (`scene.rs`, `cell_loader.rs`) feed directly into
/// `Quat::from_xyzw` without normalising. The post-fix output is
/// always unit-length regardless of the input matrix's scale drift.
#[test]
fn zup_to_yup_drifted_rotation_returns_unit_quaternion() {
    // Identity-around-Z rotation scaled by 1.03 — 6% determinant
    // drift, still inside the fast path. Pre-fix |q| ≈ 1.03; post-fix
    // |q| == 1.0 to f32 precision.
    let drift = 1.03f32;
    let scaled_identity = NiMatrix3 {
        rows: [[drift, 0.0, 0.0], [0.0, drift, 0.0], [0.0, 0.0, drift]],
    };
    let q = coord::zup_matrix_to_yup_quat(&scaled_identity);
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    assert!(
        (len - 1.0).abs() < 1e-5,
        "fast-path quaternion must be unit-length; got {len} (q={q:?})"
    );
}

#[test]
fn zup_to_yup_90deg_ccw_rotation_around_x() {
    let rot_x90 = NiMatrix3 {
        rows: [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
    };
    let q = coord::zup_matrix_to_yup_quat(&rot_x90);
    let sin45 = std::f32::consts::FRAC_PI_4.sin();
    let cos45 = std::f32::consts::FRAC_PI_4.cos();
    assert!((q[0].abs() - sin45).abs() < 1e-4, "qx={}", q[0]);
    assert!(q[1].abs() < 1e-4, "qy={}", q[1]);
    assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
    assert!((q[3].abs() - cos45).abs() < 1e-4, "qw={}", q[3]);
}

/// Regression test for issue #150 — `BsOrderedNode` (and every other
/// NiNode subclass with a `base: NiNode` field) must unwrap cleanly
/// during scene-graph walks. Previously the walker only downcast to
/// plain `NiNode`, so children of BSOrderedNode (FO3/FNV weapons,
/// effects, architecture) were silently dropped.
#[test]
fn bs_ordered_node_children_are_walked() {
    use crate::blocks::node::BsOrderedNode;

    // Root BsOrderedNode with a single NiTriShape child.
    let inner_node = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let ordered = BsOrderedNode {
        base: inner_node,
        alpha_sort_bound: [0.0, 0.0, 0.0, 10.0],
        is_static_bound: false,
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(ordered),
        Box::new(make_ni_tri_shape(
            "OrderedChild",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);

    // Flat path — would return zero meshes before the fix.
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    assert_eq!(
        meshes.len(),
        1,
        "BsOrderedNode subtree must yield 1 mesh in flat import"
    );
    assert_eq!(meshes[0].name, Some(Arc::from("OrderedChild")));

    // Hierarchical path — must register the parent node AND the mesh.
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    assert_eq!(imported.meshes.len(), 1);
    assert_eq!(imported.meshes[0].parent_node, Some(0));
}

/// Regression test for issue #150 — `BsValueNode` is a NiNode
/// subclass carrying numeric metadata; its children must also be
/// walked.
#[test]
fn bs_value_node_children_are_walked() {
    use crate::blocks::node::BsValueNode;

    let inner_node = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let value_node = BsValueNode {
        base: inner_node,
        value: 42,
        value_flags: 0,
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(value_node),
        Box::new(make_ni_tri_shape(
            "ValueChild",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].name, Some(Arc::from("ValueChild")));
}

/// Regression for #625 / SK-D4-02: the BSValueNode `(value,
/// value_flags)` pair survives the `as_ni_node` unwrap and lands
/// on the matching `ImportedNode.bs_value_node`. Pre-fix the
/// walker dropped these alongside the type identity, hiding LOD-
/// distance overrides + billboard hints from the scene builder.
#[test]
fn bs_value_node_value_and_flags_are_surfaced_on_imported_node() {
    use crate::blocks::node::BsValueNode;

    let inner_node = make_ni_node(identity_transform(), vec![]);
    let value_node = BsValueNode {
        base: inner_node,
        value: 0xCAFEBABE,
        value_flags: 0x07,
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![Box::new(value_node)];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    let payload = imported.nodes[0]
        .bs_value_node
        .expect("BsValueNode must surface bs_value_node payload (#625 / SK-D4-02)");
    assert_eq!(payload.value, 0xCAFEBABE);
    assert_eq!(payload.flags, 0x07);
    // Plain NiNode siblings stay None — the field is only populated
    // for the matching subclass.
    assert!(imported.nodes[0].bs_ordered_node.is_none());
}

/// Regression for #625 / SK-D4-03: BSOrderedNode `alpha_sort_bound`
/// + `is_static_bound` survive the walker unwrap. Renderer-side
/// consumption (a `RenderOrderHint` component on each child + a
/// sort-key tweak in `build_render_data`) is deferred per the
/// no-speculative-Vulkan-fixes policy — this test pins the data-
/// plumbing half so the eventual renderer fix has the source
/// material to read.
#[test]
fn bs_ordered_node_alpha_sort_bound_is_surfaced_on_imported_node() {
    use crate::blocks::node::BsOrderedNode;

    let inner_node = make_ni_node(identity_transform(), vec![]);
    let ordered = BsOrderedNode {
        base: inner_node,
        alpha_sort_bound: [1.0, 2.0, 3.0, 7.5],
        is_static_bound: true,
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![Box::new(ordered)];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    let payload = imported.nodes[0]
        .bs_ordered_node
        .expect("BsOrderedNode must surface bs_ordered_node payload (#625 / SK-D4-03)");
    assert_eq!(payload.alpha_sort_bound, [1.0, 2.0, 3.0, 7.5]);
    assert!(payload.is_static_bound);
    assert!(imported.nodes[0].bs_value_node.is_none());
}

/// Plain `NiNode` (no subclass payload) keeps both fields `None`.
/// Guards against a future regression where the walker
/// inadvertently fabricates default payloads on every node.
#[test]
fn plain_ni_node_has_no_bs_subclass_payloads() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(make_ni_node(identity_transform(), vec![]))];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    assert!(imported.nodes[0].bs_value_node.is_none());
    assert!(imported.nodes[0].bs_ordered_node.is_none());
}

/// Build a synthetic NIF scene where the root NiNode has a single
/// NiParticleSystem child. The hierarchical importer must surface
/// the emitter via `ImportedScene::particle_emitters` and the flat
/// importer must surface it via `import_nif_particle_emitters`.
/// Pre-#401 both paths discarded the block silently.
///
/// Modern emitter types (`NiParticleSystem` / `NiMeshParticleSystem` /
/// `NiParticles` / `BSStripParticleSystem`) dispatch to the typed
/// `NiParticleSystem` struct post-#984 — the synthetic fixture needs
/// to match. Legacy controller types (`NiParticleSystemController` /
/// `NiBSPArrayController` / `NiAutoNormalParticles` /
/// `NiRotatingParticles`) stay on the opaque `NiPSysBlock` fallback.
fn synthetic_particle_block(type_name: &str) -> Box<dyn crate::blocks::NiObject> {
    match type_name {
        "NiParticleSystem"
        | "NiMeshParticleSystem"
        | "NiParticles"
        | "BSStripParticleSystem" => Box::new(crate::blocks::particle::NiParticleSystem {
            original_type: type_name.to_string(),
            modifier_refs: Vec::new(),
        }),
        _ => Box::new(crate::blocks::particle::NiPSysBlock {
            original_type: type_name.to_string(),
        }),
    }
}

#[test]
fn hierarchical_import_surfaces_particle_emitter_under_named_host() {
    // Root NiNode named "TorchNode" with a NiParticleSystem child at index 1.
    let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(root), synthetic_particle_block("NiParticleSystem")];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.particle_emitters.len(), 1);
    let em = &imported.particle_emitters[0];
    assert_eq!(em.original_type, "NiParticleSystem");
    // Host is the root NiNode (index 0 in imported.nodes).
    assert_eq!(em.parent_node, Some(0));
}

#[test]
fn flat_import_surfaces_particle_emitter_with_nearest_named_host() {
    // Root NiNode at translation (5, 10, 20), with NiParticleSystem child.
    let root = make_ni_node(translated(5.0, 10.0, 20.0), vec![BlockRef(1)]);
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(root), synthetic_particle_block("NiParticleSystem")];
    let scene = scene_from_blocks(blocks);
    let emitters = import_nif_particle_emitters(&scene);
    assert_eq!(emitters.len(), 1);
    let em = &emitters[0];
    // Y-up conversion: (5, 10, 20) → (5, 20, -10).
    assert!((em.local_position[0] - 5.0).abs() < 1e-5);
    assert!((em.local_position[1] - 20.0).abs() < 1e-5);
    assert!((em.local_position[2] + 10.0).abs() < 1e-5);
    // Host name is the root node's name ("TestNode" per make_ni_node).
    assert_eq!(em.host_name.as_deref(), Some("TestNode"));
    assert_eq!(em.original_type, "NiParticleSystem");
}

#[test]
fn flat_import_recognizes_legacy_particle_block_types() {
    // Each variant's original_type comes from the NIF dispatcher;
    // the importer must recognize all of them, not just "NiParticleSystem".
    for variant in [
        "NiMeshParticleSystem",
        "NiParticles",
        "NiParticleSystemController",
        "NiBSPArrayController",
        "NiAutoNormalParticles",
        "NiRotatingParticles",
    ] {
        let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
            vec![Box::new(root), synthetic_particle_block(variant)];
        let scene = scene_from_blocks(blocks);
        let emitters = import_nif_particle_emitters(&scene);
        assert_eq!(
            emitters.len(),
            1,
            "{} should surface as a particle emitter",
            variant
        );
        assert_eq!(emitters[0].original_type, variant);
    }
}

/// Regression for #707 / FX-2. When the scene has a real
/// `NiPSysColorModifier` chained to a `NiColorData` keyframe
/// stream, both the hierarchical and flat importers must
/// surface the captured `(start, end)` colour curve on every
/// emitter so the cell loader / scene builder can override the
/// name-heuristic preset's start_color / end_color.
///
/// Pre-fix the parser captured the modifier's `color_data_ref`
/// then immediately discarded it, every emitter rendered with
/// the heuristic preset's colour, and Dragonsreach embers
/// rendered as generic dark torch-flame columns.
#[test]
fn import_captures_color_curve_from_psys_color_modifier_chain() {
    use crate::blocks::interpolator::{Color4Key, KeyGroup, KeyType, NiColorData};
    use crate::blocks::particle::{NiPSysColorModifier, NiPSysModifierBase};

    // Scene layout:
    //   [0] NiNode root with the emitter as child
    //   [1] NiParticleSystem (the renderable emitter)
    //   [2] NiPSysColorModifier referencing block [3]
    //   [3] NiColorData with start = orange, end = red
    let root = make_ni_node(identity_transform(), vec![BlockRef(1), BlockRef(2)]);
    let modifier = NiPSysColorModifier {
        base: NiPSysModifierBase {
            name: Some(Arc::from("ColorMod")),
            order: 0,
            target_ref: BlockRef::NULL,
            active: true,
        },
        color_data_ref: BlockRef(3),
    };
    let color_data = NiColorData {
        keys: KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                Color4Key {
                    time: 0.0,
                    value: [1.0, 0.55, 0.10, 1.0], // warm orange — start
                    tangent_forward: [0.0; 4],
                    tangent_backward: [0.0; 4],
                    tbc: None,
                },
                Color4Key {
                    time: 0.5,
                    value: [0.85, 0.20, 0.05, 0.7], // mid (intentionally distinct
                    // from start/end so an
                    // off-by-one would surface)
                    tangent_forward: [0.0; 4],
                    tangent_backward: [0.0; 4],
                    tbc: None,
                },
                Color4Key {
                    time: 1.0,
                    value: [0.30, 0.05, 0.02, 0.0], // dim red fade — end
                    tangent_forward: [0.0; 4],
                    tangent_backward: [0.0; 4],
                    tbc: None,
                },
            ],
        },
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(root),
        synthetic_particle_block("NiParticleSystem"),
        Box::new(modifier),
        Box::new(color_data),
    ];
    let scene = scene_from_blocks(blocks);

    // Hierarchical import.
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.particle_emitters.len(), 1);
    let curve = imported.particle_emitters[0]
        .color_curve
        .expect("hierarchical import must capture the curve");
    assert_eq!(curve.start, [1.0, 0.55, 0.10, 1.0]);
    assert_eq!(curve.end, [0.30, 0.05, 0.02, 0.0]);

    // Flat import — same scene, same expectation.
    let flat = import_nif_particle_emitters(&scene);
    assert_eq!(flat.len(), 1);
    let flat_curve = flat[0]
        .color_curve
        .expect("flat import must capture the curve");
    assert_eq!(flat_curve.start, [1.0, 0.55, 0.10, 1.0]);
    assert_eq!(flat_curve.end, [0.30, 0.05, 0.02, 0.0]);
}

/// Companion: when no `NiPSysColorModifier` is present, the
/// importer leaves `color_curve = None` and the renderer falls
/// back to the name-heuristic preset.
#[test]
fn import_leaves_color_curve_none_when_no_color_modifier() {
    let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(root), synthetic_particle_block("NiParticleSystem")];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.particle_emitters.len(), 1);
    assert!(
        imported.particle_emitters[0].color_curve.is_none(),
        "no NiPSysColorModifier in scene → color_curve must stay None"
    );
    let flat = import_nif_particle_emitters(&scene);
    assert!(flat[0].color_curve.is_none());
}

#[test]
fn flat_import_skips_modifier_only_blocks() {
    // NiPSysGravity / NiPSysColorModifier / etc. are NiPSysBlock too,
    // but they're not renderable emitters — only modifier inputs to a
    // host NiParticleSystem. Surfacing them as emitters would spawn
    // duplicates; the importer must filter them out by original_type.
    let root = make_ni_node(identity_transform(), vec![BlockRef(1), BlockRef(2)]);
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(root),
        synthetic_particle_block("NiPSysGravity"),
        synthetic_particle_block("NiPSysColorModifier"),
    ];
    let scene = scene_from_blocks(blocks);
    let emitters = import_nif_particle_emitters(&scene);
    assert!(
        emitters.is_empty(),
        "modifier-only NiPSysBlocks must not surface as emitters, got {} entries",
        emitters.len(),
    );
}

/// Helper for the #364 test: build a `BsRangeNode` block with the
/// given discriminator + the canonical (min, max, current) triple.
fn ni_range_node(
    kind: crate::blocks::node::BsRangeKind,
    min: u8,
    max: u8,
    current: u8,
) -> crate::blocks::node::BsRangeNode {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    let inner_node = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("RangeHost")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: identity_transform(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    crate::blocks::node::BsRangeNode {
        base: inner_node,
        min,
        max,
        current,
        kind,
    }
}

/// Regression: #364 — BSRangeNode subclasses (BSBlastNode /
/// BSDamageStage / BSDebrisNode) must surface their wire-type
/// discriminator on the resulting `ImportedNode.range_kind`.
/// Pre-fix all four collapsed into a `BsRangeNode` with no
/// surviving discriminator and the walker stripped them down to
/// plain NiNode — gameplay-side systems couldn't tell apart
/// "switch the visible damage stage" from "spawn debris on
/// detach" from "fire the blast effect".
#[test]
fn import_surfaces_bs_range_kind_for_each_subclass() {
    for kind in [
        crate::blocks::node::BsRangeKind::Range,
        crate::blocks::node::BsRangeKind::DamageStage,
        crate::blocks::node::BsRangeKind::Blast,
        crate::blocks::node::BsRangeKind::Debris,
    ] {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
            vec![Box::new(ni_range_node(kind, 0, 5, 2))];
        let scene = scene_from_blocks(blocks);
        let mut pool = StringPool::new();
        let imported = import_nif_scene(&scene, &mut pool);
        assert_eq!(imported.nodes.len(), 1, "{:?}", kind);
        assert_eq!(
            imported.nodes[0].range_kind,
            Some(kind),
            "range_kind should round-trip the dispatcher discriminator for {:?}",
            kind,
        );
    }
}

/// Regression: #364 — plain NiNode produces `range_kind: None`.
/// Catches a regression that defaults the discriminator to
/// `Some(BsRangeKind::Range)` for every node.
#[test]
fn import_plain_ninode_has_no_range_kind() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(make_ni_node(identity_transform(), Vec::new()))];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    assert!(imported.nodes[0].range_kind.is_none());
}

/// Regression: #363 — `BSTreeNode` bone-list metadata must surface
/// on `ImportedNode.tree_bones` resolved to the targets'
/// `NiObjectNET.name` (mirrors the `#335` affected-node-names
/// pattern). Pre-fix the walker stripped the BSTreeNode down to
/// plain NiNode and dropped both bone lists, blocking any future
/// SpeedTree wind / bend simulation from finding what to animate.
#[test]
fn import_surfaces_bs_tree_node_bones_by_name() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    // Build three bone targets (NiNodes with names) at indices 1, 2, 3.
    // Then a BSTreeNode at index 0 whose:
    //   bones_1 = [1, 3]  (branch roots)
    //   bones_2 = [2]     (trunk)
    let bone = |name: &str| -> Box<dyn crate::blocks::NiObject> {
        Box::new(crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from(name)),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: identity_transform(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        })
    };
    let host = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("TreeRoot")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: identity_transform(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let tree = crate::blocks::node::BsTreeNode {
        base: host,
        bones_1: vec![BlockRef(1), BlockRef(3)],
        bones_2: vec![BlockRef(2)],
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(tree),
        bone("Branch_A"),
        bone("Trunk_0"),
        bone("Branch_B"),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    let host_node = &imported.nodes[0];
    let bones = host_node
        .tree_bones
        .as_ref()
        .expect("BSTreeNode should surface tree_bones");
    let branch: Vec<&str> = bones.branch_roots.iter().map(|s| s.as_ref()).collect();
    let trunk: Vec<&str> = bones.trunk.iter().map(|s| s.as_ref()).collect();
    assert_eq!(branch, vec!["Branch_A", "Branch_B"]);
    assert_eq!(trunk, vec!["Trunk_0"]);
}

/// Regression: #363 — when every bone ref in a BSTreeNode is null
/// or unresolvable, surface `tree_bones: None` rather than a
/// `Some(TreeBones { empty, empty })` so the consumer doesn't have
/// to filter empty payloads downstream.
#[test]
fn import_drops_bs_tree_node_with_only_unresolvable_bones() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    let host = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("EmptyTree")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: identity_transform(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let tree = crate::blocks::node::BsTreeNode {
        base: host,
        bones_1: vec![BlockRef::NULL, BlockRef(99)], // null + out-of-range
        bones_2: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(tree)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert!(imported.nodes[0].tree_bones.is_none());
}

/// SK-D4-04 / #564 — distant-LOD `BSMultiBoundNode` hosts whose
/// extra_data carries a `BSPackedCombinedGeomDataExtra` are
/// skipped wholesale. The packed-extra block is renderer-side
/// deferred (M35 terrain-streaming) and the host subtree carries
/// no other geometry, so walking it would only produce empty
/// `ImportedNode` entries.
#[test]
fn bs_multi_bound_node_with_packed_geom_extra_subtree_is_skipped() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::{BsPackedCombinedGeomDataExtra, BsPackedCombinedPayload};
    use crate::blocks::node::BsMultiBoundNode;

    // [0] BSMultiBoundNode root with extra_data → block 1.
    // [1] BSPackedCombinedGeomDataExtra (the LOD batch).
    let packed = BsPackedCombinedGeomDataExtra {
        type_name: "BSPackedCombinedGeomDataExtra",
        name: None,
        vertex_desc: 0,
        num_vertices: 0,
        num_triangles: 0,
        unknown_flags_1: 0,
        unknown_flags_2: 0,
        num_data: 0,
        payload: BsPackedCombinedPayload::Baked(Vec::new()),
    };
    let host = BsMultiBoundNode {
        base: crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(std::sync::Arc::from("LODHost")),
                    extra_data_refs: vec![BlockRef(1)],
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        },
        multi_bound_ref: BlockRef::NULL,
        culling_mode: 0,
    };
    let scene = scene_from_blocks(vec![Box::new(host), Box::new(packed)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    assert!(
        imported.nodes.is_empty(),
        "LOD-batch host must be skipped — no ImportedNode entries should leak"
    );
    assert!(imported.meshes.is_empty());
}

/// Sanity: a plain `BSMultiBoundNode` with no packed-extra
/// extra_data still produces an `ImportedNode` so non-LOD scenes
/// (Dragonsreach interior, College of Winterhold) keep working.
/// Pre-#564 the skip applied unconditionally, which would have
/// broken these.
#[test]
fn plain_bs_multi_bound_node_without_packed_geom_extra_still_imports() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::node::BsMultiBoundNode;

    let host = BsMultiBoundNode {
        base: crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(std::sync::Arc::from("DragonsreachInterior")),
                    // No extra_data_refs — the packed-extra detector
                    // returns false and the walker falls through to
                    // the normal NiNode path.
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        },
        multi_bound_ref: BlockRef::NULL,
        culling_mode: 0,
    };
    let scene = scene_from_blocks(vec![Box::new(host)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    assert_eq!(
        imported.nodes.len(),
        1,
        "Plain BSMultiBoundNode (no packed-extra) must still produce a node"
    );
}

// ── #985 / NIF-D5-ORPHAN-A3 — FO4 weapon-mod attach graph consumer ──

/// `BSConnectPoint::Parents` extra-data on the root node lifts every
/// authored attach point into `ImportedScene::attach_points`. Without
/// this routing, every FO4 modular weapon imports with no discoverable
/// attach surface — the OMOD / weapon-mod system can't function.
#[test]
fn bs_connect_point_parents_lifts_to_imported_scene() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::{BsConnectPointParents, ConnectPointData};

    // FO4 10mm-pistol-style attach graph: receiver bone exposes
    // a magazine slot and a scope rail.
    let parents = BsConnectPointParents {
        name: None,
        connect_points: vec![
            ConnectPointData {
                parent: "GunBoneReceiver".to_string(),
                name: "CON_Magazine".to_string(),
                rotation: [1.0, 0.0, 0.0, 0.0],
                translation: [0.0, -1.5, 0.0],
                scale: 1.0,
            },
            ConnectPointData {
                parent: "GunBoneReceiver".to_string(),
                name: "CON_Scope".to_string(),
                rotation: [1.0, 0.0, 0.0, 0.0],
                translation: [0.0, 0.0, 2.0],
                scale: 1.0,
            },
        ],
    };
    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("10mmPistolRoot")),
                extra_data_refs: vec![BlockRef(1)],
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(parents)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    let points = imported
        .attach_points
        .as_ref()
        .expect("BSConnectPoint::Parents must reach ImportedScene.attach_points");
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].name, "CON_Magazine");
    assert_eq!(points[0].parent, "GunBoneReceiver");
    assert_eq!(points[0].translation, [0.0, -1.5, 0.0]);
    assert_eq!(points[0].scale, 1.0);
    assert_eq!(points[1].name, "CON_Scope");
    assert_eq!(points[1].translation, [0.0, 0.0, 2.0]);
    // Child connections were not authored on this NIF; field stays None.
    assert!(imported.child_attach_connections.is_none());
}

/// `BSConnectPoint::Children` extra-data on the root node lifts the
/// child-side of the attach graph (the names this accessory connects
/// back to on its parent host) into
/// `ImportedScene::child_attach_connections`.
#[test]
fn bs_connect_point_children_lifts_to_imported_scene() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::BsConnectPointChildren;

    // A reflex-sight accessory mesh mounting to a parent's CON_Scope.
    let children = BsConnectPointChildren {
        name: None,
        skinned: false,
        point_names: vec!["CON_Scope".to_string()],
    };
    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("ReflexSightRoot")),
                extra_data_refs: vec![BlockRef(1)],
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(children)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    let conn = imported
        .child_attach_connections
        .as_ref()
        .expect("BSConnectPoint::Children must reach ImportedScene.child_attach_connections");
    assert_eq!(conn.point_names, vec!["CON_Scope".to_string()]);
    assert!(!conn.skinned);
    // Parents not authored on this accessory; field stays None.
    assert!(imported.attach_points.is_none());
}

/// `skinned: true` from `BSConnectPoint::Children` round-trips into
/// `ImportedChildAttachConnections.skinned` — drives the equip-side
/// "rigid attach vs bone-weighted attach" decision.
#[test]
fn bs_connect_point_children_skinned_flag_round_trips() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::BsConnectPointChildren;

    let children = BsConnectPointChildren {
        name: None,
        skinned: true,
        point_names: vec!["CON_Cape".to_string()],
    };
    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("CapeAccessoryRoot")),
                extra_data_refs: vec![BlockRef(1)],
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(children)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    let conn = imported.child_attach_connections.as_ref().unwrap();
    assert!(conn.skinned);
}

/// Sibling check: a NIF with neither `BSConnectPoint::Parents` nor
/// `BSConnectPoint::Children` in its root extra-data leaves both
/// fields at `None`. Defends against an unconditional default
/// `Some(empty)` initialization that would mislead consumers into
/// "this entity has an explicitly-empty attach graph" (vs the truth:
/// "no graph authored").
#[test]
fn scene_without_connect_point_extras_leaves_fields_none() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};

    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("PlainStatic")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    assert!(imported.attach_points.is_none());
    assert!(imported.child_attach_connections.is_none());
}

/// #986 / NIF-D5-ORPHAN-B2 — `BSBound` extra-data on the root node lifts
/// onto `ImportedScene::bs_bound` with the center/dimensions rotated
/// from NIF Z-up to renderer Y-up so the downstream
/// `BSBound` ECS component agrees with `Transform` / `GlobalTransform`.
/// Pre-fix the captured value was raw Z-up, leaving any future culling
/// or spatial-query consumer 90° out of plane with the scene graph.
#[test]
fn bs_bound_lifts_to_imported_scene_in_y_up() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::BsBound;

    // Asymmetric center + half-extents so the y/z permutation is
    // observable separately from the y-sign flip.
    let bound = BsBound {
        name: None,
        center: [1.0, 2.0, 3.0],     // Z-up
        dimensions: [4.0, 5.0, 6.0], // half-extents (Z-up labels)
    };
    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("BoundedRoot")),
                extra_data_refs: vec![BlockRef(1)],
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(bound)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    let (center, half_extents) = imported
        .bs_bound
        .expect("BsBound on the root node must reach ImportedScene.bs_bound");
    // Z-up [1, 2, 3] → Y-up [x, z, -y] = [1, 3, -2]. Same rule as
    // every other point in the importer (zup_point_to_yup).
    assert_eq!(center, [1.0, 3.0, -2.0]);
    // Half-extents are unsigned magnitudes — the Z-up→Y-up rotation
    // around X is a 90° relabel, so the new-Y half-extent equals the
    // old Z half-extent and vice versa. No sign flip.
    assert_eq!(half_extents, [4.0, 6.0, 5.0]);
}

/// #988 / SK-D5-NEW-09 — BSLODTriShape geometry was silently dropped by both
/// import walkers because no NiLodTriShape downcast arm existed. The #838 parser
/// fix added the type but the import path was never wired up.
///
/// Regression: a scene containing a BSLODTriShape (NiLodTriShape) under a root
/// NiNode must import exactly one mesh (from lod.base, the inner NiTriShape),
/// not zero.
#[test]
fn bs_lod_tri_shape_imports_geometry_not_dropped() {
    use crate::blocks::tri_shape::NiLodTriShape;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};

    // Root NiNode → BSLODTriShape (NiLodTriShape) → NiTriShapeData
    let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let lod = NiLodTriShape {
        base: make_ni_tri_shape("LODTree", identity_transform(), 2, Vec::new()),
        lod0_size: 100,
        lod1_size: 50,
        lod2_size: 25,
    };
    let data = make_tri_shape_data();
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(lod), Box::new(data)]);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    // Pre-#988: meshes.len() == 0 (silently dropped).
    assert_eq!(
        meshes.len(),
        1,
        "BSLODTriShape must produce 1 ImportedMesh, not be silently dropped"
    );
    let m = &meshes[0];
    assert_eq!(m.name, Some(std::sync::Arc::from("LODTree")));
    assert_eq!(m.positions.len(), 3, "triangle mesh has 3 positions");
}
