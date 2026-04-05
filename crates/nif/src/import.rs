//! NIF-to-ECS import — converts a parsed NifScene into meshes and nodes.
//!
//! Walks the NiNode scene graph tree, preserving hierarchy as `ImportedNode`
//! entries with parent indices. Produces `ImportedMesh` per geometry leaf and
//! `ImportedNode` per NiNode. Transforms are local (relative to parent).
//!
//! The output is GPU-agnostic: `ImportedMesh` contains plain `Vec<Vertex>`
//! and `Vec<u32>` data ready for upload via `MeshRegistry::upload()`.

use crate::blocks::node::NiNode;
use crate::blocks::properties::{
    NiAlphaProperty, NiMaterialProperty, NiStencilProperty, NiTexturingProperty,
};
use crate::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderTextureSet, ShaderTypeData,
};
use crate::blocks::texture::NiSourceTexture;
use crate::blocks::tri_shape::{BsTriShape, NiTriShape, NiTriShapeData, NiTriStripsData};
use crate::scene::NifScene;
use crate::types::{NiMatrix3, NiPoint3, NiTransform};

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

    walk_node_hierarchical(scene, root_idx, None, &mut imported);
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

    walk_node_flat(scene, root_idx, &NiTransform::default(), &mut meshes);
    meshes
}

/// Recursively walk the scene graph, preserving hierarchy.
/// NiNodes become ImportedNode entries; geometry becomes ImportedMesh with parent_node set.
fn walk_node_hierarchical(
    scene: &NifScene,
    block_idx: usize,
    parent_node_idx: Option<usize>,
    out: &mut ImportedScene,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    if let Some(node) = block.as_any().downcast_ref::<NiNode>() {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }

        // Convert this node's LOCAL transform to Y-up.
        let t = &node.av.transform.translation;
        let quat = zup_matrix_to_yup_quat(&node.av.transform.rotation);

        let this_node_idx = out.nodes.len();
        out.nodes.push(ImportedNode {
            name: node.av.net.name.clone(),
            translation: [t.x, t.z, -t.y],
            rotation: quat,
            scale: node.av.transform.scale,
            parent_node: parent_node_idx,
        });

        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_hierarchical(scene, idx, Some(this_node_idx), out);
            }
        }
        return;
    }

    if let Some(shape) = block.as_any().downcast_ref::<NiTriShape>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }

        if let Some(mesh) = extract_mesh_local(scene, shape) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            out.meshes.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BsTriShape>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }

        if let Some(mesh) = extract_bs_tri_shape_local(scene, shape) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            out.meshes.push(mesh);
        }
    }
}

/// Recursively walk the scene graph, accumulating world-space transforms (flat, no hierarchy).
fn walk_node_flat(
    scene: &NifScene,
    block_idx: usize,
    parent_transform: &NiTransform,
    out: &mut Vec<ImportedMesh>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    if let Some(node) = block.as_any().downcast_ref::<NiNode>() {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);

        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_flat(scene, idx, &world_transform, out);
            }
        }
        return;
    }

    if let Some(shape) = block.as_any().downcast_ref::<NiTriShape>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);

        if let Some(mesh) = extract_mesh(scene, shape, &world_transform) {
            out.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BsTriShape>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);

        if let Some(mesh) = extract_bs_tri_shape(scene, shape, &world_transform) {
            out.push(mesh);
        }
    }
}

/// Check if a node name is an editor marker that should be skipped.
fn is_editor_marker(name: Option<&str>) -> bool {
    let Some(name) = name else { return false };
    let lower = name.to_ascii_lowercase();
    lower.starts_with("editormarker")
        || lower.starts_with("marker_")
        || lower == "markerx"
        || lower.starts_with("marker:")
}

