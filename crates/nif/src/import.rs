//! NIF-to-ECS import — converts a parsed NifScene into flat meshes.
//!
//! Walks the NiNode scene graph tree, accumulating world-space transforms,
//! and produces one `ImportedMesh` per NiTriShape leaf. The scene graph
//! hierarchy is discarded — ECS is flat.
//!
//! The output is GPU-agnostic: `ImportedMesh` contains plain `Vec<Vertex>`
//! and `Vec<u32>` data ready for upload via `MeshRegistry::upload()`.

use crate::blocks::node::NiNode;
use crate::blocks::properties::{NiMaterialProperty, NiTexturingProperty};
use crate::blocks::shader::{BSShaderPPLightingProperty, BSShaderTextureSet};
use crate::blocks::texture::NiSourceTexture;
use crate::blocks::tri_shape::{NiTriShape, NiTriShapeData, NiTriStripsData};
use crate::scene::NifScene;
use crate::types::{NiMatrix3, NiPoint3, NiTransform};

/// A mesh extracted from a NIF file, ready for GPU upload.
#[derive(Debug)]
pub struct ImportedMesh {
    /// Vertices in renderer format: position + color + normal + UV.
    pub positions: Vec<[f32; 3]>,
    /// Vertex colors (RGB). Falls back to material diffuse or white.
    pub colors: Vec<[f32; 3]>,
    /// Vertex normals. Falls back to +Y up if the mesh has no normals.
    pub normals: Vec<[f32; 3]>,
    /// UV coordinates. Empty if the mesh has no UVs.
    pub uvs: Vec<[f32; 2]>,
    /// Triangle indices (u32 for Vulkan compatibility).
    pub indices: Vec<u32>,
    /// World-space transform (parent chain composed).
    pub translation: [f32; 3],
    pub rotation: [[f32; 3]; 3],
    pub scale: f32,
    /// Texture file path (if a base texture was found).
    pub texture_path: Option<String>,
    /// Node name from the NIF.
    pub name: Option<String>,
}

/// Import all renderable meshes from a parsed NIF scene.
///
/// Returns one `ImportedMesh` per NiTriShape found in the scene graph.
/// The meshes have world-space transforms computed by composing parent
/// node transforms down the tree.
pub fn import_nif(scene: &NifScene) -> Vec<ImportedMesh> {
    let mut meshes = Vec::new();

    let Some(root_idx) = scene.root_index else {
        return meshes;
    };

    // Start recursive walk from the root with identity transform.
    walk_node(scene, root_idx, &NiTransform::default(), &mut meshes);
    meshes
}

/// Recursively walk the scene graph, accumulating transforms.
fn walk_node(
    scene: &NifScene,
    block_idx: usize,
    parent_transform: &NiTransform,
    out: &mut Vec<ImportedMesh>,
) {
    let Some(block) = scene.get(block_idx) else { return };

    // Try as NiNode (scene graph parent)
    if let Some(node) = block.as_any().downcast_ref::<NiNode>() {
        let world_transform = compose_transforms(parent_transform, &node.transform);

        // Process children
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node(scene, idx, &world_transform, out);
            }
        }
        return;
    }

    // Try as NiTriShape or NiTriStrips (geometry leaf).
    // NiTriStrips is a type alias for NiTriShape, so both downcast to NiTriShape.
    if let Some(shape) = block.as_any().downcast_ref::<NiTriShape>() {
        let world_transform = compose_transforms(parent_transform, &shape.transform);

        if let Some(mesh) = extract_mesh(scene, shape, &world_transform) {
            out.push(mesh);
        }
    }
}

/// Intermediate geometry data extracted from either NiTriShapeData or NiTriStripsData.
#[allow(dead_code)]
struct GeomData<'a> {
    vertices: &'a [NiPoint3],
    normals: &'a [NiPoint3],
    vertex_colors: &'a [[f32; 4]],
    uv_sets: &'a [Vec<[f32; 2]>],
    triangles: Vec<[u16; 3]>,
}

