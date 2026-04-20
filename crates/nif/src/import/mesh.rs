//! Geometry extraction from NiTriShape and BsTriShape blocks.

use std::sync::Arc;

use crate::blocks::node::NiNode;
use crate::blocks::properties::NiAlphaProperty;
use crate::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderTextureSet, ShaderTypeData,
};
use crate::blocks::skin::{
    BsDismemberSkinInstance, BsSkinBoneData, BsSkinInstance, NiSkinData, NiSkinInstance,
};
use crate::blocks::tri_shape::{BsTriShape, NiTriShape, NiTriShapeData, NiTriStripsData};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};

use super::coord::{zup_matrix_to_yup_quat, zup_point_to_yup};
use super::material::{
    capture_shader_type_fields, extract_material_info, extract_vertex_colors, find_decal_bs,
    find_effect_shader_bs,
};
use super::{ImportedBone, ImportedMesh, ImportedSkin};

/// Intermediate geometry data extracted from either NiTriShapeData or NiTriStripsData.
#[allow(dead_code)]
pub(super) struct GeomData<'a> {
    pub vertices: &'a [NiPoint3],
    pub normals: &'a [NiPoint3],
    pub vertex_colors: &'a [[f32; 4]],
    pub uv_sets: &'a [Vec<[f32; 2]>],
    pub triangles: std::borrow::Cow<'a, [[u16; 3]]>,
    /// NIF-provided bounding sphere center, still in Gamebryo Z-up space.
    /// Zero when the NIF omits a bound — the caller then computes one
    /// from the positions. See #217.
    pub bound_center: NiPoint3,
    /// NIF-provided bounding sphere radius (no axis conversion needed).
    pub bound_radius: f32,
}

/// Extract an ImportedMesh from an NiTriShape and its referenced data block.
pub(super) fn extract_mesh(
    scene: &NifScene,
    shape: &NiTriShape,
    world_transform: &NiTransform,
    inherited_props: &[BlockRef],
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
            bound_center: data.center,
            bound_radius: data.radius,
        }
    } else if let Some(data) = scene.get_as::<NiTriStripsData>(data_idx) {
        GeomData {
            vertices: &data.vertices,
            normals: &data.normals,
            vertex_colors: &data.vertex_colors,
            uv_sets: &data.uv_sets,
            triangles: std::borrow::Cow::Owned(data.to_triangles()),
            bound_center: data.center,
            bound_radius: data.radius,
        }
    } else {
        return None;
    };

    if geom.vertices.is_empty() || geom.triangles.is_empty() {
        return None;
    }

    // Convert positions: Gamebryo Z-up → renderer Y-up (see `coord.rs`).
    let positions: Vec<[f32; 3]> = geom.vertices.iter().map(zup_point_to_yup).collect();

    // Convert indices (u16 → u32). Winding order preserved — the Z-up → Y-up
    // transform is a proper rotation (det=+1), not a reflection.
    let indices: Vec<u32> = geom
        .triangles
        .iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    // Convert normals with same axis swap (fall back to +Y up if none)
    let normals: Vec<[f32; 3]> = if !geom.normals.is_empty() {
        geom.normals.iter().map(zup_point_to_yup).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    // Get UVs from first UV set (if available)
    let uvs = geom.uv_sets.first().cloned().unwrap_or_default();

    // Single-pass material property extraction — called once and reused for
    // both vertex color resolution and material fields. Eliminates the double
    // extract_material_info that previously occurred via extract_material →
    // find_texture_path → extract_material_info + direct call. #279 D5-10.
    let mat = extract_material_info(scene, shape, inherited_props);

    // Determine vertex colors: prefer per-vertex colors, then material diffuse, then white.
    let colors = extract_vertex_colors(scene, shape, &geom, inherited_props, &mat);

    // Apply Z-up → Y-up to the entity transform.
    let t = &world_transform.translation;
    let r = &world_transform.rotation;

    // Convert the Z-up rotation matrix to Y-up, then extract a robust quaternion.
    let quat = zup_matrix_to_yup_quat(r);

    // Skinning data (issue #151). Populated when the shape has a
    // NiSkinInstance / BSDismemberSkinInstance backing it.
    let skin = extract_skin_ni_tri_shape(scene, shape, positions.len());

    // Local bounding sphere in Y-up renderer space. Prefer the NIF-provided
    // NiBound on NiGeometryData; fall back to a fresh centroid+max-distance
    // sphere computed from the positions when the NIF omits one (radius 0).
    // See #217.
    let (local_bound_center, local_bound_radius) =
        extract_local_bound(geom.bound_center, geom.bound_radius, &positions);

    // Capture the shader-type fields before moving other `mat` fields into
    // the `ImportedMesh` literal. See #430.
    let shader_type_fields = mat.shader_type_fields();

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        uvs,
        indices,
        translation: zup_point_to_yup(t),
        rotation: quat,
        scale: world_transform.scale,
        name: shape.av.net.name.clone(),
        texture_path: mat.texture_path,
        material_path: mat.material_path,
        has_alpha: mat.alpha_blend,
        src_blend_mode: mat.src_blend_mode,
        dst_blend_mode: mat.dst_blend_mode,
        alpha_test: mat.alpha_test,
        alpha_threshold: mat.alpha_threshold,
        alpha_test_func: mat.alpha_test_func,
        two_sided: mat.two_sided,
        is_decal: mat.is_decal,
        normal_map: mat.normal_map,
        glow_map: mat.glow_map,
        detail_map: mat.detail_map,
        gloss_map: mat.gloss_map,
        dark_map: mat.dark_map,
        parallax_map: mat.parallax_map,
        env_map: mat.env_map,
        env_mask: mat.env_mask,
        parallax_max_passes: mat.parallax_max_passes,
        parallax_height_scale: mat.parallax_height_scale,
        vertex_color_mode: mat.vertex_color_mode as u8,
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
        skin,
        z_test: mat.z_test,
        z_write: mat.z_write,
        z_function: mat.z_function,
        local_bound_center,
        local_bound_radius,
        effect_shader: mat.effect_shader,
        material_kind: mat.material_kind,
        // #430 — surface SkinTint / HairTint / EyeEnvmap / ParallaxOcc /
        // MultiLayerParallax / SparkleSnow fields on the mesh.
        // `extract_material_info` already populated them on MaterialInfo
        // via `apply_shader_type_data`; before this fix they died here.
        shader_type_fields,
        // #451 — forward the BSShaderNoLightingProperty soft-falloff
        // cone (FO3/FNV HUD overlays). `None` for non-NoLighting meshes.
        no_lighting_falloff: mat.no_lighting_falloff,
        flags: shape.av.flags,
    })
}

/// Produce a mesh-local bounding sphere in Y-up renderer space.
///
/// If the NIF supplied a non-zero `center`/`radius` (from `NiGeometryData`
/// or `BsTriShape`), convert the center from Gamebryo Z-up to Y-up and
/// return it — this is cheap and matches what the game engine computed
/// at export time. When the NIF bound is zero (legacy content or
/// auto-generated meshes) fall back to computing a centroid+max-distance
/// sphere from the already-converted vertex positions.
fn extract_local_bound(
    nif_center: NiPoint3,
    nif_radius: f32,
    positions_yup: &[[f32; 3]],
) -> ([f32; 3], f32) {
    if nif_radius > 0.0 {
        return (zup_point_to_yup(&nif_center), nif_radius);
    }
    if positions_yup.is_empty() {
        return ([0.0; 3], 0.0);
    }
    let mut sum = [0.0f32; 3];
    for p in positions_yup {
        sum[0] += p[0];
        sum[1] += p[1];
        sum[2] += p[2];
    }
    let inv_n = 1.0 / positions_yup.len() as f32;
    let center = [sum[0] * inv_n, sum[1] * inv_n, sum[2] * inv_n];
    let mut max_sq = 0.0f32;
    for p in positions_yup {
        let dx = p[0] - center[0];
        let dy = p[1] - center[1];
        let dz = p[2] - center[2];
        let d_sq = dx * dx + dy * dy + dz * dz;
        if d_sq > max_sq {
            max_sq = d_sq;
        }
    }
    (center, max_sq.sqrt())
}