/// Intermediate geometry data extracted from either NiTriShapeData or NiTriStripsData.
#[allow(dead_code)]
struct GeomData<'a> {
    vertices: &'a [NiPoint3],
    normals: &'a [NiPoint3],
    vertex_colors: &'a [[f32; 4]],
    uv_sets: &'a [Vec<[f32; 2]>],
    triangles: std::borrow::Cow<'a, [[u16; 3]]>,
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
            triangles: std::borrow::Cow::Borrowed(&data.triangles),
        }
    } else if let Some(data) = scene.get_as::<NiTriStripsData>(data_idx) {
        GeomData {
            vertices: &data.vertices,
            normals: &data.normals,
            vertex_colors: &data.vertex_colors,
            uv_sets: &data.uv_sets,
            triangles: std::borrow::Cow::Owned(data.to_triangles()),
        }
    } else {
        return None;
    };

    if geom.vertices.is_empty() || geom.triangles.is_empty() {
        return None;
    }

    // Convert positions: Gamebryo Z-up → renderer Y-up: (x,y,z) → (x,z,-y)
    let positions: Vec<[f32; 3]> = geom.vertices.iter().map(|v| [v.x, v.z, -v.y]).collect();

    // Convert indices (u16 → u32). Winding order preserved — the Z-up → Y-up
    // transform is a proper rotation (det=+1), not a reflection.
    let indices: Vec<u32> = geom
        .triangles
        .iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    // Convert normals with same axis swap (fall back to +Y up if none)
    let normals: Vec<[f32; 3]> = if !geom.normals.is_empty() {
        geom.normals.iter().map(|n| [n.x, n.z, -n.y]).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    // Get UVs from first UV set (if available)
    let uvs = geom.uv_sets.first().cloned().unwrap_or_default();

    // Determine vertex colors: prefer per-vertex colors, then material diffuse, then white
    let (colors, texture_path) = extract_material(scene, shape, &geom);

    // Apply Z-up → Y-up to the entity transform.
    let t = &world_transform.translation;
    let r = &world_transform.rotation;

    // Convert the Z-up rotation matrix to Y-up, then extract a robust quaternion.
    let quat = zup_matrix_to_yup_quat(r);

    // Single-pass material property extraction (alpha, two-sided, decal).
    let mat = extract_material_info(scene, shape);

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        uvs,
        indices,
        translation: [t.x, t.z, -t.y],
        rotation: quat,
        scale: world_transform.scale,
        name: shape.av.net.name.clone(),
        texture_path,
        has_alpha: mat.alpha_blend,
        two_sided: mat.two_sided,
        is_decal: mat.is_decal,
        normal_map: mat.normal_map,
        emissive_color: mat.emissive_color,
        emissive_mult: mat.emissive_mult,
        specular_color: mat.specular_color,
        specular_strength: mat.specular_strength,
        glossiness: mat.glossiness,
        uv_offset: mat.uv_offset,
        uv_scale: mat.uv_scale,
        mat_alpha: mat.alpha,
        env_map_scale: mat.env_map_scale,
        parent_node: None,
    })
}

/// Extract an ImportedMesh with local transform (for hierarchical import).
fn extract_mesh_local(scene: &NifScene, shape: &NiTriShape) -> Option<ImportedMesh> {
    // Pass shape's own transform as the "world" transform — extract_mesh converts it to Y-up.
    extract_mesh(scene, shape, &shape.av.transform)
}

