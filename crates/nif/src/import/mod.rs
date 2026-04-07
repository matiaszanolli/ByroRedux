//! NIF-to-ECS import — converts a parsed NifScene into meshes and nodes.
//!
//! Walks the NiNode scene graph tree, preserving hierarchy as `ImportedNode`
//! entries with parent indices. Produces `ImportedMesh` per geometry leaf and
//! `ImportedNode` per NiNode. Transforms are local (relative to parent).
//!
//! The output is GPU-agnostic: `ImportedMesh` contains plain `Vec<Vertex>`
//! and `Vec<u32>` data ready for upload via `MeshRegistry::upload()`.

pub mod collision;
mod coord;
mod material;
mod mesh;
mod transform;
mod walk;

use crate::scene::NifScene;
use crate::types::NiTransform;
use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};

/// Collision data extracted from a NiNode, positioned in world space.
///
/// Used by the flat import path to return collision alongside geometry,
/// since the flat path doesn't produce ImportedNode hierarchy.
#[derive(Debug)]
pub struct ImportedCollision {
    /// World-space translation (Y-up).
    pub translation: [f32; 3],
    /// World-space rotation as quaternion [x, y, z, w] (Y-up).
    pub rotation: [f32; 4],
    pub scale: f32,
    pub shape: CollisionShape,
    pub body: RigidBodyData,
}

/// A scene graph node (NiNode) extracted from a NIF file.
#[derive(Debug)]
pub struct ImportedNode {
    /// Node name from the NIF (e.g., "Bip01 Head", "Scene Root").
    pub name: Option<String>,
    /// Local-space translation (Y-up), relative to parent.
    pub translation: [f32; 3],
    /// Local-space rotation as quaternion [x, y, z, w] (Y-up).
    pub rotation: [f32; 4],
    pub scale: f32,
    /// Index into `ImportedScene.nodes` for this node's parent, or None for root.
    pub parent_node: Option<usize>,
    /// Collision shape and rigid body data (from bhkCollisionObject chain).
    pub collision: Option<(CollisionShape, RigidBodyData)>,
}

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
    /// Local-space translation (Y-up), relative to parent node.
    pub translation: [f32; 3],
    /// Local-space rotation as quaternion [x, y, z, w] (Y-up).
    pub rotation: [f32; 4],
    pub scale: f32,
    /// Texture file path (if a base texture was found).
    pub texture_path: Option<String>,
    /// Node name from the NIF.
    pub name: Option<String>,
    /// Whether this mesh uses alpha blending (from NiAlphaProperty).
    pub has_alpha: bool,
    /// Whether this mesh should be rendered two-sided (no backface culling).
    pub two_sided: bool,
    /// Whether this mesh is a decal (should render on top of coplanar surfaces).
    pub is_decal: bool,
    /// Normal map texture path (if found in shader texture set).
    pub normal_map: Option<String>,
    /// Emissive color (RGB, linear).
    pub emissive_color: [f32; 3],
    /// Emissive intensity multiplier.
    pub emissive_mult: f32,
    /// Specular highlight color (RGB, linear).
    pub specular_color: [f32; 3],
    /// Specular intensity multiplier.
    pub specular_strength: f32,
    /// Glossiness / smoothness.
    pub glossiness: f32,
    /// UV texture coordinate offset [u, v].
    pub uv_offset: [f32; 2],
    /// UV texture coordinate scale [u, v].
    pub uv_scale: [f32; 2],
    /// Material alpha/transparency.
    pub mat_alpha: f32,
    /// Environment map reflection scale (from shader type 1).
    pub env_map_scale: f32,
    /// Index into `ImportedScene.nodes` for this mesh's parent node, or None.
    pub parent_node: Option<usize>,
}

/// A fully imported NIF scene with hierarchy preserved.
#[derive(Debug)]
pub struct ImportedScene {
    /// Scene graph nodes (NiNode hierarchy).
    pub nodes: Vec<ImportedNode>,
    /// Leaf geometry meshes.
    pub meshes: Vec<ImportedMesh>,
}

/// Import all renderable meshes from a parsed NIF scene, preserving hierarchy.
///
/// Returns an `ImportedScene` with nodes (NiNode hierarchy) and meshes (geometry leaves).
/// Transforms are local-space (relative to parent). Use the parent indices
/// to rebuild the hierarchy in the ECS.
pub fn import_nif_scene(scene: &NifScene) -> ImportedScene {
    let mut imported = ImportedScene {
        nodes: Vec::new(),
        meshes: Vec::new(),
    };

    let Some(root_idx) = scene.root_index else {
        return imported;
    };

    walk::walk_node_hierarchical(scene, root_idx, None, &mut imported);
    imported
}

/// Backward-compatible flat import (used by cell loader where hierarchy is unnecessary).
///
/// Returns one `ImportedMesh` per NiTriShape with world-space transforms
/// (parent chain composed). Meshes have `parent_node: None`.
pub fn import_nif(scene: &NifScene) -> Vec<ImportedMesh> {
    let mut meshes = Vec::new();

    let Some(root_idx) = scene.root_index else {
        return meshes;
    };

    walk::walk_node_flat(scene, root_idx, &NiTransform::default(), &mut meshes, None);
    meshes
}

/// Flat import with collision data.
///
/// Like `import_nif()`, returns world-space meshes (flat, no hierarchy).
/// Additionally extracts collision shapes from NiNodes, returning them
/// in world space alongside the geometry.
pub fn import_nif_with_collision(scene: &NifScene) -> (Vec<ImportedMesh>, Vec<ImportedCollision>) {
    let mut meshes = Vec::new();
    let mut collisions = Vec::new();

    let Some(root_idx) = scene.root_index else {
        return (meshes, collisions);
    };

    walk::walk_node_flat(
        scene,
        root_idx,
        &NiTransform::default(),
        &mut meshes,
        Some(&mut collisions),
    );
    (meshes, collisions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::tri_shape::NiTriShapeData;
    use crate::types::{BlockRef, NiColor, NiMatrix3, NiPoint3, NiTransform};

    /// Helper: build a minimal NifScene with the given blocks.
    fn scene_from_blocks(blocks: Vec<Box<dyn crate::blocks::NiObject>>) -> NifScene {
        let root_index = if blocks.is_empty() { None } else { Some(0) };
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
        let scene = NifScene {
            blocks: Vec::new(),
            root_index: None,
        };
        let meshes = import_nif(&scene);
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
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert_eq!(m.name, Some("Triangle".to_string()));
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
        let meshes = import_nif(&scene);

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
        let meshes = import_nif(&scene);

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
        let meshes = import_nif(&scene);

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
            Box::new(make_ni_tri_shape(
                "Colored",
                identity_transform(),
                2,
                Vec::new(),
            )),
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
        let meshes = import_nif(&scene);

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
        let meshes = import_nif(&scene);
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
        let meshes = import_nif(&scene);

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
        let meshes = import_nif(&scene);

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
        let meshes = import_nif(&scene);

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
        let meshes = import_nif(&scene);

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
        let meshes = import_nif(&scene);

        assert_eq!(meshes[0].indices, vec![0, 1, 2]);
    }

    #[test]
    fn compose_degenerate_zero_matrix_uses_identity() {
        let zero_rot = NiMatrix3 {
            rows: [[0.0; 3]; 3],
        };
        let parent = NiTransform {
            rotation: zero_rot,
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
            rotation: scaled_identity,
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
            rotation: scaled_rot_z90,
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
}