/// Extract an ImportedMesh with local transform (for hierarchical import).
pub(super) fn extract_mesh_local(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited_props: &[BlockRef],
) -> Option<ImportedMesh> {
    extract_mesh(scene, shape, &shape.av.transform, inherited_props)
}

/// Extract an ImportedMesh from a BsTriShape (Skyrim SE+ self-contained geometry).
pub(super) fn extract_bs_tri_shape(
    scene: &NifScene,
    shape: &BsTriShape,
    world_transform: &NiTransform,
) -> Option<ImportedMesh> {
    if shape.vertices.is_empty() || shape.triangles.is_empty() {
        return None;
    }

    let positions: Vec<[f32; 3]> = shape.vertices.iter().map(zup_point_to_yup).collect();

    let indices: Vec<u32> = shape
        .triangles
        .iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    let normals: Vec<[f32; 3]> = if !shape.normals.is_empty() {
        shape.normals.iter().map(zup_point_to_yup).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    let uvs = shape.uvs.clone();

    let colors: Vec<[f32; 3]> = if !shape.vertex_colors.is_empty() {
        shape
            .vertex_colors
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect()
    } else {
        vec![[1.0, 1.0, 1.0]; positions.len()]
    };

    let texture_path = find_texture_path_bs_tri_shape(scene, shape);
    let material_path = find_material_path_bs_tri_shape(scene, shape);

    // NiAlphaProperty: bit 0 = alpha blend, bit 9 (0x200) = alpha test
    // (cutout). See issue #152. Prefer alpha-test over alpha-blend when
    // both bits are set — same policy as the NiTriShape path in
    // `apply_alpha_flags`.
    let (has_alpha, alpha_test, alpha_threshold, alpha_test_func, src_blend_mode, dst_blend_mode) =
        if let Some(idx) = shape.alpha_property_ref.index() {
            if let Some(a) = scene.get_as::<NiAlphaProperty>(idx) {
                let blend = a.flags & 0x001 != 0;
                let test = a.flags & 0x200 != 0;
                let func = ((a.flags & 0x1C00) >> 10) as u8;
                let src = ((a.flags >> 1) & 0xF) as u8;
                let dst = ((a.flags >> 5) & 0xF) as u8;
                if test {
                    (false, true, a.threshold as f32 / 255.0, func, src, dst)
                } else {
                    (blend, false, 0.0, 6, src, dst)
                }
            } else {
                (false, false, 0.0, 6, 6, 7)
            }
        } else {
            (false, false, 0.0, 6, 6, 7)
        };

    let two_sided = bs_tri_shape_two_sided(scene, shape);

    let t = &world_transform.translation;
    let quat = zup_matrix_to_yup_quat(&world_transform.rotation);

    // Material defaults — used when the shape has no shader property or
    // when the linked block is neither BSLightingShaderProperty nor
    // BSEffectShaderProperty. `ems = 1.0` matches pre-#346 behavior.
    let mut emissive_color = [0.0_f32; 3];
    let mut emissive_mult = 1.0_f32;
    let mut specular_color = [1.0_f32; 3];
    let mut specular_strength = 1.0_f32;
    let mut glossiness = 80.0_f32;
    let mut uv_offset = [0.0_f32; 2];
    let mut uv_scale = [1.0_f32; 2];
    let mut mat_alpha = 1.0_f32;
    let mut normal_map: Option<String> = None;
    let mut parallax_map: Option<String> = None;
    let mut env_map: Option<String> = None;
    let mut env_mask: Option<String> = None;
    let mut env_map_scale = 1.0_f32;
    let mut shader_type_fields = super::material::ShaderTypeFields::default();

    if let Some(idx) = shape.shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            if let Some(ts) = shader
                .texture_set_ref
                .index()
                .and_then(|ts_idx| scene.get_as::<BSShaderTextureSet>(ts_idx))
            {
                normal_map = ts.textures.get(1).cloned().filter(|s| !s.is_empty());
                // #452 / #453 — slots 3/4/5 reach the BSTriShape path
                // alongside the existing slot-1 (normal) pull. Without
                // this, Skyrim+ parallax / env materials dropped every
                // non-base texture regardless of what the NIF carried.
                parallax_map = ts.textures.get(3).cloned().filter(|s| !s.is_empty());
                env_map = ts.textures.get(4).cloned().filter(|s| !s.is_empty());
                env_mask = ts.textures.get(5).cloned().filter(|s| !s.is_empty());
            }
            // `EnvironmentMap` feeds `env_map_scale`; every other variant
            // (SkinTint, HairTint, ParallaxOcc, MultiLayerParallax,
            // SparkleSnow, EyeEnvmap, Fo76SkinTint) carries per-variant
            // data that rides through `ShaderTypeFields` onto the mesh so
            // the renderer can branch on `material_kind`. Fixes #430 —
            // before this change the BsTriShape path silently dropped
            // every SkinTint/HairTint/etc. payload on Skyrim+/FO4/FO76/
            // Starfield characters.
            if let ShaderTypeData::EnvironmentMap {
                env_map_scale: ems,
            } = shader.shader_type_data
            {
                env_map_scale = ems;
            }
            shader_type_fields = capture_shader_type_fields(&shader.shader_type_data);
            emissive_color = shader.emissive_color;
            emissive_mult = shader.emissive_multiple;
            specular_color = shader.specular_color;
            specular_strength = shader.specular_strength;
            glossiness = shader.glossiness;
            uv_offset = shader.uv_offset;
            uv_scale = shader.uv_scale;
            mat_alpha = shader.alpha;
        } else if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
            // BSEffectShaderProperty path — VFX surfaces, magic FX,
            // particle-on-mesh emissives. Pre-#346 the importer ignored
            // this block entirely (apart from `texture_path`, picked up
            // by `find_texture_path_bs_tri_shape`), so emissive multiplier,
            // UV transform, alpha, env-map scale, and the FO4+ normal
            // texture all dropped to their defaults. Mirror the same
            // fields the legacy NiTriShape `extract_material_info` path
            // already pulls from `BSEffectShaderProperty`. Fields
            // renamed to base_color/base_color_scale per #166; still
            // routed into emissive_* because the current fragment
            // shader drives effect-shader glow off the emissive slot.
            emissive_color = [
                shader.base_color[0],
                shader.base_color[1],
                shader.base_color[2],
            ];
            emissive_mult = shader.base_color_scale;
            uv_offset = shader.uv_offset;
            uv_scale = shader.uv_scale;
            mat_alpha = shader.base_color[3]; // BGEM uses alpha channel of base color
            // FO4+ effect shaders carry their own normal/env textures
            // (BSVER >= 130). Pre-FO4 those strings are empty.
            if !shader.normal_texture.is_empty() {
                normal_map = Some(shader.normal_texture.clone());
            }
            env_map_scale = shader.env_map_scale;
        }
    }

    // Skinning data. BSTriShape per-vertex weights live in the packed
    // vertex buffer (VF_SKINNED), decoded at parse time (#177).
    let skin = extract_skin_bs_tri_shape(scene, shape);

    // BSTriShape carries its own bounding sphere (center + radius) on the
    // block. See #217.
    let (local_bound_center, local_bound_radius) =
        extract_local_bound(shape.center, shape.radius, &positions);

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        uvs,
        indices,
        translation: zup_point_to_yup(t),
        rotation: quat,
        scale: world_transform.scale,
        name: shape.av.net.name.clone(),
        texture_path,
        material_path,
        has_alpha,
        src_blend_mode,
        dst_blend_mode,
        alpha_test,
        alpha_threshold,
        alpha_test_func,
        two_sided,
        is_decal: find_decal_bs(scene, shape),
        normal_map,
        // BsTriShape (Skyrim+) routes all texture slots through
        // BSShaderTextureSet, which this path reads above. The legacy
        // NiTexturingProperty glow/detail/gloss slots don't apply here,
        // so leave them as `None`. Skyrim+ glow maps live in
        // BSShaderTextureSet slot 2; wiring those is a separate task
        // once we teach the renderer to sample a third slot. See #214.
        glow_map: None,
        detail_map: None,
        gloss_map: None,
        dark_map: None, // BSTriShape doesn't use NiTexturingProperty slots
        // BSShaderTextureSet slots 3/4/5 — pulled above from the
        // BSLightingShaderProperty texture set. #453.
        parallax_map,
        env_map,
        env_mask,
        // ParallaxOcc / MultiLayerParallax scalars arrive via
        // `shader_type_fields` (captured from ShaderTypeData). Mirror
        // them into the dedicated Option<f32>s so the renderer side
        // of #453 reads a single canonical field regardless of path.
        parallax_max_passes: shader_type_fields.parallax_max_passes,
        parallax_height_scale: shader_type_fields.parallax_height_scale,
        // BsTriShape vertex colors are driven by the shader
        // properties, not an NiVertexColorProperty — pass the default
        // (AmbientDiffuse = 2) so downstream consumers behave the same
        // as before.
        vertex_color_mode: 2,
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
        skin,
        // BSTriShape (Skyrim+) has no NiZBufferProperty; defaults to
        // Gamebryo runtime defaults (z_test+write on, LESSEQUAL).
        z_test: true,
        z_write: true,
        z_function: 3,
        local_bound_center,
        local_bound_radius,
        // BsTriShape with an effect-shader parent (VFX surfaces, magic
        // overlays, BGEM materials) — capture the rich shader fields
        // (falloff cone, greyscale palette, FO4+/FO76 companion
        // textures, lighting influence, etc.) so downstream consumers
        // can route them. `None` for BSLightingShaderProperty and for
        // shapes with no shader. See #346 / audit S4-02.
        effect_shader: find_effect_shader_bs(scene, shape),
        // BsTriShape's BSLightingShaderProperty does carry a
        // shader_type — capture it directly off the linked shader so
        // Skyrim+ characters get the right `material_kind` (#344).
        // Falls back to 0 (Default lit) when the shape has no shader
        // or the shader isn't a BSLightingShaderProperty.
        material_kind: shape
            .shader_property_ref
            .index()
            .and_then(|i| scene.get_as::<BSLightingShaderProperty>(i))
            .map(|s| s.shader_type as u8)
            .unwrap_or(0),
        // #430 — populated from `capture_shader_type_fields` above when
        // the shape has a BSLightingShaderProperty backing, else default.
        shader_type_fields,
        // BSShaderNoLightingProperty is an FO3/FNV-era property and
        // doesn't bind to BsTriShape (Skyrim+). Always None on this
        // path. See #451.
        no_lighting_falloff: None,
        flags: shape.av.flags,
    })
}