/// Extract an ImportedMesh from a BsTriShape (Skyrim SE+ self-contained geometry).
///
/// BsTriShape embeds vertex data directly — no separate data block needed.
fn extract_bs_tri_shape(
    scene: &NifScene,
    shape: &BsTriShape,
    world_transform: &NiTransform,
) -> Option<ImportedMesh> {
    if shape.vertices.is_empty() || shape.triangles.is_empty() {
        return None;
    }

    // Convert positions: Gamebryo Z-up → renderer Y-up
    let positions: Vec<[f32; 3]> = shape.vertices.iter().map(|v| [v.x, v.z, -v.y]).collect();

    let indices: Vec<u32> = shape
        .triangles
        .iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    let normals: Vec<[f32; 3]> = if !shape.normals.is_empty() {
        shape.normals.iter().map(|n| [n.x, n.z, -n.y]).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    let uvs = shape.uvs.clone();

    // Vertex colors
    let colors: Vec<[f32; 3]> = if !shape.vertex_colors.is_empty() {
        shape
            .vertex_colors
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect()
    } else {
        vec![[1.0, 1.0, 1.0]; positions.len()]
    };

    // Find texture via shader_property_ref → BSLightingShaderProperty → BSShaderTextureSet.
    let texture_path = find_texture_path_bs_tri_shape(scene, shape);

    // Check alpha via dedicated alpha_property_ref.
    let has_alpha = if let Some(idx) = shape.alpha_property_ref.index() {
        scene
            .get_as::<NiAlphaProperty>(idx)
            .map(|a| a.flags & 1 != 0)
            .unwrap_or(false)
    } else {
        false
    };

    // Check two-sided via BSLightingShaderProperty shader_flags_2.
    let two_sided = if let Some(idx) = shape.shader_property_ref.index() {
        scene
            .get_as::<BSLightingShaderProperty>(idx)
            .map(|s| s.shader_flags_2 & 0x10 != 0)
            .unwrap_or(false)
    } else {
        false
    };

    let t = &world_transform.translation;
    let quat = zup_matrix_to_yup_quat(&world_transform.rotation);

    // Extract material data from BSLightingShaderProperty (if present).
    let (
        emissive_color,
        emissive_mult,
        specular_color,
        specular_strength,
        glossiness,
        uv_offset,
        uv_scale,
        mat_alpha,
        normal_map,
        env_map_scale,
    ) = if let Some(idx) = shape.shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            let nm = shader
                .texture_set_ref
                .index()
                .and_then(|ts_idx| scene.get_as::<BSShaderTextureSet>(ts_idx))
                .and_then(|ts| ts.textures.get(1).cloned())
                .filter(|s| !s.is_empty());
            let ems = if let ShaderTypeData::EnvironmentMap { env_map_scale } = shader.shader_type_data {
                env_map_scale
            } else {
                1.0
            };
            (
                shader.emissive_color,
                shader.emissive_multiple,
                shader.specular_color,
                shader.specular_strength,
                shader.glossiness,
                shader.uv_offset,
                shader.uv_scale,
                shader.alpha,
                nm,
                ems,
            )
        } else {
            (
                [0.0; 3], 1.0, [1.0; 3], 1.0, 80.0, [0.0; 2], [1.0; 2], 1.0, None, 1.0,
            )
        }
    } else {
        (
            [0.0; 3], 1.0, [1.0; 3], 1.0, 80.0, [0.0; 2], [1.0; 2], 1.0, None, 1.0,
        )
    };

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        uvs,
        indices,
        translation: [t.x, t.z, -t.y],
        rotation: quat,
        scale: world_transform.scale,
        name: shape.av.net.name.clone(),
        texture_path,
        has_alpha,
        two_sided,
        is_decal: find_decal_bs(scene, shape),
        normal_map,
        emissive_color,
        emissive_mult,
        specular_color,
        specular_strength,
        glossiness,
        uv_offset,
        uv_scale,
        mat_alpha,
        env_map_scale,
        parent_node: None,
    })
}

/// Extract a BsTriShape with local transform (for hierarchical import).
fn extract_bs_tri_shape_local(scene: &NifScene, shape: &BsTriShape) -> Option<ImportedMesh> {
    extract_bs_tri_shape(scene, shape, &shape.av.transform)
}