/// Extract an ImportedMesh from an NiTriShape and its referenced data block.
fn extract_mesh(
    scene: &NifScene,
    shape: &NiTriShape,
    world_transform: &NiTransform,
) -> Option<ImportedMesh> {
    let data_idx = shape.data_ref.index()?;

    // Try NiTriShapeData first, then NiTriStripsData
    let geom = if let Some(data) = scene.get_as::<NiTriShapeData>(data_idx) {
        GeomData {
            vertices: &data.vertices,
            normals: &data.normals,
            vertex_colors: &data.vertex_colors,
            uv_sets: &data.uv_sets,
            triangles: data.triangles.clone(),
        }
    } else if let Some(data) = scene.get_as::<NiTriStripsData>(data_idx) {
        GeomData {
            vertices: &data.vertices,
            normals: &data.normals,
            vertex_colors: &data.vertex_colors,
            uv_sets: &data.uv_sets,
            triangles: data.to_triangles(),
        }
    } else {
        return None;
    };

    if geom.vertices.is_empty() || geom.triangles.is_empty() {
        return None;
    }

    // Convert positions: Gamebryo Z-up → renderer Y-up: (x,y,z) → (x,z,-y)
    let positions: Vec<[f32; 3]> = geom.vertices.iter()
        .map(|v| [v.x, v.z, -v.y])
        .collect();

    // Convert indices (u16 → u32). Winding order preserved — the Z-up → Y-up
    // transform is a proper rotation (det=+1), not a reflection.
    let indices: Vec<u32> = geom.triangles.iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    // Convert normals with same axis swap (fall back to +Y up if none)
    let normals: Vec<[f32; 3]> = if !geom.normals.is_empty() {
        geom.normals.iter().map(|n| [n.x, n.z, -n.y]).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    // Get UVs from first UV set (if available)
    let uvs = geom.uv_sets.first()
        .cloned()
        .unwrap_or_default();

    // Determine vertex colors: prefer per-vertex colors, then material diffuse, then white
    let (colors, texture_path) = extract_material(scene, shape, &geom);

    // Apply Z-up → Y-up to the entity transform as well.
    let t = &world_transform.translation;
    let r = &world_transform.rotation.rows;

    // Rotation matrix axis swap: for each row [rx, ry, rz] → [rx, rz, -ry],
    // and swap row order: row_y ↔ row_z with negation.
    let converted_rotation = [
        [r[0][0], r[0][2], -r[0][1]],  // original X row, axes swapped
        [r[2][0], r[2][2], -r[2][1]],  // original Z row becomes Y row
        [-r[1][0], -r[1][2], r[1][1]], // original -Y row becomes Z row
    ];

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        uvs,
        indices,
        translation: [t.x, t.z, -t.y],
        rotation: converted_rotation,
        scale: world_transform.scale,
        name: shape.name.clone(),
        texture_path,
    })
}

/// Extract vertex colors and texture path from the shape's properties.
fn extract_material(
    scene: &NifScene,
    shape: &NiTriShape,
    data: &GeomData,
) -> (Vec<[f32; 3]>, Option<String>) {
    let num_verts = data.vertices.len();

    // Per-vertex colors take priority
    if !data.vertex_colors.is_empty() {
        let colors = data.vertex_colors.iter()
            .map(|c| [c[0], c[1], c[2]]) // drop alpha
            .collect();
        let tex = find_texture_path(scene, shape);
        return (colors, tex);
    }

    // Search properties for NiMaterialProperty
    let mut diffuse = [1.0f32; 3]; // default white
    for prop_ref in &shape.properties {
        if let Some(idx) = prop_ref.index() {
            if let Some(mat) = scene.get_as::<NiMaterialProperty>(idx) {
                diffuse = [mat.diffuse.r, mat.diffuse.g, mat.diffuse.b];
                break;
            }
        }
    }

    let colors = vec![diffuse; num_verts];
    let tex = find_texture_path(scene, shape);
    (colors, tex)
}