/// Extract a BsTriShape with local transform (for hierarchical import).
pub(super) fn extract_bs_tri_shape_local(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<ImportedMesh> {
    extract_bs_tri_shape(scene, shape, &shape.av.transform)
}

/// Resolve the double-sided flag for a BsTriShape from either of the
/// two shader-property variants Skyrim+ binds. Both
/// `BSLightingShaderProperty` (the common case for static / clutter /
/// actor meshes) and `BSEffectShaderProperty` (Skyrim+ VFX surfaces:
/// force fields, magic auras, glow shells, Dwemer steam) use bit
/// `0x10` of `shader_flags_2` for the same double-sided semantics.
///
/// Pre-#128 only the BSLightingShaderProperty branch was checked, so
/// effect-shader-backed meshes silently dropped the flag and rendered
/// backface-culled glow geometry that should have been visible from
/// either side.
fn bs_tri_shape_two_sided(scene: &NifScene, shape: &BsTriShape) -> bool {
    let Some(idx) = shape.shader_property_ref.index() else {
        return false;
    };
    if let Some(s) = scene.get_as::<BSLightingShaderProperty>(idx) {
        return s.shader_flags_2 & 0x10 != 0;
    }
    if let Some(s) = scene.get_as::<BSEffectShaderProperty>(idx) {
        return s.shader_flags_2 & 0x10 != 0;
    }
    false
}

/// Find texture path for BsTriShape via its shader_property_ref.
pub(super) fn find_texture_path_bs_tri_shape(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<String> {
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

/// Find BGSM/BGEM material path for BsTriShape via shader property name.
///
/// FO4+ / FO76 / Starfield bind real material data to external material
/// files referenced by the shader block's `net.name`:
/// - **BSLightingShaderProperty** → `.bgsm` (opaque surfaces)
/// - **BSEffectShaderProperty** → `.bgem` (effect surfaces: weapon energy
///   effects, magic spells, steam vents, electrical arcs, glow decals)
///
/// Before #434 this only inspected BSLightingShaderProperty, so every
/// effect-shader surface silently lost its BGEM pointer even though
/// `find_effect_shader_bs` already captured the rich effect fields.
/// `.bgsm`/`.bgem` suffixes are both accepted on either shader variant
/// since the game treats the extension as advisory rather than gating.
fn find_material_path_bs_tri_shape(scene: &NifScene, shape: &BsTriShape) -> Option<String> {
    let idx = shape.shader_property_ref.index()?;
    if let Some(lit) = scene.get_as::<BSLightingShaderProperty>(idx) {
        if let Some(path) = material_path_from_name(lit.net.name.as_deref()) {
            return Some(path);
        }
    }
    if let Some(eff) = scene.get_as::<BSEffectShaderProperty>(idx) {
        if let Some(path) = material_path_from_name(eff.net.name.as_deref()) {
            return Some(path);
        }
    }
    None
}

/// Return `Some(name)` when `name` is a `.bgsm`/`.bgem` material file
/// path, else `None`. Shared between the BsTriShape and NiTriShape
/// material-path extractors so both report material pointers consistently.
pub(super) fn material_path_from_name(name: Option<&str>) -> Option<String> {
    let name = name?;
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".bgsm") || lower.ends_with(".bgem") {
        Some(name.to_string())
    } else {
        None
    }
}

// ── Skinning extraction (issue #151) ──────────────────────────────────

/// Extract `ImportedSkin` for a NiTriShape via `skin_instance_ref`.
///
/// Follows:
///   NiTriShape.skin_instance_ref → NiSkinInstance (or BSDismemberSkinInstance)
///     → NiSkinData.bones[] (bind transforms + sparse vertex weights)
///     → per-bone NiNode refs (names for bone lookup)
///
/// Converts the sparse per-bone weight lists to dense per-vertex
/// `[u8; 4]` indices + `[f32; 4]` weights by keeping the 4 highest
/// contributions per vertex and re-normalizing so the weights sum to 1.
/// Vertices with no bone contribution get weight `[1, 0, 0, 0]` bound
/// to bone 0 (safer than all-zero weights which would collapse the
/// vertex to the origin during skinning).
pub(super) fn extract_skin_ni_tri_shape(
    scene: &NifScene,
    shape: &NiTriShape,
    num_vertices: usize,
) -> Option<ImportedSkin> {
    let skin_idx = shape.skin_instance_ref.index()?;

    // Accept either NiSkinInstance or BSDismemberSkinInstance (the
    // Bethesda extension with body-part flags — we only need the base).
    let (bone_refs, skeleton_root_ref, data_ref) =
        if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
            (
                inst.bone_refs.as_slice(),
                inst.skeleton_root_ref,
                inst.data_ref,
            )
        } else if let Some(inst) = scene.get_as::<BsDismemberSkinInstance>(skin_idx) {
            (
                inst.base.bone_refs.as_slice(),
                inst.base.skeleton_root_ref,
                inst.base.data_ref,
            )
        } else {
            return None;
        };

    let data = scene.get_as::<NiSkinData>(data_ref.index()?)?;
    if data.bones.len() != bone_refs.len() {
        log::debug!(
            "NiSkinData bone count ({}) != NiSkinInstance bone_refs count ({})",
            data.bones.len(),
            bone_refs.len(),
        );
        return None;
    }

    // Resolve bone names (the interpolator refers to bones by index
    // into this vec, so the order must match NiSkinInstance.bone_refs).
    let bones = build_imported_bones(scene, bone_refs, data)?;
    let skeleton_root = resolve_node_name(scene, skeleton_root_ref);

    // Build dense per-vertex weight tables.
    let (vertex_bone_indices, vertex_bone_weights) = densify_sparse_weights(num_vertices, data);

    Some(ImportedSkin {
        bones,
        skeleton_root,
        vertex_bone_indices,
        vertex_bone_weights,
    })
}

/// Extract `ImportedSkin` for a BSTriShape via `skin_ref`. Walks the
/// skin instance for bone list + bind-inverse transforms, then copies
/// the parsed per-vertex weights + indices from the packed vertex
/// buffer (VF_SKINNED, issue #177).
///
/// Handles both:
///   - NiSkinInstance (Skyrim LE BSTriShape) via NiSkinData
///   - BSSkin::Instance (Skyrim SE / FO4+) via BSSkin::BoneData
pub(super) fn extract_skin_bs_tri_shape(
    scene: &NifScene,
    shape: &BsTriShape,
) -> Option<ImportedSkin> {
    let skin_idx = shape.skin_ref.index()?;

    // Per-vertex weights and indices come from the BSTriShape vertex
    // buffer (VF_SKINNED) — already decoded at parse time (#177). We
    // just clone them through to ImportedSkin. If the vertex buffer
    // lacks the VF_SKINNED bit these will be empty, and downstream
    // should treat the mesh as rigid.
    let vertex_bone_indices = shape.bone_indices.clone();
    let vertex_bone_weights = shape.bone_weights.clone();

    // Skyrim LE path: NiSkinInstance + NiSkinData (bone list + bind transforms).
    // Borrow bone_refs instead of cloning — they're only iterated. #279 D5-11.
    let (bone_refs_slice, skeleton_root_ref, data_ref) =
        if let Some(inst) = scene.get_as::<NiSkinInstance>(skin_idx) {
            (
                inst.bone_refs.as_slice(),
                inst.skeleton_root_ref,
                inst.data_ref,
            )
        } else if let Some(inst) = scene.get_as::<BsDismemberSkinInstance>(skin_idx) {
            (
                inst.base.bone_refs.as_slice(),
                inst.base.skeleton_root_ref,
                inst.base.data_ref,
            )
        } else {
            (&[] as &[_], BlockRef::NULL, BlockRef::NULL)
        };
    if !bone_refs_slice.is_empty() {
        let data = scene.get_as::<NiSkinData>(data_ref.index()?)?;
        if data.bones.len() != bone_refs_slice.len() {
            return None;
        }
        let bones = build_imported_bones(scene, bone_refs_slice, data)?;
        let skeleton_root = resolve_node_name(scene, skeleton_root_ref);
        return Some(ImportedSkin {
            bones,
            skeleton_root,
            vertex_bone_indices,
            vertex_bone_weights,
        });
    }

    // Skyrim SE / FO4+ path: BSSkin::Instance + BSSkin::BoneData.
    if let Some(inst) = scene.get_as::<BsSkinInstance>(skin_idx) {
        let bone_data = scene.get_as::<BsSkinBoneData>(inst.bone_data_ref.index()?)?;
        if bone_data.bones.len() != inst.bone_refs.len() {
            return None;
        }
        let mut bones = Vec::with_capacity(inst.bone_refs.len());
        for (i, bone_ref) in inst.bone_refs.iter().enumerate() {
            let name = resolve_node_name(scene, *bone_ref)
                .unwrap_or_else(|| Arc::from(format!("Bone{}", i)));
            let bt = &bone_data.bones[i];
            bones.push(ImportedBone {
                name,
                bind_inverse: bs_bone_to_inverse_matrix(bt),
                bounding_sphere: bt.bounding_sphere,
            });
        }
        let skeleton_root = resolve_node_name(scene, inst.skeleton_root_ref);
        return Some(ImportedSkin {
            bones,
            skeleton_root,
            vertex_bone_indices,
            vertex_bone_weights,
        });
    }

    None
}

/// Build `ImportedBone`s from a NiSkinInstance bone list and NiSkinData
/// bone entries. The two inputs must have matching lengths (checked by
/// the caller). Applies Z-up → Y-up conversion to each bind transform.
fn build_imported_bones(
    scene: &NifScene,
    bone_refs: &[BlockRef],
    data: &NiSkinData,
) -> Option<Vec<ImportedBone>> {
    let mut bones = Vec::with_capacity(bone_refs.len());
    for (i, bone_ref) in bone_refs.iter().enumerate() {
        let name =
            resolve_node_name(scene, *bone_ref).unwrap_or_else(|| Arc::from(format!("Bone{}", i)));
        let bone = &data.bones[i];
        bones.push(ImportedBone {
            name,
            bind_inverse: ni_transform_to_yup_matrix(&bone.skin_transform),
            bounding_sphere: bone.bounding_sphere,
        });
    }
    Some(bones)
}

/// Resolve a BlockRef pointing to a NiNode to the node's name.
/// Returns `None` if the ref is null, the block isn't a NiNode, or the
/// node has no name.
fn resolve_node_name(scene: &NifScene, node_ref: BlockRef) -> Option<Arc<str>> {
    let idx = node_ref.index()?;
    let node = scene.get_as::<NiNode>(idx)?;
    node.av.net.name.clone()
}

/// Convert a NiTransform to a column-major 4x4 matrix with the Y-up
/// basis change applied. NiSkinData stores the bind-inverse already —
/// we just need to reorder rows/columns for glam's column-major layout
/// and convert Gamebryo Z-up to engine Y-up (90° rotation around X).
fn ni_transform_to_yup_matrix(t: &NiTransform) -> [[f32; 4]; 4] {
    // Z-up → Y-up basis change matrix C (row vectors for NiMatrix3 style):
    //   C = [[1, 0, 0], [0, 0, 1], [0, -1, 0]]
    // For a NiTransform (R, t, s) in Z-up, the Y-up equivalent is:
    //   R' = C * R * C^T
    //   t' = C * t
    //   s  = s
    let r = &t.rotation.rows;
    let tx = t.translation.x;
    let ty = t.translation.y;
    let tz = t.translation.z;

    // C * R: row-major multiply. C has rows [1,0,0], [0,0,1], [0,-1,0].
    //   cr[0][j] = r[0][j]
    //   cr[1][j] = r[2][j]
    //   cr[2][j] = -r[1][j]
    let cr = [
        [r[0][0], r[0][1], r[0][2]],
        [r[2][0], r[2][1], r[2][2]],
        [-r[1][0], -r[1][1], -r[1][2]],
    ];
    // (C*R) * C^T: columns of C^T are the rows of C.
    //   cr_ct[i][0] = cr[i][0]
    //   cr_ct[i][1] = cr[i][2]
    //   cr_ct[i][2] = -cr[i][1]
    let rr = [
        [cr[0][0], cr[0][2], -cr[0][1]],
        [cr[1][0], cr[1][2], -cr[1][1]],
        [cr[2][0], cr[2][2], -cr[2][1]],
    ];
    // C * t
    let tt = [tx, tz, -ty];

    // Pack into column-major 4x4 with uniform scale baked in.
    let s = t.scale;
    [
        [rr[0][0] * s, rr[1][0] * s, rr[2][0] * s, 0.0],
        [rr[0][1] * s, rr[1][1] * s, rr[2][1] * s, 0.0],
        [rr[0][2] * s, rr[1][2] * s, rr[2][2] * s, 0.0],
        [tt[0], tt[1], tt[2], 1.0],
    ]
}

/// Build a bind-inverse matrix from a BSSkin::BoneData bone entry.
/// The row-major 3x3 rotation + translation + scale layout mirrors
/// NiTransform, so we reuse the same conversion.
fn bs_bone_to_inverse_matrix(b: &crate::blocks::skin::BsSkinBoneTrans) -> [[f32; 4]; 4] {
    let t = NiTransform {
        rotation: crate::types::NiMatrix3 { rows: b.rotation },
        translation: NiPoint3 {
            x: b.translation[0],
            y: b.translation[1],
            z: b.translation[2],
        },
        scale: b.scale,
    };
    ni_transform_to_yup_matrix(&t)
}

/// Densify sparse per-bone weight lists to per-vertex `[bone_idx; 4]` +
/// `[weight; 4]` arrays. Keeps the 4 highest contributions per vertex
/// and re-normalizes so the weights sum to 1.0.
///
/// Vertices with no bone contribution get `([0, 0, 0, 0], [1, 0, 0, 0])`
/// which binds them to bone 0 with full weight — safer than all-zeros
/// which would collapse to the origin during matrix palette skinning.
fn densify_sparse_weights(num_vertices: usize, data: &NiSkinData) -> (Vec<[u8; 4]>, Vec<[f32; 4]>) {
    // Per-vertex sorted top-4 contributions. Initialized to (255, 0.0)
    // so missing slots are obviously invalid until we replace them.
    let mut per_vertex: Vec<[(u8, f32); 4]> = vec![[(255u8, 0.0f32); 4]; num_vertices];

    for (bone_idx, bone) in data.bones.iter().enumerate() {
        // NiSkinData supports more than 256 bones in theory, but the
        // hardware palette limits us to u8. Skip any bone index that
        // can't be represented.
        let bone_u8 = if bone_idx < 256 {
            bone_idx as u8
        } else {
            continue;
        };
        for vw in &bone.vertex_weights {
            let v = vw.vertex_index as usize;
            if v >= num_vertices {
                continue;
            }
            let slots = &mut per_vertex[v];

            // Find the slot with the smallest current weight; replace
            // it if our weight is larger. This runs O(4) per weight
            // entry which is negligible for typical meshes.
            let (min_slot, min_weight) = slots
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    a.1 .1
                        .partial_cmp(&b.1 .1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, s)| (i, s.1))
                .unwrap_or((0, 0.0));

            if vw.weight > min_weight {
                slots[min_slot] = (bone_u8, vw.weight);
            }
        }
    }

    let mut vertex_bone_indices = Vec::with_capacity(num_vertices);
    let mut vertex_bone_weights = Vec::with_capacity(num_vertices);

    for slots in &per_vertex {
        let total: f32 = slots
            .iter()
            .filter(|(b, _)| *b != 255)
            .map(|(_, w)| *w)
            .sum();

        if total <= f32::EPSILON {
            // No contribution — bind to bone 0 so matrix palette
            // skinning doesn't collapse the vertex to the origin.
            vertex_bone_indices.push([0, 0, 0, 0]);
            vertex_bone_weights.push([1.0, 0.0, 0.0, 0.0]);
            continue;
        }

        let inv = 1.0 / total;
        let mut idx = [0u8; 4];
        let mut w = [0.0f32; 4];
        for (i, (b, weight)) in slots.iter().enumerate() {
            if *b != 255 {
                idx[i] = *b;
                w[i] = *weight * inv;
            }
        }
        vertex_bone_indices.push(idx);
        vertex_bone_weights.push(w);
    }

    (vertex_bone_indices, vertex_bone_weights)
}