/// Find texture path for BsTriShape via its shader_property_ref.
fn find_texture_path_bs_tri_shape(scene: &NifScene, shape: &BsTriShape) -> Option<String> {
    if let Some(idx) = shape.shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            if let Some(ts_idx) = shader.texture_set_ref.index() {
                if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                    if let Some(path) = tex_set.textures.first() {
                        if !path.is_empty() {
                            return Some(path.clone());
                        }
                    }
                }
            }
        }
        if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
            if !shader.source_texture.is_empty() {
                return Some(shader.source_texture.clone());
            }
        }
    }
    None
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
        let colors = data
            .vertex_colors
            .iter()
            .map(|c| [c[0], c[1], c[2]]) // drop alpha
            .collect();
        let tex = find_texture_path(scene, shape);
        return (colors, tex);
    }

    // Search properties for NiMaterialProperty
    let mut diffuse = [1.0f32; 3]; // default white
    for prop_ref in &shape.av.properties {
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

/// Material properties extracted from a NiTriShape's property list in a single pass.
///
/// Replaces four independent property-list iterations with one.
#[derive(Debug)]
struct MaterialInfo {
    texture_path: Option<String>,
    normal_map: Option<String>,
    alpha_blend: bool,
    two_sided: bool,
    is_decal: bool,
    emissive_color: [f32; 3],
    emissive_mult: f32,
    specular_color: [f32; 3],
    specular_strength: f32,
    glossiness: f32,
    uv_offset: [f32; 2],
    uv_scale: [f32; 2],
    alpha: f32,
    /// Environment map scale from shader type 1 (EnvironmentMap).
    env_map_scale: f32,
    has_material_data: bool,
}

impl Default for MaterialInfo {
    fn default() -> Self {
        Self {
            texture_path: None,
            normal_map: None,
            alpha_blend: false,
            two_sided: false,
            is_decal: false,
            emissive_color: [0.0, 0.0, 0.0],
            emissive_mult: 1.0,
            specular_color: [1.0, 1.0, 1.0],
            specular_strength: 1.0,
            glossiness: 80.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            alpha: 1.0,
            env_map_scale: 1.0,
            has_material_data: false,
        }
    }
}

const DECAL_SINGLE_PASS: u32 = 0x04000000;
const DYNAMIC_DECAL: u32 = 0x08000000;
const ALPHA_DECAL_F2: u32 = 0x00200000;
const SF_DOUBLE_SIDED: u32 = 0x1000;

/// Extract all material properties from a NiTriShape in a single pass.
///
/// Checks both dedicated refs (Skyrim+) and the properties list (FO3/FNV/Oblivion).
fn extract_material_info(scene: &NifScene, shape: &NiTriShape) -> MaterialInfo {
    let mut info = MaterialInfo::default();

    // Skyrim+: dedicated shader_property_ref
    if let Some(idx) = shape.shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            if let Some(ts_idx) = shader.texture_set_ref.index() {
                if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                    if let Some(path) = tex_set.textures.first() {
                        if !path.is_empty() {
                            info.texture_path = Some(path.clone());
                        }
                    }
                    // Normal map is textures[1] in BSShaderTextureSet.
                    if let Some(normal) = tex_set.textures.get(1) {
                        if !normal.is_empty() {
                            info.normal_map = Some(normal.clone());
                        }
                    }
                }
            }
            if shader.shader_flags_2 & 0x10 != 0 {
                info.two_sided = true;
            }
            if shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 {
                info.is_decal = true;
            }
            // Capture rich material data.
            info.emissive_color = shader.emissive_color;
            info.emissive_mult = shader.emissive_multiple;
            info.specular_color = shader.specular_color;
            info.specular_strength = shader.specular_strength;
            info.glossiness = shader.glossiness;
            info.uv_offset = shader.uv_offset;
            info.uv_scale = shader.uv_scale;
            info.alpha = shader.alpha;
            if let ShaderTypeData::EnvironmentMap { env_map_scale } = shader.shader_type_data {
                info.env_map_scale = env_map_scale;
            }
            info.has_material_data = true;
        }
        if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
            if info.texture_path.is_none() && !shader.source_texture.is_empty() {
                info.texture_path = Some(shader.source_texture.clone());
            }
            if !info.has_material_data {
                info.emissive_color = [
                    shader.emissive_color[0],
                    shader.emissive_color[1],
                    shader.emissive_color[2],
                ];
                info.emissive_mult = shader.emissive_multiple;
                info.uv_offset = shader.uv_offset;
                info.uv_scale = shader.uv_scale;
                info.has_material_data = true;
            }
        }
    }

    // Skyrim+: dedicated alpha_property_ref
    if let Some(idx) = shape.alpha_property_ref.index() {
        if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
            if alpha.flags & 1 != 0 {
                info.alpha_blend = true;
            }
        }
    }

    // FO3/FNV/Oblivion: single pass over properties list
    for prop_ref in &shape.av.properties {
        let Some(idx) = prop_ref.index() else {
            continue;
        };

        if !info.alpha_blend {
            if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
                if alpha.flags & 1 != 0 {
                    info.alpha_blend = true;
                }
            }
        }

        // NiMaterialProperty — capture specular/emissive/shininess/alpha.
        if !info.has_material_data {
            if let Some(mat) = scene.get_as::<NiMaterialProperty>(idx) {
                info.specular_color = [mat.specular.r, mat.specular.g, mat.specular.b];
                info.emissive_color = [mat.emissive.r, mat.emissive.g, mat.emissive.b];
                info.glossiness = mat.shininess;
                info.alpha = mat.alpha;
                info.emissive_mult = mat.emissive_mult;
                info.has_material_data = true;
            }
        }

        if info.texture_path.is_none() {
            if let Some(tex_prop) = scene.get_as::<NiTexturingProperty>(idx) {
                if let Some(ref base) = tex_prop.base_texture {
                    if let Some(src_idx) = base.source_ref.index() {
                        if let Some(src_tex) = scene.get_as::<NiSourceTexture>(src_idx) {
                            if src_tex.filename.is_some() {
                                info.texture_path = src_tex.filename.clone();
                            }
                        }
                    }
                }
            }
        }

        if let Some(shader) = scene.get_as::<BSShaderPPLightingProperty>(idx) {
            if info.texture_path.is_none() {
                if let Some(ts_idx) = shader.texture_set_ref.index() {
                    if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                        if let Some(path) = tex_set.textures.first() {
                            if !path.is_empty() {
                                info.texture_path = Some(path.clone());
                            }
                        }
                    }
                }
            }
            if shader.shader.shader_flags_1 & SF_DOUBLE_SIDED != 0 {
                info.two_sided = true;
            }
            if shader.shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0
                || shader.shader.shader_flags_2 & ALPHA_DECAL_F2 != 0
            {
                info.is_decal = true;
            }
        }

        if let Some(shader) = scene.get_as::<BSShaderNoLightingProperty>(idx) {
            if info.texture_path.is_none() && !shader.file_name.is_empty() {
                info.texture_path = Some(shader.file_name.clone());
            }
            if shader.shader.shader_flags_1 & SF_DOUBLE_SIDED != 0 {
                info.two_sided = true;
            }
            if shader.shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 {
                info.is_decal = true;
            }
        }

        // NiStencilProperty — proper parser replaces NiUnknown heuristic.
        if !info.two_sided {
            if let Some(stencil) = scene.get_as::<NiStencilProperty>(idx) {
                if stencil.is_two_sided() {
                    info.two_sided = true;
                }
            }
        }
    }

    info
}

