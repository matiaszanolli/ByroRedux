//! Material and texture resolution tests: vertex colors/alpha, the
//! legacy `bump_texture` → normal-map fallback, `normal_texture`
//! precedence, and `NiMaterialProperty` diffuse fallback.

use super::super::*;
use crate::types::{BlockRef, NiColor};

use super::{
    identity_transform, make_ni_node, make_ni_tri_shape, make_tri_shape_data, scene_from_blocks,
};

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
            clamp_mode: 0,
            transform: None,
        }),
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: Some(TexDesc {
            source_ref: BlockRef(4),
            flags: 0,
            clamp_mode: 0,
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
        test_support::resolve_path(&pool, m.material.textures.base_color),
        Some("textures\\architecture\\wall01.dds"),
        "base_texture should still be extracted"
    );
    assert_eq!(
        test_support::resolve_path(&pool, m.material.textures.normal),
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
            clamp_mode: 0,
            transform: None,
        }),
        normal_texture: Some(TexDesc {
            source_ref: BlockRef(5),
            flags: 0,
            clamp_mode: 0,
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
        test_support::resolve_path(&pool, meshes[0].material.textures.normal),
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