#[cfg(test)]
mod skin_tests {
    use super::*;
    use crate::blocks::skin::{BoneData, BoneVertWeight};
    use crate::types::NiMatrix3;

    fn identity_transform() -> NiTransform {
        NiTransform {
            rotation: NiMatrix3 {
                rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            },
            translation: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            scale: 1.0,
        }
    }

    fn bone(weights: Vec<(u16, f32)>) -> BoneData {
        BoneData {
            skin_transform: identity_transform(),
            bounding_sphere: [0.0, 0.0, 0.0, 0.0],
            vertex_weights: weights
                .into_iter()
                .map(|(vertex_index, weight)| BoneVertWeight {
                    vertex_index,
                    weight,
                })
                .collect(),
        }
    }

    #[test]
    fn densify_empty_data_gives_default_binding() {
        // No bones at all — every vertex should fall back to bone 0 weight 1.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: Vec::new(),
        };
        let (indices, weights) = densify_sparse_weights(3, &data);
        assert_eq!(indices.len(), 3);
        assert_eq!(weights.len(), 3);
        for i in 0..3 {
            assert_eq!(indices[i], [0, 0, 0, 0]);
            assert_eq!(weights[i], [1.0, 0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn densify_single_bone_full_weight() {
        // Bone 0 binds vertex 0 with weight 1.0, vertex 1 not bound.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: vec![bone(vec![(0, 1.0)])],
        };
        let (indices, weights) = densify_sparse_weights(2, &data);
        assert_eq!(indices[0], [0, 0, 0, 0]);
        assert!((weights[0][0] - 1.0).abs() < 1e-6);
        // Vertex 1 falls back to bone 0 weight 1.
        assert_eq!(indices[1], [0, 0, 0, 0]);
        assert_eq!(weights[1], [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn densify_two_bones_normalized() {
        // Vertex 0 gets half-and-half from bones 0 and 1.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: vec![bone(vec![(0, 0.5)]), bone(vec![(0, 0.5)])],
        };
        let (indices, weights) = densify_sparse_weights(1, &data);
        // Two slots used, two unused. Weights sum to 1.
        let total: f32 = weights[0].iter().sum();
        assert!((total - 1.0).abs() < 1e-5);
        // Exactly two distinct bones present (0 and 1). Order inside
        // the 4-slot tuple isn't guaranteed by the algorithm.
        let mut seen: Vec<u8> = indices[0]
            .iter()
            .zip(weights[0].iter())
            .filter(|(_, w)| **w > 0.0)
            .map(|(b, _)| *b)
            .collect();
        seen.sort();
        assert_eq!(seen, vec![0, 1]);
    }

    #[test]
    fn densify_more_than_four_bones_keeps_top_four_by_weight() {
        // Five bones all bind vertex 0 with increasing weight. The top
        // 4 (weights 0.2, 0.3, 0.4, 0.5) should survive; the smallest
        // (0.1) should be dropped. After normalization the kept weights
        // sum to 1.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: vec![
                bone(vec![(0, 0.1)]), // bone 0 — should be dropped
                bone(vec![(0, 0.2)]),
                bone(vec![(0, 0.3)]),
                bone(vec![(0, 0.4)]),
                bone(vec![(0, 0.5)]),
            ],
        };
        let (indices, weights) = densify_sparse_weights(1, &data);

        let total: f32 = weights[0].iter().sum();
        assert!((total - 1.0).abs() < 1e-5, "weights should sum to 1");

        let mut present: Vec<(u8, f32)> = indices[0]
            .iter()
            .zip(weights[0].iter())
            .filter(|(_, w)| **w > 0.0)
            .map(|(b, w)| (*b, *w))
            .collect();
        assert_eq!(present.len(), 4, "should keep exactly 4 bones");
        present.sort_by_key(|(b, _)| *b);

        // Dropped bone 0 (weight 0.1); kept bones 1..=4.
        let bones: Vec<u8> = present.iter().map(|(b, _)| *b).collect();
        assert_eq!(bones, vec![1, 2, 3, 4]);

        // Original sum = 0.2 + 0.3 + 0.4 + 0.5 = 1.4; after normalizing
        // each weight becomes w / 1.4.
        assert!((present[0].1 - 0.2 / 1.4).abs() < 1e-5);
        assert!((present[3].1 - 0.5 / 1.4).abs() < 1e-5);
    }

    #[test]
    fn ni_transform_to_yup_matrix_identity() {
        let t = identity_transform();
        let m = ni_transform_to_yup_matrix(&t);
        // Identity rotation through C * I * C^T = I, identity translation, scale 1.
        // Column 0 = (1,0,0,0), col 1 = (0,1,0,0), col 2 = (0,0,1,0), col 3 = (0,0,0,1)
        assert!((m[0][0] - 1.0).abs() < 1e-6);
        assert!((m[1][1] - 1.0).abs() < 1e-6);
        assert!((m[2][2] - 1.0).abs() < 1e-6);
        assert!((m[3][3] - 1.0).abs() < 1e-6);
        // Off-diagonals zero.
        assert!(m[0][1].abs() < 1e-6);
        assert!(m[1][0].abs() < 1e-6);
    }

    #[test]
    fn ni_transform_to_yup_matrix_translation_only() {
        // Gamebryo Z-up translation (1, 2, 3) → Y-up (1, 3, -2).
        let t = NiTransform {
            rotation: NiMatrix3 {
                rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            },
            translation: NiPoint3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            scale: 1.0,
        };
        let m = ni_transform_to_yup_matrix(&t);
        // Column 3 holds the translation in column-major storage.
        assert!((m[3][0] - 1.0).abs() < 1e-6);
        assert!((m[3][1] - 3.0).abs() < 1e-6);
        assert!((m[3][2] + 2.0).abs() < 1e-6);
    }

    #[test]
    fn ni_transform_to_yup_matrix_scale_baked_in() {
        let mut t = identity_transform();
        t.scale = 2.5;
        let m = ni_transform_to_yup_matrix(&t);
        // Diagonal should be scale.
        assert!((m[0][0] - 2.5).abs() < 1e-6);
        assert!((m[1][1] - 2.5).abs() < 1e-6);
        assert!((m[2][2] - 2.5).abs() < 1e-6);
        // W column still identity.
        assert!((m[3][3] - 1.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod two_sided_lookup_tests {
    //! Regression tests for issue #128 — `bs_tri_shape_two_sided` must
    //! check `BSEffectShaderProperty` in addition to
    //! `BSLightingShaderProperty`. Skyrim+ VFX surfaces (force fields,
    //! glow shells, magic auras) bind the effect-shader variant and
    //! use the same `shader_flags_2 & 0x10` semantics; pre-fix the
    //! lookup only tried `BSLightingShaderProperty`, so every
    //! effect-shader-backed mesh silently dropped the flag and
    //! rendered backface-culled.
    use super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::shader::BSEffectShaderProperty;
    use crate::scene::NifScene;
    use crate::types::{BlockRef, NiPoint3};

    fn empty_net() -> NiObjectNETData {
        NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        }
    }

    /// Build a minimal `BsTriShape` whose `shader_property_ref` points
    /// at the given block index.
    fn shape_with_shader(idx: u32) -> BsTriShape {
        BsTriShape {
            av: NiAVObjectData {
                net: empty_net(),
                flags: 0,
                transform: crate::types::NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            center: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            radius: 0.0,
            skin_ref: BlockRef::NULL,
            shader_property_ref: BlockRef(idx),
            alpha_property_ref: BlockRef::NULL,
            vertex_desc: 0,
            num_triangles: 0,
            num_vertices: 0,
            vertices: Vec::new(),
            uvs: Vec::new(),
            normals: Vec::new(),
            vertex_colors: Vec::new(),
            triangles: Vec::new(),
            bone_weights: Vec::new(),
            bone_indices: Vec::new(),
        }
    }

    /// Minimal `BSEffectShaderProperty` with only the bit under test
    /// set; everything else stays at safe defaults.
    fn effect_shader(flags2: u32) -> BSEffectShaderProperty {
        BSEffectShaderProperty {
            net: empty_net(),
            material_reference: false,
            shader_flags_1: 0,
            shader_flags_2: flags2,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            source_texture: String::new(),
            texture_clamp_mode: 3,
            lighting_influence: 0,
            env_map_min_lod: 0,
            falloff_start_angle: 1.0,
            falloff_stop_angle: 1.0,
            falloff_start_opacity: 0.0,
            falloff_stop_opacity: 0.0,
            refraction_power: 0.0,
            base_color: [0.0; 4],
            base_color_scale: 1.0,
            soft_falloff_depth: 0.0,
            greyscale_texture: String::new(),
            env_map_texture: String::new(),
            normal_texture: String::new(),
            env_mask_texture: String::new(),
            env_map_scale: 1.0,
            reflectance_texture: String::new(),
            lighting_texture: String::new(),
            emittance_color: [0.0; 3],
            emit_gradient_texture: String::new(),
            luminance: None,
        }
    }

    /// Regression: #128 — pre-fix, this test failed because the
    /// `BSLightingShaderProperty::get_as` lookup returned None and
    /// the function fell through to `false`, silently dropping the
    /// double-sided flag on every Skyrim+ VFX surface (force fields,
    /// glow shells, magic auras, Dwemer steam).
    #[test]
    fn two_sided_via_bs_effect_shader_property() {
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(effect_shader(0x10)));
        let shape = shape_with_shader(0);
        assert!(bs_tri_shape_two_sided(&scene, &shape));
    }

    #[test]
    fn not_two_sided_via_bs_effect_shader_without_flag() {
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(effect_shader(0x00)));
        let shape = shape_with_shader(0);
        assert!(!bs_tri_shape_two_sided(&scene, &shape));
    }

    #[test]
    fn null_shader_ref_yields_single_sided() {
        let scene = NifScene::default();
        let mut shape = shape_with_shader(0);
        shape.shader_property_ref = BlockRef::NULL;
        assert!(!bs_tri_shape_two_sided(&scene, &shape));
    }

    #[test]
    fn shader_ref_pointing_at_unrelated_block_yields_single_sided() {
        // Sibling check — a `shader_property_ref` index that points
        // at a block which is neither of the two shader-property
        // variants must return `false` (no spurious flag from
        // mistaking some other block as a shader).
        let mut scene = NifScene::default();
        // Push a non-shader block. Use a NiNode to keep the test
        // self-contained; NiNode is the universal scene-graph block.
        scene.blocks.push(Box::new(crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: empty_net(),
                flags: 0,
                transform: crate::types::NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        }));
        let shape = shape_with_shader(0);
        assert!(!bs_tri_shape_two_sided(&scene, &shape));
    }

    /// Regression: #346 — BsTriShape import must read material fields
    /// (emissive, UV transform, alpha, env-map scale, FO4+ normal map)
    /// from `BSEffectShaderProperty`, mirror the `find_decal_bs` decal
    /// check across both shader variants, and populate the
    /// `effect_shader` capture struct. Pre-fix every Skyrim+ effect-
    /// shader-backed mesh fell back to defaults — magic FX, particle-on-
    /// mesh emissives, blood-decals all rendered untransformed, opaque,
    /// and without the emissive multiplier.
    fn effect_shader_with_payload() -> BSEffectShaderProperty {
        let mut s = effect_shader(0);
        s.uv_offset = [0.25, 0.5];
        s.uv_scale = [2.0, 4.0];
        s.base_color = [0.7, 0.8, 0.9, 0.5]; // alpha = 0.5
        s.base_color_scale = 3.5;
        s.env_map_scale = 0.75;
        s.normal_texture = "fx/glow_n.dds".to_string();
        s.greyscale_texture = "fx/fire_palette.dds".to_string();
        s
    }

    #[test]
    fn extract_bs_tri_shape_pulls_effect_shader_emissive_uv_alpha_normal() {
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(effect_shader_with_payload()));
        let mut shape = shape_with_shader(0);
        // One degenerate triangle so `extract_bs_tri_shape` returns Some.
        shape.vertices.push(NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        });
        shape.vertices.push(NiPoint3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        });
        shape.vertices.push(NiPoint3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        });
        shape.triangles.push([0, 1, 2]);
        shape.num_vertices = 3;
        shape.num_triangles = 1;