/// Texture path only — delegates to extract_material_info.
fn find_texture_path(scene: &NifScene, shape: &NiTriShape) -> Option<String> {
    extract_material_info(scene, shape).texture_path
}

/// Check if a BsTriShape is decal geometry (Skyrim+).
fn find_decal_bs(scene: &NifScene, shape: &BsTriShape) -> bool {
    const DECAL_SINGLE_PASS: u32 = 0x04000000;
    const DYNAMIC_DECAL: u32 = 0x08000000;

    if let Some(idx) = shape.shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            if shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 {
                return true;
            }
        }
    }
    false
}

/// Compose parent * child transforms.
///
/// `NiTransform` composition: rotation = parent.rot * child.rot,
/// translation = parent.rot * (parent.scale * child.trans) + parent.trans,
/// scale = parent.scale * child.scale.
fn compose_transforms(parent: &NiTransform, child: &NiTransform) -> NiTransform {
    let parent_rot = if is_degenerate_rotation(&parent.rotation) {
        // SVD-repair and check if the original had meaningful orientation.
        // Near-zero matrices (e.g. BSFadeNode roots with garbage data) have tiny
        // singular values — SVD produces an arbitrary rotation that would scatter
        // child positions. Fall back to identity for those.
        // Scaled rotations (det >> 1 but valid orientation) get the correct
        // rotation extracted via SVD.
        repair_rotation_svd_or_identity(&parent.rotation)
    } else {
        parent.rotation
    };

    let rot = mul_matrix3(&parent_rot, &child.rotation);
    let scaled_child_trans = scale_point(child.translation, parent.scale);
    let rotated = mul_matrix3_point(&parent_rot, scaled_child_trans);
    let translation = add_points(parent.translation, rotated);
    let scale = parent.scale * child.scale;

    NiTransform {
        rotation: rot,
        translation,
        scale,
    }
}