/// Walk the shape's properties to find the base texture filename.
///
/// Checks NiTexturingProperty → NiSourceTexture (Gamebryo path) and
/// BSShaderPPLightingProperty → BSShaderTextureSet (Bethesda FO3/FNV path).
fn find_texture_path(scene: &NifScene, shape: &NiTriShape) -> Option<String> {
    for prop_ref in &shape.properties {
        let idx = match prop_ref.index() {
            Some(i) => i,
            None => continue,
        };

        // Gamebryo path: NiTexturingProperty → NiSourceTexture
        if let Some(tex_prop) = scene.get_as::<NiTexturingProperty>(idx) {
            if let Some(ref base) = tex_prop.base_texture {
                if let Some(src_idx) = base.source_ref.index() {
                    if let Some(src_tex) = scene.get_as::<NiSourceTexture>(src_idx) {
                        if src_tex.filename.is_some() {
                            return src_tex.filename.clone();
                        }
                    }
                }
            }
        }

        // Bethesda path: BSShaderPPLightingProperty → BSShaderTextureSet
        if let Some(shader_prop) = scene.get_as::<BSShaderPPLightingProperty>(idx) {
            if let Some(ts_idx) = shader_prop.texture_set_ref.index() {
                if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                    // textures[0] is the diffuse texture
                    if let Some(path) = tex_set.textures.first() {
                        if !path.is_empty() {
                            return Some(path.clone());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Compose parent * child transforms.
///
/// `NiTransform` composition: rotation = parent.rot * child.rot,
/// translation = parent.rot * (parent.scale * child.trans) + parent.trans,
/// scale = parent.scale * child.scale.
fn compose_transforms(parent: &NiTransform, child: &NiTransform) -> NiTransform {
    let rot = mul_matrix3(&parent.rotation, &child.rotation);
    let scaled_child_trans = scale_point(child.translation, parent.scale);
    let rotated = mul_matrix3_point(&parent.rotation, scaled_child_trans);
    let translation = add_points(parent.translation, rotated);
    let scale = parent.scale * child.scale;

    NiTransform { rotation: rot, translation, scale }
}

fn mul_matrix3(a: &NiMatrix3, b: &NiMatrix3) -> NiMatrix3 {
    let mut result = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            result[i][j] = a.rows[i][0] * b.rows[0][j]
                + a.rows[i][1] * b.rows[1][j]
                + a.rows[i][2] * b.rows[2][j];
        }
    }
    NiMatrix3 { rows: result }
}

fn mul_matrix3_point(m: &NiMatrix3, p: NiPoint3) -> NiPoint3 {
    NiPoint3 {
        x: m.rows[0][0] * p.x + m.rows[0][1] * p.y + m.rows[0][2] * p.z,
        y: m.rows[1][0] * p.x + m.rows[1][1] * p.y + m.rows[1][2] * p.z,
        z: m.rows[2][0] * p.x + m.rows[2][1] * p.y + m.rows[2][2] * p.z,
    }
}

fn scale_point(p: NiPoint3, s: f32) -> NiPoint3 {
    NiPoint3 { x: p.x * s, y: p.y * s, z: p.z * s }
}

fn add_points(a: NiPoint3, b: NiPoint3) -> NiPoint3 {
    NiPoint3 { x: a.x + b.x, y: a.y + b.y, z: a.z + b.z }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BlockRef, NiColor};

    /// Helper: build a minimal NifScene with the given blocks.
    fn scene_from_blocks(blocks: Vec<Box<dyn crate::blocks::NiObject>>) -> NifScene {
        use std::sync::Arc;
        let root_index = if blocks.is_empty() { None } else { Some(0) };
        let blocks = blocks.into_iter().map(|b| Arc::from(b)).collect();
        NifScene { blocks, root_index }
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
                NiPoint3 { x: 0.0, y: 0.0, z: 0.0 },
                NiPoint3 { x: 1.0, y: 0.0, z: 0.0 },
                NiPoint3 { x: 0.0, y: 1.0, z: 0.0 },
            ],
            normals: vec![
                NiPoint3 { x: 0.0, y: 0.0, z: 1.0 },
                NiPoint3 { x: 0.0, y: 0.0, z: 1.0 },
                NiPoint3 { x: 0.0, y: 0.0, z: 1.0 },
            ],
            center: NiPoint3 { x: 0.33, y: 0.33, z: 0.0 },
            radius: 1.0,
            vertex_colors: Vec::new(),
            uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
            triangles: vec![[0, 1, 2]],
        }
    }

    fn make_ni_node(transform: NiTransform, children: Vec<BlockRef>) -> NiNode {
        NiNode {
            name: Some("TestNode".to_string()),
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
            flags: 0,
            transform,
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
            children,
            effects: Vec::new(),
        }
    }

    fn make_ni_tri_shape(
        name: &str,
        transform: NiTransform,
        data_ref: u32,
        properties: Vec<BlockRef>,
    ) -> NiTriShape {
        NiTriShape {
            name: Some(name.to_string()),
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
            flags: 0,
            transform,
            properties,
            collision_ref: BlockRef::NULL,
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
        let scene = NifScene {
            blocks: Vec::new(),
            root_index: None,
        };
        let meshes = import_nif(&scene);
        assert!(meshes.is_empty());
    }

    #[test]
    fn import_single_shape_under_root() {
        // Block 0: NiNode (root) with child → block 1
        // Block 1: NiTriShape with data_ref → block 2
        // Block 2: NiTriShapeData
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape("Triangle", identity_transform(), 2, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert_eq!(m.name, Some("Triangle".to_string()));
        assert_eq!(m.positions.len(), 3);
        assert_eq!(m.indices, vec![0, 1, 2]);
        assert_eq!(m.uvs.len(), 3);
        // Identity transform (Z-up → Y-up is identity for zero translation)
        assert_eq!(m.translation, [0.0, 0.0, 0.0]);
        assert_eq!(m.scale, 1.0);
    }

    #[test]
    fn import_inherits_parent_translation() {
        // Root node translated by (10, 0, 0), child shape at identity
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(translated(10.0, 0.0, 0.0), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape("Mesh", identity_transform(), 2, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert!((m.translation[0] - 10.0).abs() < 1e-6);
        assert!((m.translation[1]).abs() < 1e-6);
        assert!((m.translation[2]).abs() < 1e-6);
    }

    #[test]
    fn import_composes_nested_transforms() {
        // Root at (5, 0, 0) → Child NiNode at (0, 3, 0) → Shape at identity
        // NIF-space composed: (5, 3, 0)
        // After Z-up → Y-up (x,z,-y): (5, 0, -3)
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(translated(5.0, 0.0, 0.0), vec![BlockRef(1)])),
            Box::new(make_ni_node(translated(0.0, 3.0, 0.0), vec![BlockRef(2)])),
            Box::new(make_ni_tri_shape("Deep", identity_transform(), 3, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert!((m.translation[0] - 5.0).abs() < 1e-6);
        assert!((m.translation[1] - 0.0).abs() < 1e-6);
        assert!((m.translation[2] - -3.0).abs() < 1e-6);
    }

    #[test]
    fn import_composes_scale() {
        // Root scale 2.0, shape at (1, 0, 0) with scale 3.0
        let root_transform = NiTransform { scale: 2.0, ..NiTransform::default() };
        let shape_transform = NiTransform {
            translation: NiPoint3 { x: 1.0, y: 0.0, z: 0.0 },
            scale: 3.0,
            ..NiTransform::default()
        };
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(root_transform, vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape("Scaled", shape_transform, 2, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        // Scale composes: 2.0 * 3.0 = 6.0
        assert!((m.scale - 6.0).abs() < 1e-6);
        // Translation: parent.rot * (parent.scale * child.trans) + parent.trans
        // = identity * (2.0 * (1,0,0)) + (0,0,0) = (2, 0, 0)
        assert!((m.translation[0] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn import_multiple_shapes() {
        // Root → [shape0 (data→2), shape1 (data→3)]
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1), BlockRef(3)])),
            Box::new(make_ni_tri_shape("A", translated(1.0, 0.0, 0.0), 2, Vec::new())),
            Box::new(make_tri_shape_data()),
            Box::new(make_ni_tri_shape("B", translated(-1.0, 0.0, 0.0), 4, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 2);
        assert_eq!(meshes[0].name, Some("A".to_string()));
        assert_eq!(meshes[1].name, Some("B".to_string()));
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
            Box::new(make_ni_tri_shape("Colored", identity_transform(), 2, Vec::new())),
            Box::new(data),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes[0].colors[0], [1.0, 0.0, 0.0]);
        assert_eq!(meshes[0].colors[1], [0.0, 1.0, 0.0]);
        assert_eq!(meshes[0].colors[2], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn import_falls_back_to_material_diffuse() {
        use crate::blocks::properties::NiMaterialProperty;

        // Block 0: root, Block 1: shape (props→[3]), Block 2: data, Block 3: material
        let mat = NiMaterialProperty {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
            ambient: NiColor { r: 0.2, g: 0.2, b: 0.2 },
            diffuse: NiColor { r: 0.8, g: 0.4, b: 0.2 },
            specular: NiColor::default(),
            emissive: NiColor { r: 0.0, g: 0.0, b: 0.0 },
            shininess: 10.0,
            alpha: 1.0,
            emissive_mult: 1.0,
        };

        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape("Mat", identity_transform(), 2, vec![BlockRef(3)])),
            Box::new(make_tri_shape_data()),
            Box::new(mat),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        // All 3 vertices should have the diffuse color
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
            Box::new(make_ni_tri_shape("NoMat", identity_transform(), 2, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        for color in &meshes[0].colors {
            assert_eq!(*color, [1.0, 1.0, 1.0]);
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
        let meshes = import_nif(&scene);
        assert!(meshes.is_empty());
    }

    #[test]
    fn compose_transforms_identity() {
        let a = NiTransform::default();
        let b = NiTransform::default();
        let c = compose_transforms(&a, &b);
        assert_eq!(c.scale, 1.0);
        assert!((c.translation.x).abs() < 1e-6);
    }

    #[test]
    fn compose_transforms_translation_only() {
        let a = translated(1.0, 2.0, 3.0);
        let b = translated(4.0, 5.0, 6.0);
        let c = compose_transforms(&a, &b);
        assert!((c.translation.x - 5.0).abs() < 1e-6);
        assert!((c.translation.y - 7.0).abs() < 1e-6);
        assert!((c.translation.z - 9.0).abs() < 1e-6);
    }
}