        let mesh =
            extract_bs_tri_shape(&scene, &shape, &crate::types::NiTransform::default()).unwrap();
        assert_eq!(mesh.emissive_color, [0.7, 0.8, 0.9]);
        assert!((mesh.emissive_mult - 3.5).abs() < 1e-6);
        assert_eq!(mesh.uv_offset, [0.25, 0.5]);
        assert_eq!(mesh.uv_scale, [2.0, 4.0]);
        assert!((mesh.mat_alpha - 0.5).abs() < 1e-6);
        assert!((mesh.env_map_scale - 0.75).abs() < 1e-6);
        assert_eq!(mesh.normal_map.as_deref(), Some("fx/glow_n.dds"));
        let fx = mesh.effect_shader.expect("effect_shader should populate");
        assert_eq!(fx.greyscale_texture.as_deref(), Some("fx/fire_palette.dds"));
        assert!((fx.env_map_scale - 0.75).abs() < 1e-6);
    }

    #[test]
    fn find_decal_bs_via_effect_shader_alpha_decal_flag() {
        // ALPHA_DECAL_F2 = 0x00200000 in shader_flags_2 — used by Skyrim+
        // blood splats and similar overlay effects bound to an
        // BSEffectShaderProperty. Pre-#346 this fell through to false
        // and the decal got no z-bias, z-fighting against its host.
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(effect_shader(0x0020_0000)));
        let shape = shape_with_shader(0);
        assert!(super::find_decal_bs(&scene, &shape));
    }

    #[test]
    fn find_decal_bs_via_effect_shader_decal_single_pass() {
        // DECAL_SINGLE_PASS = 0x04000000 in shader_flags_1 — universal
        // decal flag the engine honors regardless of which shader
        // variant the artist bound. Mirror across BSLighting/BSEffect.
        let mut scene = NifScene::default();
        let mut shader = effect_shader(0);
        shader.shader_flags_1 = 0x0400_0000;
        scene.blocks.push(Box::new(shader));
        let shape = shape_with_shader(0);
        assert!(super::find_decal_bs(&scene, &shape));
    }
}