/// Check if a rotation matrix is degenerate (det far from 1.0).
fn is_degenerate_rotation(m: &NiMatrix3) -> bool {
    let r = &m.rows;
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
        - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
        + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    (det - 1.0).abs() >= 0.1
}

/// SVD-repair a degenerate rotation matrix, or return identity if the matrix
/// has no meaningful orientation (all singular values near zero).
///
/// Uses nalgebra SVD: M = U*Σ*Vt → nearest rotation = U*Vt (with det correction).
/// If the maximum singular value is below a threshold, the matrix is considered
/// garbage (e.g. BSFadeNode with zeroed rotation) and identity is returned.
fn repair_rotation_svd_or_identity(m: &NiMatrix3) -> NiMatrix3 {
    use nalgebra::Matrix3;

    let r = &m.rows;
    let mat = Matrix3::new(
        r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2],
    );

    let svd = mat.svd(true, true);

    // If the largest singular value is tiny, the matrix carries no meaningful
    // orientation — return identity rather than an arbitrary SVD rotation.
    let max_sv = svd.singular_values.max();
    if max_sv < 0.01 {
        return NiMatrix3::default();
    }

    let u = svd.u.unwrap();
    let vt = svd.v_t.unwrap();
    let mut nearest = u * vt;

    if nearest.determinant() < 0.0 {
        let mut u_fixed = u;
        u_fixed.column_mut(2).scale_mut(-1.0);
        nearest = u_fixed * vt;
    }

    NiMatrix3 {
        rows: [
            [nearest[(0, 0)], nearest[(0, 1)], nearest[(0, 2)]],
            [nearest[(1, 0)], nearest[(1, 1)], nearest[(1, 2)]],
            [nearest[(2, 0)], nearest[(2, 1)], nearest[(2, 2)]],
        ],
    }
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
    NiPoint3 {
        x: p.x * s,
        y: p.y * s,
        z: p.z * s,
    }
}

fn add_points(a: NiPoint3, b: NiPoint3) -> NiPoint3 {
    NiPoint3 {
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
    }
}