/// Regression tests for issue #430 — the BsTriShape import path must
/// capture `BSLightingShaderProperty.shader_type_data` payload onto
/// `ImportedMesh.shader_type_fields`. Pre-fix the match collapsed every
/// non-`EnvironmentMap` variant to `1.0` and silently dropped SkinTint /
/// HairTint / EyeEnvmap / ParallaxOcc / MultiLayerParallax / SparkleSnow
/// payloads on Skyrim+ / FO4 / FO76 / Starfield characters.
#[cfg(test)]
mod shader_type_fields_tests {
    use super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::shader::{BSLightingShaderProperty, ShaderTypeData};
    use crate::scene::NifScene;
    use crate::types::{BlockRef, NiPoint3, NiTransform};

    fn empty_net() -> NiObjectNETData {
        NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        }
    }

    fn lighting_shader_with(shader_type: u32, data: ShaderTypeData) -> BSLightingShaderProperty {
        BSLightingShaderProperty {
            shader_type,
            net: empty_net(),
            material_reference: false,
            shader_flags_1: 0,
            shader_flags_2: 0,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            texture_set_ref: BlockRef::NULL,
            emissive_color: [0.0; 3],
            emissive_multiple: 1.0,
            texture_clamp_mode: 3,
            alpha: 1.0,
            refraction_strength: 0.0,
            glossiness: 80.0,
            specular_color: [1.0; 3],
            specular_strength: 1.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale: 1.0,
            fresnel_power: 5.0,
            wetness: None,
            luminance: None,
            do_translucency: false,
            translucency: None,
            texture_arrays: Vec::new(),
            shader_type_data: data,
        }
    }

    /// Minimal renderable `BsTriShape` (one triangle, three vertices) bound
    /// to a shader block at index `shader_idx` on the scene.
    fn renderable_shape(shader_idx: u32) -> BsTriShape {
        BsTriShape {
            av: NiAVObjectData {
                net: empty_net(),
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            center: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            radius: 0.0,
            skin_ref: BlockRef::NULL,
            shader_property_ref: BlockRef(shader_idx),
            alpha_property_ref: BlockRef::NULL,
            vertex_desc: 0,
            num_triangles: 1,
            num_vertices: 3,
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
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            normals: Vec::new(),
            vertex_colors: Vec::new(),
            triangles: vec![[0, 1, 2]],
            bone_weights: Vec::new(),
            bone_indices: Vec::new(),
        }
    }

    #[test]
    fn bs_tri_shape_captures_skin_tint_color() {
        // Skyrim SE NPC head — shader_type = 5 (SkinTint). Pre-#430 the
        // match arm dropped the color silently.
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(lighting_shader_with(
            5,
            ShaderTypeData::SkinTint {
                skin_tint_color: [0.87, 0.65, 0.54],
            },
        )));
        let shape = renderable_shape(0);
        let imported = extract_bs_tri_shape(&scene, &shape, &NiTransform::default())
            .expect("synthetic shape should import");
        assert_eq!(imported.material_kind, 5);
        assert_eq!(
            imported.shader_type_fields.skin_tint_color,
            Some([0.87, 0.65, 0.54])
        );
        assert_eq!(imported.shader_type_fields.skin_tint_alpha, None);
    }

    #[test]
    fn bs_tri_shape_captures_hair_tint_color() {
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(lighting_shader_with(
            6,
            ShaderTypeData::HairTint {
                hair_tint_color: [0.3, 0.15, 0.05],
            },
        )));
        let shape = renderable_shape(0);
        let imported = extract_bs_tri_shape(&scene, &shape, &NiTransform::default()).unwrap();
        assert_eq!(imported.material_kind, 6);
        assert_eq!(
            imported.shader_type_fields.hair_tint_color,
            Some([0.3, 0.15, 0.05])
        );
    }

    #[test]
    fn bs_tri_shape_captures_eye_envmap_centers() {
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(lighting_shader_with(
            16,
            ShaderTypeData::EyeEnvmap {
                eye_cubemap_scale: 1.5,
                left_eye_reflection_center: [0.1, 0.2, 0.3],
                right_eye_reflection_center: [0.4, 0.5, 0.6],
            },
        )));
        let shape = renderable_shape(0);
        let imported = extract_bs_tri_shape(&scene, &shape, &NiTransform::default()).unwrap();
        assert_eq!(imported.shader_type_fields.eye_cubemap_scale, Some(1.5));
        assert_eq!(
            imported.shader_type_fields.eye_left_reflection_center,
            Some([0.1, 0.2, 0.3])
        );
        assert_eq!(
            imported.shader_type_fields.eye_right_reflection_center,
            Some([0.4, 0.5, 0.6])
        );
    }

    #[test]
    fn bs_tri_shape_fo76_skin_tint_splits_rgba() {
        // FO76 BSShaderType155::SkinTint — the 4-wide variant. Must split
        // into rgb + alpha exactly the way MaterialInfo's copy does.
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(lighting_shader_with(
            4,
            ShaderTypeData::Fo76SkinTint {
                skin_tint_color: [0.9, 0.7, 0.55, 0.25],
            },
        )));
        let shape = renderable_shape(0);
        let imported = extract_bs_tri_shape(&scene, &shape, &NiTransform::default()).unwrap();
        assert_eq!(
            imported.shader_type_fields.skin_tint_color,
            Some([0.9, 0.7, 0.55])
        );
        assert_eq!(imported.shader_type_fields.skin_tint_alpha, Some(0.25));
    }

    #[test]
    fn bs_tri_shape_environment_map_routes_scale_not_fields() {
        // EnvironmentMap lives on `env_map_scale`, not `shader_type_fields`.
        // The default / no-variant-match ShaderTypeFields should stay clean.
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(lighting_shader_with(
            1,
            ShaderTypeData::EnvironmentMap { env_map_scale: 2.5 },
        )));
        let shape = renderable_shape(0);
        let imported = extract_bs_tri_shape(&scene, &shape, &NiTransform::default()).unwrap();
        assert_eq!(imported.env_map_scale, 2.5);
        assert_eq!(
            imported.shader_type_fields,
            super::super::material::ShaderTypeFields::default()
        );
    }

    #[test]
    fn bs_tri_shape_without_shader_has_default_fields() {
        let scene = NifScene::default();
        let mut shape = renderable_shape(0);
        shape.shader_property_ref = BlockRef::NULL;
        let imported = extract_bs_tri_shape(&scene, &shape, &NiTransform::default()).unwrap();
        assert_eq!(imported.material_kind, 0);
        assert_eq!(
            imported.shader_type_fields,
            super::super::material::ShaderTypeFields::default()
        );
    }
}