/// Convert a Z-up NiMatrix3 rotation to a Y-up quaternion [x, y, z, w].
///
/// Gamebryo uses a clockwise-positive rotation convention, so its rotation
/// matrices are the transpose of the standard (CCW) convention. However,
/// the matrix × point multiplication produces the SAME physical result
/// regardless of convention — the matrix IS the rotation. So we can
/// extract a quaternion directly from the NIF matrix without transposing.
///
/// Uses SVD decomposition (via nalgebra) to handle degenerate matrices
/// that Gamebryo NIF files sometimes contain (rank-deficient, det=0).
/// The nearest valid rotation matrix is extracted as U*Vt from the SVD,
/// then the Z-up → Y-up coordinate change is applied.
fn zup_matrix_to_yup_quat(m: &NiMatrix3) -> [f32; 4] {
    use nalgebra::{Matrix3, UnitQuaternion};

    let r = &m.rows;

    // Apply the Z-up → Y-up axis swap to the rotation matrix:
    // C: (x,y,z)_zup → (x,z,-y)_yup
    // R_yup = C * R_zup * C^T
    let yup = Matrix3::new(
        r[0][0], r[0][2], -r[0][1], // X row, columns swapped
        r[2][0], r[2][2], -r[2][1], // Z row becomes Y row
        -r[1][0], -r[1][2], r[1][1], // -Y row becomes Z row
    );

    // Fast path: if det ≈ 1.0, the matrix is already a valid rotation and
    // we can extract the quaternion directly. This is the common case (~99%
    // of NIF matrices). SVD is only needed for degenerate matrices (zeroed
    // BSFadeNode rotations, scaled/sheared matrices from bad exports).
    let det = yup.determinant();
    let rotation_matrix = if (det - 1.0).abs() < 0.1 {
        yup
    } else {
        // Degenerate — SVD repair: M = U*Σ*Vt → nearest rotation = U*Vt.
        let svd = yup.svd(true, true);
        let u = svd.u.unwrap();
        let vt = svd.v_t.unwrap();
        let mut nearest = u * vt;

        if nearest.determinant() < 0.0 {
            let mut u_fixed = u;
            u_fixed.column_mut(2).scale_mut(-1.0);
            nearest = u_fixed * vt;
        }
        nearest
    };

    let rot = nalgebra::Rotation3::from_matrix_unchecked(rotation_matrix);
    let q = UnitQuaternion::from_rotation_matrix(&rot);

    [q.i, q.j, q.k, q.w]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BlockRef, NiColor};

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

    fn make_ni_node(transform: NiTransform, children: Vec<BlockRef>) -> NiNode {
        use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
        NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some("TestNode".to_string()),
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
    ) -> NiTriShape {
        use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
        NiTriShape {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(name.to_string()),
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
        // Block 0: NiNode (root) with child → block 1
        // Block 1: NiTriShape with data_ref → block 2
        // Block 2: NiTriShapeData
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
        // Identity transform (Z-up → Y-up is identity for zero translation)
        assert_eq!(m.translation, [0.0, 0.0, 0.0]);
        assert_eq!(m.scale, 1.0);
    }

    #[test]
    fn import_inherits_parent_translation() {
        // Root node translated by (10, 0, 0), child shape at identity
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
        // Root at (5, 0, 0) → Child NiNode at (0, 3, 0) → Shape at identity
        // NIF-space composed: (5, 3, 0)
        // After Z-up → Y-up (x,z,-y): (5, 0, -3)
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
        // Root scale 2.0, shape at (1, 0, 0) with scale 3.0
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

        // Block 0: root, Block 1: shape (props→[3]), Block 2: data, Block 3: material
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

    // ── Z-up → Y-up coordinate conversion regression tests ─────────

    #[test]
    fn zup_to_yup_vertex_positions() {
        // Regression: Gamebryo is Z-up, renderer is Y-up.
        // Conversion: (x,y,z) → (x, z, -y)
        // make_tri_shape_data has vertices at (0,0,0), (1,0,0), (0,1,0)
        // After conversion: (0,0,0), (1,0,0), (0,0,-1)
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
        assert_eq!(m.positions[2], [0.0, 0.0, -1.0]); // Z-up (0,1,0) → Y-up (0,0,-1)
    }

    #[test]
    fn zup_to_yup_vertex_normals() {
        // make_tri_shape_data normals are all (0,0,1) in Z-up
        // After conversion: (0, 1, 0) — pointing up in Y-up
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
            assert_eq!(*n, [0.0, 1.0, 0.0]); // Z-up (0,0,1) → Y-up (0,1,0)
        }
    }

    #[test]
    fn zup_to_yup_translation() {
        // A node translated along NIF-Z (up) should become Y (up) in renderer.
        // NIF translation (0, 0, 5) → Y-up (0, 5, 0)
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(translated(0.0, 0.0, 5.0), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape("Up", identity_transform(), 2, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert!((meshes[0].translation[0]).abs() < 1e-6); // X unchanged
        assert!((meshes[0].translation[1] - 5.0).abs() < 1e-6); // Z→Y
        assert!((meshes[0].translation[2]).abs() < 1e-6); // Y→-Z (was 0)
    }

    #[test]
    fn zup_to_yup_translation_forward() {
        // NIF Y-axis (forward in Z-up) maps to -Z in Y-up.
        // NIF translation (0, 7, 0) → Y-up (0, 0, -7)
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
        // An identity rotation in NIF space should remain identity after conversion.
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape("Id", identity_transform(), 2, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        // Identity rotation → quaternion [0, 0, 0, 1]
        let q = &meshes[0].rotation;
        assert!(q[0].abs() < 1e-4, "qx={}", q[0]); // x
        assert!(q[1].abs() < 1e-4, "qy={}", q[1]); // y
        assert!(q[2].abs() < 1e-4, "qz={}", q[2]); // z
        assert!((q[3].abs() - 1.0).abs() < 1e-4, "qw={}", q[3]); // w = ±1
    }

    #[test]
    fn zup_to_yup_winding_order_preserved() {
        // The Z→Y conversion is a proper rotation (det=+1), not a reflection,
        // so triangle winding order must stay the same.
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

        // Original triangle: [0, 1, 2] — winding must be preserved
        assert_eq!(meshes[0].indices, vec![0, 1, 2]);
    }

    // ── Degenerate rotation regression tests ──────────────────────────

    #[test]
    fn compose_degenerate_zero_matrix_uses_identity() {
        // A parent with an all-zero rotation matrix (garbage BSFadeNode data).
        // Children should pass through with their own transforms intact.
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
        let result = compose_transforms(&parent, &child);

        // With identity fallback: child translation passes through unrotated.
        assert!((result.translation.x - 15.0).abs() < 1e-4);
        assert!((result.translation.y).abs() < 1e-4);
        assert!((result.translation.z).abs() < 1e-4);
    }

    #[test]
    fn compose_degenerate_scaled_rotation_uses_svd() {
        // A parent with a 2x-scaled identity rotation (det=8, degenerate threshold).
        // SVD should extract the identity rotation and use it for both
        // rotation composition and translation rotation.
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
        let result = compose_transforms(&parent, &child);

        // SVD extracts identity from 2*I → child translation passes through.
        assert!((result.translation.x - 3.0).abs() < 1e-4);
        assert!((result.translation.y - 4.0).abs() < 1e-4);
        assert!((result.translation.z - 5.0).abs() < 1e-4);
    }

    #[test]
    fn compose_degenerate_scaled_rotation_rotates_child() {
        // A parent with a 2x-scaled 90° rotation around Z.
        // det = 8 → degenerate. SVD should extract the 90° Z rotation
        // and apply it to both rotation and translation.
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
        // Child at (1, 0, 0). After 90° Z rotation → (0, 1, 0).
        let child = translated(1.0, 0.0, 0.0);
        let result = compose_transforms(&parent, &child);

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
        // A NIF matrix representing 90° CCW around Z-up.
        // In Gamebryo CW convention this is Rz_cw(-90°), but the physical
        // rotation is the same — the matrix IS the rotation, convention only
        // affects angle labeling. After Z→Y conversion: 90° CCW around Y.
        //
        // Standard Rz(90°) = [[0,-1,0],[1,0,0],[0,0,1]]
        // Gamebryo Rz_cw(-90°) = same matrix (CW by -90° = CCW by 90°)
        let rot_z90 = NiMatrix3 {
            rows: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        };
        let q = zup_matrix_to_yup_quat(&rot_z90);
        // Expected: 90° CCW around Y → quat (0, sin(45°), 0, cos(45°))
        let sin45 = std::f32::consts::FRAC_PI_4.sin();
        let cos45 = std::f32::consts::FRAC_PI_4.cos();
        assert!(q[0].abs() < 1e-4, "qx={}", q[0]);
        assert!((q[1].abs() - sin45).abs() < 1e-4, "qy={}", q[1]);
        assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
        assert!((q[3].abs() - cos45).abs() < 1e-4, "qw={}", q[3]);
    }

    #[test]
    fn zup_to_yup_90deg_ccw_rotation_around_x() {
        // A NIF matrix representing 90° CCW around X-axis.
        // Standard Rx(90°) = [[1,0,0],[0,0,-1],[0,1,0]]
        // After Z→Y: still 90° CCW around X (X axis is unchanged by the conversion).
        let rot_x90 = NiMatrix3 {
            rows: [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        };
        let q = zup_matrix_to_yup_quat(&rot_x90);
        // Expected: 90° CCW around X → quat (sin(45°), 0, 0, cos(45°))
        let sin45 = std::f32::consts::FRAC_PI_4.sin();
        let cos45 = std::f32::consts::FRAC_PI_4.cos();
        assert!((q[0].abs() - sin45).abs() < 1e-4, "qx={}", q[0]);
        assert!(q[1].abs() < 1e-4, "qy={}", q[1]);
        assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
        assert!((q[3].abs() - cos45).abs() < 1e-4, "qw={}", q[3]);
    }
}