/// Regression tests for issue #434 — `find_material_path_bs_tri_shape`
/// must pick up the `.bgem` path from a `BSEffectShaderProperty` bound to
/// the shape, not just from a `BSLightingShaderProperty`. FO4+/FO76/
/// Starfield weapon energy effects, magic surfaces, and steam vents all
/// bind the effect-shader variant with the material pointer in
/// `net.name`.
#[cfg(test)]
mod material_path_capture_tests {
    use super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::shader::{BSEffectShaderProperty, BSLightingShaderProperty, ShaderTypeData};
    use crate::scene::NifScene;
    use crate::types::{BlockRef, NiPoint3, NiTransform};
    use std::sync::Arc;

    fn net_with_name(name: &str) -> NiObjectNETData {
        NiObjectNETData {
            name: Some(Arc::from(name)),
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        }
    }

    fn minimal_lighting_shader_named(name: &str) -> BSLightingShaderProperty {
        BSLightingShaderProperty {
            shader_type: 0,
            net: net_with_name(name),
            material_reference: true,
            shader_flags_1: 0,
            shader_flags_2: 0,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            texture_set_ref: BlockRef::NULL,
            emissive_color: [0.0; 3],
            emissive_multiple: 1.0,
            texture_clamp_mode: 3,
            alpha: 1.0,
            refraction_strength: 0.0,
            glossiness: 80.0,
            specular_color: [1.0; 3],
            specular_strength: 1.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale: 1.0,
            fresnel_power: 5.0,
            wetness: None,
            luminance: None,
            do_translucency: false,
            translucency: None,
            texture_arrays: Vec::new(),
            shader_type_data: ShaderTypeData::None,
        }
    }

    fn minimal_effect_shader_named(name: &str) -> BSEffectShaderProperty {
        BSEffectShaderProperty {
            net: net_with_name(name),
            material_reference: true,
            shader_flags_1: 0,
            shader_flags_2: 0,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            source_texture: String::new(),
            texture_clamp_mode: 3,
            lighting_influence: 0,
            env_map_min_lod: 0,
            falloff_start_angle: 1.0,
            falloff_stop_angle: 1.0,
            falloff_start_opacity: 0.0,
            falloff_stop_opacity: 0.0,
            refraction_power: 0.0,
            base_color: [0.0; 4],
            base_color_scale: 1.0,
            soft_falloff_depth: 0.0,
            greyscale_texture: String::new(),
            env_map_texture: String::new(),
            normal_texture: String::new(),
            env_mask_texture: String::new(),
            env_map_scale: 1.0,
            reflectance_texture: String::new(),
            lighting_texture: String::new(),
            emittance_color: [0.0; 3],
            emit_gradient_texture: String::new(),
            luminance: None,
        }
    }

    fn shape_with_shader(shader_idx: u32) -> BsTriShape {
        BsTriShape {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: None,
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            center: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            radius: 0.0,
            skin_ref: BlockRef::NULL,
            shader_property_ref: BlockRef(shader_idx),
            alpha_property_ref: BlockRef::NULL,
            vertex_desc: 0,
            num_triangles: 0,
            num_vertices: 0,
            vertices: Vec::new(),
            uvs: Vec::new(),
            normals: Vec::new(),
            vertex_colors: Vec::new(),
            triangles: Vec::new(),
            bone_weights: Vec::new(),
            bone_indices: Vec::new(),
        }
    }

    #[test]
    fn bgsm_on_lighting_shader_still_captured() {
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(minimal_lighting_shader_named(
                "Materials\\Architecture\\WhiterunStone.BGSM",
            )));
        let shape = shape_with_shader(0);
        assert_eq!(
            find_material_path_bs_tri_shape(&scene, &shape).as_deref(),
            Some("Materials\\Architecture\\WhiterunStone.BGSM")
        );
    }

    #[test]
    fn bgem_on_effect_shader_is_captured() {
        // #434 — this was the failing case. FO4 laser rifle beam binds a
        // `BSEffectShaderProperty` whose `net.name` ends in `.bgem`.
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(minimal_effect_shader_named(
            "Materials\\Weapons\\LaserRifle\\LaserBeam.BGEM",
        )));
        let shape = shape_with_shader(0);
        assert_eq!(
            find_material_path_bs_tri_shape(&scene, &shape).as_deref(),
            Some("Materials\\Weapons\\LaserRifle\\LaserBeam.BGEM")
        );
    }

    #[test]
    fn bgsm_on_effect_shader_also_captured() {
        // Occasionally artists bind a `.bgsm` (opaque) material to a
        // `BSEffectShaderProperty` — the engine treats the suffix as
        // advisory rather than gating, so the importer mirrors that.
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(minimal_effect_shader_named(
            "Materials\\Statics\\Sign01.BGSM",
        )));
        let shape = shape_with_shader(0);
        assert_eq!(
            find_material_path_bs_tri_shape(&scene, &shape).as_deref(),
            Some("Materials\\Statics\\Sign01.BGSM")
        );
    }

    #[test]
    fn non_material_name_returns_none() {
        // Plain NiObjectNET name without a material suffix — legitimate
        // Skyrim+ content (asset node name, not a material pointer).
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(minimal_effect_shader_named("FxGlowEdge01")));
        let shape = shape_with_shader(0);
        assert!(find_material_path_bs_tri_shape(&scene, &shape).is_none());
    }

    #[test]
    fn lighting_shader_name_takes_priority() {
        // If somehow both a BSLighting and a BSEffect shader share the
        // same slot (can't happen in valid NIFs, but the dispatch order
        // should still be deterministic), the BSLighting name wins — it's
        // the "normal" material channel.
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(minimal_lighting_shader_named(
                "Materials\\Primary.BGSM",
            )));
        let shape = shape_with_shader(0);
        assert_eq!(
            find_material_path_bs_tri_shape(&scene, &shape).as_deref(),
            Some("Materials\\Primary.BGSM")
        );
    }

    #[test]
    fn material_path_from_name_helper_accepts_both_suffixes() {
        assert_eq!(
            material_path_from_name(Some("x/y/z.bgem")).as_deref(),
            Some("x/y/z.bgem")
        );
        assert_eq!(
            material_path_from_name(Some("x/y/z.BGSM")).as_deref(),
            Some("x/y/z.BGSM")
        );
        assert_eq!(material_path_from_name(Some("plain_name")), None);
        assert_eq!(material_path_from_name(None), None);
    }
}
