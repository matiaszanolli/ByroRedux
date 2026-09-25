//! Pure data types the context hands across its API — draw commands, sky
//! and depth-of-field parameters, and the per-frame stat structs.
//!
//! #4217 — these never reference `VulkanContext` itself, so they did not fit
//! the lifecycle-phase axis the other `context/` submodules are split on:
//! `init`/`resize`/`draw`/`teardown` describe *when* code runs, and a struct
//! definition has no phase. They were ~770 of `mod.rs`'s production lines,
//! and `GroundcoverBenchCellHandle` was wedged between two halves of the
//! import block. `mod.rs` re-exports everything here, so every
//! `context::DrawCommand`-style path across the tree is unchanged.

use super::super::material::GpuMaterial;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// One resident exterior terrain cell, as the app publishes it to the
/// ground-cover sampling bench (#4052). The mesh handle is resolved against
/// `MeshRegistry` at dispatch time rather than here — see
/// [`super::super::groundcover_bench`]'s module docs on why a cached vertex offset
/// would be unsound.
#[derive(Clone, Copy, Debug)]
pub struct GroundcoverBenchCellHandle {
    /// Y-up world XZ of the cell's (row 0, col 0) terrain vertex.
    pub origin_xz: [f32; 2],
    /// `MeshHandle` of the terrain tile covering the cell.
    pub mesh_id: u32,
}

/// A single draw command: which mesh to draw, with what texture, and what model matrix.
pub struct DrawCommand {
    pub mesh_handle: u32,
    pub texture_handle: u32,
    pub model_matrix: [f32; 16],
    pub alpha_blend: bool,
    /// Source blend factor (Gamebryo AlphaFunction enum). Only meaningful
    /// when `alpha_blend` is true. 6 = SRC_ALPHA (default).
    pub src_blend: u8,
    /// Destination blend factor (Gamebryo AlphaFunction enum). Only meaningful
    /// when `alpha_blend` is true. 7 = INV_SRC_ALPHA (default).
    pub dst_blend: u8,
    pub two_sided: bool,
    /// `NiAlphaProperty.flags` bit 13 (0x2000, "No Sorter") — the shape
    /// author opted this draw out of Gamebryo's `NiAlphaAccumulator`
    /// depth sort, asking for accumulation (state-clustered) order
    /// instead. See #3797 and the alpha-over sort key's slot 3 doc
    /// below `build_render_data`'s sort routine
    /// (`byroredux/src/render/mod.rs`).
    pub no_sorter: bool,
    /// `NiWireframeProperty` flag — when true the batch routes to the
    /// `vk::PolygonMode::LINE` pipeline variant. Falls back to FILL
    /// silently when the device lacks `fillModeNonSolid`. See #869.
    pub wireframe: bool,
    /// `NiShadeProperty.flags == 0` flat-shading request — when true the
    /// per-instance `INSTANCE_FLAG_FLAT_SHADING` bit is set and the
    /// fragment shader replaces the interpolated vertex normal with
    /// the per-face screen-space derivative. See #869.
    pub flat_shading: bool,
    /// Distant LOD block; packed only for the terrain-LOD diagnostic view.
    pub is_lod: bool,
    /// Decal geometry — renders on top of coplanar surfaces via depth bias.
    pub is_decal: bool,
    /// Content-class layer for the per-layer depth-bias ladder. Replaces
    /// the ad-hoc `is_decal || alpha_test_func != 0` heuristic — see
    /// `byroredux_core::ecs::components::RenderLayer` and
    /// `byroredux_plugin::record::RecordType::render_layer`. Default
    /// value (`Architecture`) yields zero bias = pre-#renderlayer
    /// behaviour for everything that didn't already get the heuristic
    /// bias.
    pub render_layer: byroredux_core::ecs::components::RenderLayer,
    /// Base offset into the bone-palette SSBO for this draw, or 0 for rigid.
    pub bone_offset: u32,
    /// Bindless texture index for the normal map (0 = no normal map).
    pub normal_map_index: u32,
    /// Bindless texture index for the dark/lightmap (0 = no dark map). #264.
    pub dark_map_index: u32,
    /// Bindless texture index for the glow / self-illumination map
    /// (NiTexturingProperty slot 4). 0 = no glow map; the shader falls
    /// back to the inline `emissive_color` × `emissive_mult` constant.
    /// See #399.
    pub glow_map_index: u32,
    /// Bindless texture index for the detail overlay (NiTexturingProperty
    /// slot 2). Sampled at 2× UV scale and modulated into the base
    /// albedo. 0 = no detail map. See #399.
    pub detail_map_index: u32,
    /// #4422 — the encoded-space detail-combine neutral declared by the
    /// producer route (canonical `Material.detail_neutral`). Forwarded to
    /// `GpuMaterial.detailNeutral`; the shader divides the (raw-view)
    /// detail sample by it.
    pub detail_neutral: f32,
    /// Bindless texture index for the gloss map
    /// (NiTexturingProperty slot 3). Per Gamebryo 2.3
    /// `HandleGlossMap(... pkGlossiness)` the .r channel feeds the
    /// **glossiness / shininess** (Phong exponent) channel, which the
    /// fragment shader uses to modulate per-texel `roughness`. 0 = no
    /// gloss map. See #399 / #704.
    pub gloss_map_index: u32,
    /// Bindless texture index for the parallax / height map
    /// (`BSShaderTextureSet` slot 3). 0 = no POM; fragment shader
    /// falls back to flat normal mapping. See #453.
    pub parallax_map_index: u32,
    /// POM height scale (`BSShaderPPLightingProperty.parallax_scale`
    /// or Skyrim `ShaderTypeData::ParallaxOcc.scale`). Typical
    /// range 0.02–0.08. Default 0.04. See #453.
    pub parallax_height_scale: f32,
    /// POM ray-march sample budget (typically 4–16). Default 4.0
    /// matches the Gamebryo PPLighting default. See #453.
    pub parallax_max_passes: f32,
    /// Bindless texture index for the environment reflection map
    /// (`BSShaderTextureSet` slot 4). Currently sampled as a 2D
    /// texture; cubemap support is deferred. 0 = no env map. See #453.
    pub env_map_index: u32,
    /// Bindless texture index for the env-reflection mask
    /// (`BSShaderTextureSet` slot 5). 0 = unmasked. See #453.
    pub env_mask_index: u32,
    /// Alpha test threshold in [0,1]. 0.0 when alpha test is disabled. #263.
    pub alpha_threshold: f32,
    /// Alpha test comparison function (Gamebryo TestFunction enum). #263.
    /// 0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL, 4=GREATER, 5=NOTEQUAL,
    /// 6=GREATEREQUAL, 7=NEVER. Only meaningful when alpha_threshold > 0.
    pub alpha_test_func: u32,
    /// PBR roughness [0.05..0.95].
    pub roughness: f32,
    /// PBR metalness [0..1].
    pub metalness: f32,
    /// Per-material refractive index (#1248). Drives Schlick F0 via
    /// `F0 = ((1-η)/(1+η))²` in the fragment shader instead of the
    /// pre-#1248 hardcoded `vec3(0.04)` dielectric default. Default
    /// 1.5 reproduces that exact F0 ≈ 0.04 for legacy NIF content
    /// with no authored IOR; FO4 BGSM v9+ and Starfield .mat
    /// materials override with their authored value.
    pub ior: f32,
    /// BGEM v21+ dielectric reflection tint; neutral white otherwise.
    pub glass_fresnel_color: [f32; 3],
    /// BGEM v21+ refraction-deviation scale (format default 0.05).
    pub glass_refraction_scale: f32,
    /// BGEM v21+/v22 optical blur controls.
    pub glass_blur_scale: f32,
    pub glass_blur_scale_factor: f32,
    /// Source-authored Bethesda soft/rim/back-light response controls.
    pub lighting_effect_1: f32,
    pub lighting_effect_2: f32,
    pub subsurface_rolloff: f32,
    pub rimlight_power: f32,
    pub backlight_power: f32,
    pub fresnel_power: f32,
    pub grayscale_to_palette_scale: f32,
    /// Disney diffuse "subsurface" lobe weight (#1249). 0.0 keeps the
    /// pre-#1249 Lambert behaviour; 1.0 fully blends in the
    /// Hanrahan-Krueger fake-SSS approximation. Only consulted when
    /// `MAT_FLAG_BGSM_PBR` is set (legacy NIF stays on plain Lambert).
    pub subsurface: f32,
    /// Disney diffuse "sheen" lobe strength (#1249). 0.0 = no sheen;
    /// 1.0 = full fabric-class edge highlight. Same `MAT_FLAG_BGSM_PBR`
    /// gate.
    pub sheen: f32,
    /// Disney "sheen tint" (#1249) — `0` = white sheen, `1` = tinted by
    /// base colour, normalised by luminance so the tint carries hue only,
    /// not intensity (`mix(vec3(1), albedo / luminance(albedo), sheenTint)`,
    /// #2819 / REN-D17-05).
    pub sheen_tint: f32,
    /// Anisotropic GGX strength (#1250) [0, 1]. Drives the
    /// Disney `aspect = sqrt(1 - anisotropic * 0.9)` split into
    /// `ax = roughness / aspect, ay = roughness * aspect` at the
    /// shader. Default 0.0 → isotropic (the anisotropic NDF
    /// degenerates exactly to the legacy isotropic GGX).
    pub anisotropic: f32,
    /// Emissive intensity multiplier.
    pub emissive_mult: f32,
    /// Emissive color (RGB).
    pub emissive_color: [f32; 3],
    /// Specular intensity multiplier.
    pub specular_strength: f32,
    /// Specular color (RGB).
    pub specular_color: [f32; 3],
    /// Diffuse tint (RGB) — `NiMaterialProperty.diffuse` carried verbatim
    /// from `Material.diffuse_color`. Default `[1.0; 3]` (no tint). The
    /// fragment shader multiplies the sampled albedo by this. See #221.
    pub diffuse_color: [f32; 3],
    /// Ambient color (RGB) — `NiMaterialProperty.ambient`. Default
    /// `[1.0; 3]`. The fragment shader multiplies the cell ambient term
    /// by this. See #221.
    pub ambient_color: [f32; 3],
    /// Offset into the global vertex SSBO (in vertices).
    pub vertex_offset: u32,
    /// Offset into the global index SSBO (in indices).
    pub index_offset: u32,
    /// Vertex count for this mesh.
    pub vertex_count: u32,
    /// Camera-space depth for draw order sorting. Opaque draws are sorted
    /// front-to-back (smaller depth first) for early-Z; transparent draws
    /// are sorted back-to-front (larger depth first) for correct blending.
    /// Encoded as `f32::to_bits()` for deterministic `sort_unstable_by_key`.
    pub sort_depth: u32,
    /// Include this instance in the TLAS for RT ray queries.
    pub in_tlas: bool,
    /// Visible to the rasterizer this frame — `false` for entities whose
    /// `WorldBound` is outside the view frustum. Gated separately from
    /// `in_tlas` so off-screen occluders stay in the acceleration
    /// structure (so shadow / reflection / GI rays from on-screen
    /// fragments still hit them). Pre-#516 the frustum cull dropped
    /// the DrawCommand entirely, which also removed the TLAS entry and
    /// caused the BLAS LRU to age the occluder until it was evicted —
    /// visible as shadow pop-in and "flashlight through a wall" when
    /// the player rotated to face away from a backlit occluder.
    pub in_raster: bool,
    /// Pre-computed average albedo (RGB) for fast GI bounce approximation.
    /// Replaces per-hit UV lookup + texture sample in the GI ray hit shader.
    pub avg_albedo: [f32; 3],
    /// `BSLightingShaderProperty.shader_type` enum value (0–19) — fed
    /// to `GpuMaterial.material_kind` for the fragment shader's
    /// per-variant dispatch (SkinTint / HairTint / EyeEnvmap / etc.).
    /// 0 = Default lit. Plumbing only — variant rendering branches
    /// are per-variant follow-up work. See #344.
    pub material_kind: u32,
    /// Depth test enabled (`NiZBufferProperty.z_test`). Forwarded into
    /// `vkCmdSetDepthTestEnable` per draw batch via Vulkan 1.3 core
    /// extended dynamic state. Default true. See #398 (OBL-D4-H1).
    pub z_test: bool,
    /// Depth write enabled (`NiZBufferProperty.z_write`). Forwarded
    /// into `vkCmdSetDepthWriteEnable`. Default true. `false` for sky
    /// domes / viewmodels / glow halos / billboarded particles.
    pub z_write: bool,
    /// Depth comparison function (Gamebryo `TestFunction` enum).
    /// 0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL (default), 4=GREATER,
    /// 5=NOTEQUAL, 6=GREATEREQUAL, 7=NEVER. Mapped to
    /// `vk::CompareOp` and forwarded into `vkCmdSetDepthCompareOp`.
    pub z_function: u8,
    /// Terrain tile slot for LAND splat meshes. `None` on every non-
    /// terrain draw. When present, the draw assembler sets
    /// `INSTANCE_FLAG_TERRAIN_SPLAT` and packs the slot into the top
    /// 16 bits of `GpuInstance.flags` so the fragment shader can
    /// sample the 8 layer textures per `GpuTerrainTile`. See #470.
    pub terrain_tile_index: Option<u32>,
    /// Deterministic sort-key tiebreaker. Uniquely identifies this
    /// command within the frame so `par_sort_unstable_by_key` produces
    /// byte-identical output across runs for structurally-identical
    /// scene state. Pre-#506 the key ended on `mesh_handle` /
    /// `texture_handle`; full-tuple ties (same mesh, same material,
    /// same depth bucket, same blend) allowed rayon's work-stealing
    /// to reorder them differently frame-to-frame, breaking
    /// capture/replay + screenshot diff workflows. Semantically the
    /// ECS entity id for mesh draws; `entity ^ particle_index` for
    /// particle billboards; `u32::MAX` for the UI singleton.
    pub entity_id: u32,
    /// UV transform translation from `MaterialInfo.uv_offset`. FO4
    /// BGSM authors this explicitly; older games default to `(0,0)`.
    /// See #492.
    pub uv_offset: [f32; 2],
    /// UV transform scale from `MaterialInfo.uv_scale`. Defaults to
    /// `(1,1)` when absent. See #492.
    pub uv_scale: [f32; 2],
    /// Material alpha multiplier from `MaterialInfo.alpha` (BGSM
    /// `material_alpha`). Multiplied into the final blend-pass
    /// alpha. Default `1.0`. See #492.
    pub material_alpha: f32,
    // ── Skyrim+ BSLightingShaderProperty variant payloads (#562) ──
    //
    // Mirrors `MaterialInfo::ShaderTypeFields`. The fragment shader's
    // `material_kind` ladder consumes these when the instance's
    // `material_kind` matches the variant; zero on default-lit meshes.
    /// SkinTint (material_kind == 5): RGB skin tint + alpha.
    pub skin_tint_rgba: [f32; 4],
    /// HairTint (material_kind == 6): RGB hair tint. Default zero.
    pub hair_tint_rgb: [f32; 3],
    /// MultiLayerParallax (material_kind == 11) envmap strength.
    /// Packed alongside hair_tint on the GPU-side vec4 to save a
    /// dedicated slot; the two variants never co-occur on one mesh.
    pub multi_layer_envmap_strength: f32,
    /// EyeEnvmap (material_kind == 16) left-iris reflection center
    /// (object-space xyz).
    pub eye_left_center: [f32; 3],
    /// EyeEnvmap eye cubemap sample scale.
    pub eye_cubemap_scale: f32,
    /// EyeEnvmap right-iris reflection center.
    pub eye_right_center: [f32; 3],
    /// MultiLayerParallax inner-layer thickness scalar.
    pub multi_layer_inner_thickness: f32,
    /// MultiLayerParallax refraction scale scalar.
    pub multi_layer_refraction_scale: f32,
    /// MultiLayerParallax inner-layer UV scale `(u, v)`.
    pub multi_layer_inner_scale: [f32; 2],
    /// SparkleSnow (material_kind == 14) sparkle RGBA: color + intensity.
    pub sparkle_rgba: [f32; 4],
    // ── #620 / SK-D4-01: BSEffectShaderProperty falloff cone ────────
    /// `[start_angle, stop_angle, start_opacity, stop_opacity, soft_falloff_depth]`
    /// pulled from `MaterialInfo::effect_shader` when
    /// `material_kind == MATERIAL_KIND_EFFECT_SHADER`. Identity-pass-through
    /// `[1.0, 1.0, 1.0, 1.0, 0.0]` for non-effect materials. The fragment
    /// shader's effect-shader branch consumes them to fade alpha by view
    /// angle and soft-depth distance.
    pub effect_falloff: [f32; 5],
    /// R1 — index into the per-frame `MaterialTable` SSBO. Phase 2
    /// populates this from the per-material fields above; Phases 3–6
    /// migrate shader reads from per-instance copies to
    /// `materials[material_id].<field>` and finally drop the redundant
    /// per-instance fields. `0` is a valid id (the first material in
    /// the frame's table); meaningless when the table itself is empty.
    pub material_id: u32,
    /// `NiVertexColorProperty.vertex_mode == SOURCE_EMISSIVE` (#695 /
    /// O4-03). When set, the fragment shader treats the per-vertex
    /// `fragColor.rgb` as the authored emissive payload and skips the
    /// `albedo *= fragColor` modulation that the default
    /// `AmbientDiffuse` path applies. Mapped 1-to-1 onto
    /// `GpuMaterial::material_flags`'s
    /// [`material_flag::VERTEX_COLOR_EMISSIVE`](super::super::material::material_flag::VERTEX_COLOR_EMISSIVE)
    /// bit by `to_gpu_material`.
    pub vertex_color_emissive: bool,
    /// `BSEffectShaderProperty` flag bits packed into a
    /// `GpuMaterial::material_flags`-format u32 — populated by the
    /// importer via `pack_effect_shader_flags` in
    /// `byroredux::cell_loader`. OR'd directly into
    /// `GpuMaterial.material_flags` by [`to_gpu_material`] without
    /// per-bit re-encoding. `0` on every non-BSEffect mesh.
    /// See #890 Stage 2 / SK-D4-NEW-04.
    pub effect_shader_flags: u32,
    /// Bindless handle for the `BSEffectShaderProperty.greyscale_texture`
    /// palette LUT (#890 Stage 2c). `0` (the sentinel "missing texture"
    /// slot) means "no LUT" — the shader treats it as a disable signal
    /// even if `EFFECT_PALETTE_COLOR` / `EFFECT_PALETTE_ALPHA` are set,
    /// matching legacy behaviour where greyscale-mapped meshes without
    /// a valid LUT fall back to the raw source texture. Resolved through
    /// the common `MaterialTextureSet::greyscale_lut` role; populates
    /// `GpuMaterial::greyscale_lut_index` 1:1 via `to_gpu_material`.
    pub greyscale_lut_index: u32,
    /// Bindless handles for the common supplemental semantic roles.
    /// Ordering is defined by `material::supplemental_texture_slot`.
    pub supplemental_texture_indices:
        [u32; super::super::material::supplemental_texture_slot::COUNT],
    /// #1147 Phase 2b — BGSM v>=8 translucency suite, forwarded to
    /// `GpuMaterial.translucency_*`. Default zeros (no contribution
    /// when `MAT_FLAG_BGSM_TRANSLUCENCY` is unset). Populated by
    /// `byroredux::render::static_meshes::collect_static_mesh_draws`
    /// from the per-entity [`byroredux_core::ecs::Material`] component.
    pub translucency_subsurface_color: [f32; 3],
    pub translucency_transmissive_scale: f32,
    pub translucency_turbulence: f32,
    /// #2221 — `AnimatedShaderColor` sink override, forwarded to
    /// `GpuMaterial.shader_color_{r,g,b}`. Default `[0.0; 3]` — the
    /// field is captured for layout parity but unsampled by any shader
    /// today (see `GpuMaterial::shader_color_r`'s doc for why: the
    /// component is a deliberately generic single-slot sink with no
    /// single settled shader-uniform target). Populated by
    /// `byroredux::render::static_meshes::collect_static_mesh_draws`.
    pub shader_color: [f32; 3],
    /// #2221 — `AnimatedShaderFloat` sink override, forwarded to
    /// `GpuMaterial.shader_float`. Same unsampled-today status as
    /// `shader_color` above.
    pub shader_float: f32,
    /// `true` for water-surface entities — the triangle-pipeline path
    /// in `draw_frame` skips this command (only its `GpuInstance` SSBO
    /// slot is populated), and a parallel `WaterDrawCommand` in the
    /// frame's `water_commands` list re-emits the geometry through
    /// the water pipeline. Pre-water-plumbing this field is always
    /// `false`; the regular path handles it unconditionally.
    ///
    /// **TLAS exclusion contract (#1024 / F-WAT-03):** also load-bearing
    /// on the RT path — `build_tlas` skips any draw with
    /// `is_water == true` before BLAS lookup, so water never lands as
    /// a TLAS instance. Sibling to the mesh-side gate at
    /// `byroredux::cell_loader::water::spawn_water_plane` which uploads
    /// the water plane with `for_rt = false` (no BLAS slot is allocated).
    /// Both halves are belt-and-braces: removing either lets a future
    /// code path silently reintroduce water-ray self-hits (the water
    /// surface reflecting/refracting against itself instead of opaque
    /// geometry).
    pub is_water: bool,
}

impl DrawCommand {
    /// Project the per-material fields onto a [`GpuMaterial`] for the
    /// per-frame [`MaterialTable`]. Per-DRAW state (model matrix,
    /// mesh refs, bone offset, sort depth, visibility flags,
    /// terrain tile slot, entity id) is omitted — it stays on the
    /// per-instance `GpuInstance` because byte-identical materials
    /// can still appear at thousands of distinct world positions.
    ///
    /// R1 Phase 2 — produced once per `DrawCommand` and interned via
    /// `MaterialTable::intern`. Identical materials collapse to one
    /// id; distinct materials get distinct ids.
    pub fn to_gpu_material(&self) -> GpuMaterial {
        use super::super::material::supplemental_texture_slot as slot;
        GpuMaterial {
            roughness: self.roughness,
            metalness: self.metalness,
            emissive_mult: self.emissive_mult,
            emissive_r: self.emissive_color[0],
            emissive_g: self.emissive_color[1],
            emissive_b: self.emissive_color[2],
            specular_strength: self.specular_strength,
            specular_r: self.specular_color[0],
            specular_g: self.specular_color[1],
            specular_b: self.specular_color[2],
            alpha_threshold: self.alpha_threshold,
            normal_map_index: self.normal_map_index,
            dark_map_index: self.dark_map_index,
            glow_map_index: self.glow_map_index,
            detail_map_index: self.detail_map_index,
            detail_neutral: self.detail_neutral,
            gloss_map_index: self.gloss_map_index,
            parallax_map_index: self.parallax_map_index,
            env_map_index: self.env_map_index,
            env_mask_index: self.env_mask_index,
            alpha_test_func: self.alpha_test_func,
            material_kind: self.material_kind,
            material_alpha: self.material_alpha,
            parallax_height_scale: self.parallax_height_scale,
            parallax_max_passes: self.parallax_max_passes,
            uv_offset_u: self.uv_offset[0],
            uv_offset_v: self.uv_offset[1],
            uv_scale_u: self.uv_scale[0],
            uv_scale_v: self.uv_scale[1],
            diffuse_r: self.diffuse_color[0],
            diffuse_g: self.diffuse_color[1],
            diffuse_b: self.diffuse_color[2],
            ambient_r: self.ambient_color[0],
            ambient_g: self.ambient_color[1],
            ambient_b: self.ambient_color[2],
            // #804 — `avg_albedo` is no longer carried on `GpuMaterial`;
            // `caustic_splat.comp` + `triangle.frag` GI miss read the
            // per-instance copy on `GpuInstance.avgAlbedo*` instead.
            skin_tint_r: self.skin_tint_rgba[0],
            skin_tint_g: self.skin_tint_rgba[1],
            skin_tint_b: self.skin_tint_rgba[2],
            skin_tint_a: self.skin_tint_rgba[3],
            hair_tint_r: self.hair_tint_rgb[0],
            hair_tint_g: self.hair_tint_rgb[1],
            hair_tint_b: self.hair_tint_rgb[2],
            multi_layer_envmap_strength: self.multi_layer_envmap_strength,
            eye_left_center_x: self.eye_left_center[0],
            eye_left_center_y: self.eye_left_center[1],
            eye_left_center_z: self.eye_left_center[2],
            eye_cubemap_scale: self.eye_cubemap_scale,
            eye_right_center_x: self.eye_right_center[0],
            eye_right_center_y: self.eye_right_center[1],
            eye_right_center_z: self.eye_right_center[2],
            multi_layer_inner_thickness: self.multi_layer_inner_thickness,
            multi_layer_refraction_scale: self.multi_layer_refraction_scale,
            multi_layer_inner_scale_u: self.multi_layer_inner_scale[0],
            multi_layer_inner_scale_v: self.multi_layer_inner_scale[1],
            sparkle_r: self.sparkle_rgba[0],
            sparkle_g: self.sparkle_rgba[1],
            sparkle_b: self.sparkle_rgba[2],
            sparkle_intensity: self.sparkle_rgba[3],
            falloff_start_angle: self.effect_falloff[0],
            falloff_stop_angle: self.effect_falloff[1],
            falloff_start_opacity: self.effect_falloff[2],
            falloff_stop_opacity: self.effect_falloff[3],
            soft_falloff_depth: self.effect_falloff[4],
            material_flags: {
                // VERTEX_COLOR_EMISSIVE bit OR'd against the BSEffect
                // bits packed at the importer boundary (#890 Stage 2 —
                // `pack_effect_shader_flags`). Both contributors use
                // the same `material_flag::*` bit layout so no shift
                // / mask gymnastics are needed.
                let mut flags = self.effect_shader_flags;
                if self.vertex_color_emissive {
                    flags |= super::super::material::material_flag::VERTEX_COLOR_EMISSIVE;
                }
                flags
            },
            greyscale_lut_index: self.greyscale_lut_index,
            // #1147 Phase 2b — BGSM v>=8 translucency suite. The
            // `MAT_FLAG_BGSM_TRANSLUCENCY` bit in `material_flags`
            // gates whether the shader reads these (set by
            // `cell_loader::pack_imported_material_flags`).
            translucency_subsurface_r: self.translucency_subsurface_color[0],
            translucency_subsurface_g: self.translucency_subsurface_color[1],
            translucency_subsurface_b: self.translucency_subsurface_color[2],
            translucency_transmissive_scale: self.translucency_transmissive_scale,
            translucency_turbulence: self.translucency_turbulence,
            // #1248 — per-material refractive index.
            ior: self.ior,
            // #1249 — Disney diffuse lobe.
            subsurface: self.subsurface,
            sheen: self.sheen,
            sheen_tint: self.sheen_tint,
            // #1250 — anisotropic GGX strength.
            anisotropic: self.anisotropic,
            tint_map_index: self.supplemental_texture_indices[slot::TINT],
            inner_layer_map_index: self.supplemental_texture_indices[slot::INNER_LAYER],
            specular_map_index: self.supplemental_texture_indices[slot::SPECULAR],
            lighting_map_index: self.supplemental_texture_indices[slot::LIGHTING],
            flow_map_index: self.supplemental_texture_indices[slot::FLOW],
            wrinkle_map_index: self.supplemental_texture_indices[slot::WRINKLE],
            reflectance_map_index: self.supplemental_texture_indices[slot::REFLECTANCE],
            emittance_gradient_map_index: self.supplemental_texture_indices
                [slot::EMITTANCE_GRADIENT],
            decal_map_0_index: self.supplemental_texture_indices[slot::DECAL_0],
            decal_map_1_index: self.supplemental_texture_indices[slot::DECAL_1],
            decal_map_2_index: self.supplemental_texture_indices[slot::DECAL_2],
            decal_map_3_index: self.supplemental_texture_indices[slot::DECAL_3],
            // #2221 — animated shader color/float, unsampled today.
            shader_color_r: self.shader_color[0],
            shader_color_g: self.shader_color[1],
            shader_color_b: self.shader_color[2],
            shader_float: self.shader_float,
            glass_fresnel_r: self.glass_fresnel_color[0],
            glass_fresnel_g: self.glass_fresnel_color[1],
            glass_fresnel_b: self.glass_fresnel_color[2],
            glass_refraction_scale: self.glass_refraction_scale,
            glass_blur_scale: self.glass_blur_scale,
            glass_blur_scale_factor: self.glass_blur_scale_factor,
            glass_roughness_scratch_map_index: self.supplemental_texture_indices
                [slot::GLASS_ROUGHNESS_SCRATCH],
            glass_dirt_overlay_map_index: self.supplemental_texture_indices
                [slot::GLASS_DIRT_OVERLAY],
            lighting_effect_1: self.lighting_effect_1,
            lighting_effect_2: self.lighting_effect_2,
            subsurface_rolloff: self.subsurface_rolloff,
            rimlight_power: self.rimlight_power,
            backlight_power: self.backlight_power,
            fresnel_power: self.fresnel_power,
            grayscale_to_palette_scale: self.grayscale_to_palette_scale,
            lighting_mask_map_index: self.supplemental_texture_indices[slot::LIGHTING_MASK],
            back_lighting_map_index: self.supplemental_texture_indices[slot::BACK_LIGHTING],
        }
    }

    /// Hash of the material-relevant DrawCommand fields: exactly
    /// [`super::super::material::hash_gpu_material_fields`] of
    /// [`Self::to_gpu_material`]. Fed to
    /// [`super::super::material::MaterialTable::intern_by_hash`] as the
    /// dedup key. See #781 / PERF-N4.
    ///
    /// #4201 — this was a 150-line field walk kept in lockstep with the
    /// `GpuMaterial` one by hand, written that way so the ~97% dedup-hit
    /// path could skip building the struct. Building it turned out to be
    /// the cheap part: it is plain memory writes, while the walk was ~107
    /// dependent hash steps. Building and then hashing the bytes measured
    /// 0.17 ms against the walk's 0.48 ms at 7,359 draws, and the lockstep
    /// contract is now true by construction instead of by review.
    /// #4442 — the production call sites therefore pass the built struct
    /// straight to `MaterialTable::intern` (one build); this method
    /// remains the pinned bridge between the two sides of that contract.
    pub fn material_hash(&self) -> u64 {
        super::super::material::hash_gpu_material_fields(&self.to_gpu_material())
    }
}

/// 6-axis directional ambient cube on the renderer side. Mirror of
/// `byroredux::components::DalcCubeYup` — the engine crate owns the
/// Bethesda-Z-up → engine-Y-up axis swap (in `from_skyrim_zup`) and
/// per-TOD lerp (in `weather_system`); the renderer just receives
/// raw RGB per axis + specular tint + fresnel power and packs it into
/// `GpuDalcCube` at the draw boundary. `None` on every non-Skyrim cell
/// — the shader's fallback path keeps the legacy `AMBIENT_AO_FLOOR`
/// behaviour unchanged. See #993 / REN-AMBIENT-DALC.
#[derive(Debug, Clone, Copy)]
pub struct SkyDalcCube {
    /// Engine +X (east) ambient — raw RGB.
    pub pos_x: [f32; 3],
    pub neg_x: [f32; 3],
    /// Engine +Y (sky-fill / up) ambient — raw RGB.
    pub pos_y: [f32; 3],
    /// Engine -Y (ground-bounce / down / cavity-fill) ambient — raw RGB.
    pub neg_y: [f32; 3],
    pub pos_z: [f32; 3],
    pub neg_z: [f32; 3],
    /// DALC specular tint (vanilla Skyrim ships zeros on most weathers).
    pub specular: [f32; 3],
    /// DALC fresnel power tail (vanilla Skyrim ships 1.0).
    pub fresnel_power: f32,
}

/// Current weather controls consumed by the composite sky pass. Values are
/// normalized at the application EXAL boundary and interpolated there, so
/// the renderer remains independent of WTHR's per-game byte layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyWeatherParams {
    /// Broad procedural cloud occupancy in `[0, 1]`. Authored cloud DDS
    /// layers provide weather-specific detail on top of this continuous body.
    pub cloud_coverage: f32,
    /// `[r, g, b, alpha]` cloud tint for the four compatibility layers.
    pub cloud_tints: [[f32; 4]; 4],
    /// `[rain, snow]` precipitation intensity.
    pub precipitation: [f32; 2],
    pub thunder_frequency: f32,
    pub lightning_color: [f32; 3],
    pub stars_color: [f32; 3],
    pub sun_glare: f32,
    pub moon_glare: f32,
    pub aurora_intensity: f32,
    pub aurora_follows_sun: bool,
    pub wind_direction: [f32; 2],
    pub wind_speed: f32,
    /// History-dependent exposed-ground rain film, normalized to `[0, 1]`.
    /// Packed into the low 16 bits of `GpuCamera.render_debug.w`.
    pub surface_wetness: f32,
    /// History-dependent snow coverage/depth, normalized to `[0, 1]`.
    /// Packed into the high 16 bits of `GpuCamera.render_debug.w`.
    pub surface_snow: f32,
}

impl Default for SkyWeatherParams {
    fn default() -> Self {
        Self {
            cloud_coverage: 0.35,
            cloud_tints: [[1.0, 1.0, 1.0, 1.0]; 4],
            precipitation: [0.0; 2],
            thunder_frequency: 0.0,
            lightning_color: [1.0; 3],
            stars_color: [0.75, 0.8, 1.0],
            sun_glare: 1.0,
            moon_glare: 0.35,
            aurora_intensity: 0.0,
            aurora_follows_sun: false,
            wind_direction: [1.0, 0.0],
            wind_speed: 0.0,
            surface_wetness: 0.0,
            surface_snow: 0.0,
        }
    }
}

/// Sky rendering parameters passed per-frame to the composite shader.
/// Populated from WTHR records for exterior cells or a procedural fallback.
pub struct SkyParams {
    /// Zenith (top-of-sky) color, raw monitor-space per 0e8efc6.
    pub zenith_color: [f32; 3],
    /// The **exterior** worldspace's live TOD/weather zenith colour,
    /// carried even on interior cells (#3323).
    ///
    /// Distinct from [`Self::zenith_color`] on purpose. `build_sky_params`
    /// returns `SkyParams::default()` for any interior, because #1199 /
    /// #2226 established that an interior must never read a stale exterior
    /// `SkyParamsRes` — the whole TOD sky/sun/cloud set leaking into
    /// interior lighting is a real bug a sealed roof only hides. The
    /// window-portal escape needs a separate lane because its ray genuinely
    /// sees the outdoor sky; the interior default transmitted
    /// clear-noon blue at every hour.
    ///
    /// Populated from the surviving `SkyParamsRes` (World-lifetime, not
    /// cell-lifetime — see #1199) and falling back to
    /// the procedural exterior palette when no exterior has loaded this
    /// session. Uploaded as `GpuCamera::exterior_sky_tint` for the window
    /// escape fallback; the ordinary interior sky fields stay separate.
    pub exterior_zenith_color: [f32; 3],
    /// Outdoor palette used to bake the sky cubemap and paint clear-depth
    /// pixels through interior openings. Ordinary fields still describe the
    /// cell, so surface lighting and weather classification remain interior.
    pub portal_outdoor_sky: Option<OutdoorSkyParams>,
    /// Outdoor sun available to interior volumetric rays that pass through
    /// a verified aperture. Kept separate from the cell's sun fields so
    /// interior surfaces and the composite sky retain their own lighting.
    /// RGB is radiance after the time-of-day intensity ramp; zero at night.
    pub portal_sun_radiance: [f32; 3],
    /// Direction from a volume sample toward the outdoor sun, Y-up.
    pub portal_sun_direction: [f32; 3],
    /// The current interior CELL authors Show Sky / Behave Like Exterior.
    /// Volumetric rays may see open sky without a glass pane, and background
    /// pixels with no surface display the outdoor sky while room lighting
    /// remains interior.
    pub interior_show_sky: bool,
    /// Horizon color, raw monitor-space per 0e8efc6.
    pub horizon_color: [f32; 3],
    /// Below-horizon ground / lower-hemisphere color from WTHR's
    /// `SKY_LOWER` group (real `Sky-Lower` per nif.xml's NAM0
    /// schema — slot 7, fixed in #729). Pre-#541 the composite
    /// shader faked the below-horizon tint as `horizon_color * 0.3`,
    /// dropping the authored colour entirely. Now drives
    /// `composite.frag::compute_sky`'s `elevation < 0` branch.
    pub lower_color: [f32; 3],
    /// Sun direction (normalized, world-space Y-up).
    pub sun_direction: [f32; 3],
    /// Sun disc color, raw monitor-space per 0e8efc6.
    pub sun_color: [f32; 3],
    /// Angular size of the sun disc as cos(half-angle). ~0.9998 for real sun.
    pub sun_size: f32,
    /// Sun brightness multiplier.
    pub sun_intensity: f32,
    /// SKYAL — the directional light surfaces actually receive, in the
    /// engine's surface-lighting units: `compute_directional_upload`'s output
    /// (the WTHR sunlight colour scaled by the normalised TOD ramp). The cloud
    /// march is lit by this rather than by `sun_color * sun_intensity` (the
    /// sun *disc* colour and its raw 0-4 scale), so clouds and terrain share
    /// one sun, and a dense cloud can be calibrated against the engine's own
    /// white diffuse surface. Zero when there is no exterior sun.
    pub sun_illuminance: [f32; 3],
    /// Angular half-radius of the sun as a tangent-plane disk, in
    /// radians. Drives PCSS-lite directional-shadow disk jitter in
    /// `triangle.frag`. Default 0.020 (~1.15°) gives ~10 cm penumbra
    /// at 5 m blocker distance — visible without flooding sharp
    /// edges; smaller values approach the physical sun (~0.0047 rad)
    /// at the cost of cell-scale soft shadows. Plumbed via
    /// `GpuCamera.sky_tint.w` (the previously-reserved slot) so this
    /// change didn't touch GpuCamera's then-336 B layout (since grown to
    /// 368 B by unrelated additions). See #1023 / REN-D20-NEW-01.
    pub sun_angular_radius: f32,
    /// Whether sky rendering is enabled (true for exterior cells).
    pub is_exterior: bool,
    /// Cloud layer 0 scroll offset in UV space (accumulated by weather_system).
    pub cloud_scroll: [f32; 2],
    /// Cloud layer 0 UV tile scale. `0.0` disables the cloud sample in the shader.
    pub cloud_tile_scale: f32,
    /// Bindless texture handle for cloud_textures[0]. Ignored when
    /// `cloud_tile_scale == 0.0`; otherwise must be a valid TextureRegistry index.
    pub cloud_texture_index: u32,
    /// Bindless texture handle for the CLMT FNAM sun sprite. `0` =
    /// use the procedural disc (matching pre-#478 behaviour);
    /// otherwise the fragment shader samples `textures[idx]` within
    /// the sun disc radius so per-climate-authored sun textures
    /// (FNV `sun00.dds`, etc.) render instead of the flat `sun_color`.
    /// See #478.
    pub sun_texture_index: u32,
    /// Cloud layer 1 scroll offset (WTHR CNAM). Drifts in the opposite
    /// U direction to layer 0 to produce visible parallax between the
    /// two cloud layers.
    pub cloud_scroll_1: [f32; 2],
    /// Cloud layer 1 UV tile scale. `0.0` disables the layer (shader
    /// branch-skips the bindless sample). `0.0` when no CNAM is available.
    pub cloud_tile_scale_1: f32,
    /// Bindless texture handle for cloud_textures[1] (WTHR CNAM).
    pub cloud_texture_index_1: u32,
    /// Cloud layer 2 scroll offset (WTHR ANAM) — M33.1.
    pub cloud_scroll_2: [f32; 2],
    /// Cloud layer 2 UV tile scale. `0.0` disables the layer.
    pub cloud_tile_scale_2: f32,
    /// Bindless texture handle for cloud_textures[2] (WTHR ANAM).
    pub cloud_texture_index_2: u32,
    /// Cloud layer 3 scroll offset (WTHR BNAM) — M33.1.
    pub cloud_scroll_3: [f32; 2],
    /// Cloud layer 3 UV tile scale. `0.0` disables the layer.
    pub cloud_tile_scale_3: f32,
    /// Bindless texture handle for cloud_textures[3] (WTHR BNAM).
    pub cloud_texture_index_3: u32,
    /// Per-TOD-interpolated 6-axis directional ambient cube from Skyrim
    /// `WTHR.DALC`. `None` for FNV / FO3 / Oblivion (no DALC subrecord) —
    /// the GPU consumer sets `GpuDalcCube.flags.x = 0.0` so triangle.frag
    /// falls back to the legacy `AMBIENT_AO_FLOOR` path on those games.
    /// See #993 / REN-AMBIENT-DALC.
    pub dalc_cube: Option<SkyDalcCube>,
    /// Current weather effects and cloud tint state.
    pub weather: SkyWeatherParams,
    /// Monotonic session time used to animate rain, snow, lightning, and
    /// aurora without coupling the renderer to GameTimeRes.
    pub weather_time_seconds: f32,
}

/// The outdoor palette held by value for an interior's sky bake. Keeping
/// this non-recursive avoids allocating a boxed SkyParams on every frame.
#[derive(Clone, Copy)]
pub struct OutdoorSkyParams {
    pub zenith_color: [f32; 3],
    pub exterior_zenith_color: [f32; 3],
    pub portal_sun_radiance: [f32; 3],
    pub portal_sun_direction: [f32; 3],
    pub interior_show_sky: bool,
    pub horizon_color: [f32; 3],
    pub lower_color: [f32; 3],
    pub sun_direction: [f32; 3],
    pub sun_color: [f32; 3],
    pub sun_size: f32,
    pub sun_intensity: f32,
    pub sun_illuminance: [f32; 3],
    pub sun_angular_radius: f32,
    pub is_exterior: bool,
    pub cloud_scroll: [f32; 2],
    pub cloud_tile_scale: f32,
    pub cloud_texture_index: u32,
    pub sun_texture_index: u32,
    pub cloud_scroll_1: [f32; 2],
    pub cloud_tile_scale_1: f32,
    pub cloud_texture_index_1: u32,
    pub cloud_scroll_2: [f32; 2],
    pub cloud_tile_scale_2: f32,
    pub cloud_texture_index_2: u32,
    pub cloud_scroll_3: [f32; 2],
    pub cloud_tile_scale_3: f32,
    pub cloud_texture_index_3: u32,
    pub dalc_cube: Option<SkyDalcCube>,
    pub weather: SkyWeatherParams,
    pub weather_time_seconds: f32,
}

impl From<SkyParams> for OutdoorSkyParams {
    fn from(sky: SkyParams) -> Self {
        Self {
            zenith_color: sky.zenith_color,
            exterior_zenith_color: sky.exterior_zenith_color,
            portal_sun_radiance: sky.portal_sun_radiance,
            portal_sun_direction: sky.portal_sun_direction,
            interior_show_sky: sky.interior_show_sky,
            horizon_color: sky.horizon_color,
            lower_color: sky.lower_color,
            sun_direction: sky.sun_direction,
            sun_color: sky.sun_color,
            sun_size: sky.sun_size,
            sun_intensity: sky.sun_intensity,
            sun_illuminance: sky.sun_illuminance,
            sun_angular_radius: sky.sun_angular_radius,
            is_exterior: sky.is_exterior,
            cloud_scroll: sky.cloud_scroll,
            cloud_tile_scale: sky.cloud_tile_scale,
            cloud_texture_index: sky.cloud_texture_index,
            sun_texture_index: sky.sun_texture_index,
            cloud_scroll_1: sky.cloud_scroll_1,
            cloud_tile_scale_1: sky.cloud_tile_scale_1,
            cloud_texture_index_1: sky.cloud_texture_index_1,
            cloud_scroll_2: sky.cloud_scroll_2,
            cloud_tile_scale_2: sky.cloud_tile_scale_2,
            cloud_texture_index_2: sky.cloud_texture_index_2,
            cloud_scroll_3: sky.cloud_scroll_3,
            cloud_tile_scale_3: sky.cloud_tile_scale_3,
            cloud_texture_index_3: sky.cloud_texture_index_3,
            dalc_cube: sky.dalc_cube,
            weather: sky.weather,
            weather_time_seconds: sky.weather_time_seconds,
        }
    }
}

impl OutdoorSkyParams {
    pub(super) fn as_sky_params(self) -> SkyParams {
        SkyParams {
            portal_outdoor_sky: None,
            zenith_color: self.zenith_color,
            exterior_zenith_color: self.exterior_zenith_color,
            portal_sun_radiance: self.portal_sun_radiance,
            portal_sun_direction: self.portal_sun_direction,
            interior_show_sky: self.interior_show_sky,
            horizon_color: self.horizon_color,
            lower_color: self.lower_color,
            sun_direction: self.sun_direction,
            sun_color: self.sun_color,
            sun_size: self.sun_size,
            sun_intensity: self.sun_intensity,
            sun_illuminance: self.sun_illuminance,
            sun_angular_radius: self.sun_angular_radius,
            is_exterior: self.is_exterior,
            cloud_scroll: self.cloud_scroll,
            cloud_tile_scale: self.cloud_tile_scale,
            cloud_texture_index: self.cloud_texture_index,
            sun_texture_index: self.sun_texture_index,
            cloud_scroll_1: self.cloud_scroll_1,
            cloud_tile_scale_1: self.cloud_tile_scale_1,
            cloud_texture_index_1: self.cloud_texture_index_1,
            cloud_scroll_2: self.cloud_scroll_2,
            cloud_tile_scale_2: self.cloud_tile_scale_2,
            cloud_texture_index_2: self.cloud_texture_index_2,
            cloud_scroll_3: self.cloud_scroll_3,
            cloud_tile_scale_3: self.cloud_tile_scale_3,
            cloud_texture_index_3: self.cloud_texture_index_3,
            dalc_cube: self.dalc_cube,
            weather: self.weather,
            weather_time_seconds: self.weather_time_seconds,
        }
    }
}

/// Depth-of-field parameters for the current frame.
///
/// When `aperture > 0.0` the renderer jitters the camera position each frame
/// within a disk of radius `aperture` centred on the main camera position.
/// TAA accumulates the samples so surfaces at `focus_dist` are sharp while
/// surfaces at other depths are progressively blurred — a physically-based
/// thin-lens bokeh effect that costs zero extra passes.
///
/// Pass `DofView::default()` (aperture = 0.0) to disable DOF entirely.
#[derive(Debug, Clone, Copy)]
pub struct DofView {
    /// Lens aperture half-radius in world units. `0.0` = pinhole / no DOF.
    pub aperture: f32,
    /// Focal distance in world units. Surfaces at this depth are in focus.
    pub focus_dist: f32,
    /// Camera right vector (world space, unit length).
    pub cam_right: [f32; 3],
    /// Camera up vector (world space, unit length).
    pub cam_up: [f32; 3],
    /// Camera forward vector (world space, unit length, into the scene).
    pub cam_forward: [f32; 3],
    /// Perspective projection matrix (column-major, Vulkan clip space with Y-flip).
    pub proj_mat: [f32; 16],
    /// Exact authored perspective parameters used by temporal reconstruction.
    pub camera_near: f32,
    pub camera_far: f32,
    pub camera_fov_y: f32,
}

impl Default for DofView {
    fn default() -> Self {
        Self {
            aperture: 0.0,
            focus_dist: 20.0,
            cam_right: [1.0, 0.0, 0.0],
            cam_up: [0.0, 1.0, 0.0],
            cam_forward: [0.0, 0.0, -1.0],
            proj_mat: byroredux_core::math::Mat4::IDENTITY.to_cols_array(),
            camera_near: 0.1,
            camera_far: 1_000.0,
            camera_fov_y: std::f32::consts::FRAC_PI_4,
        }
    }
}

impl Default for SkyParams {
    fn default() -> Self {
        Self {
            zenith_color: [0.15, 0.3, 0.6],
            // Same value: with no exterior ever loaded there is no live sky
            // to report, and the portal keeps its pre-#3323 transmission.
            exterior_zenith_color: [0.15, 0.3, 0.6],
            portal_outdoor_sky: None,
            portal_sun_radiance: [0.0; 3],
            portal_sun_direction: [0.0, -1.0, 0.0],
            interior_show_sky: false,
            horizon_color: [0.5, 0.5, 0.45],
            // Pre-#541 fake `horizon * 0.3` baseline preserved as the
            // default; real WTHR-driven exterior cells overwrite from
            // their authored `SKY_LOWER` slot.
            lower_color: [0.15, 0.15, 0.135],
            sun_direction: [-0.4, 0.8, -0.45],
            sun_color: [1.0, 0.95, 0.8],
            sun_size: 0.9994, // cos(~2°) — visible disc, larger than real sun
            sun_intensity: 5.0,
            // No exterior sun: the cloud march receives ambient only.
            sun_illuminance: [0.0; 3],
            // Tangent-plane half-radius (rad) for PCSS-lite shadow
            // disk jitter. Matches the pre-#1023 hardcoded shader
            // constant so behaviour is unchanged unless a caller
            // overrides it. See SkyParams::sun_angular_radius doc.
            sun_angular_radius: 0.020,
            is_exterior: false,
            cloud_scroll: [0.0, 0.0],
            cloud_tile_scale: 0.0, // disabled until WTHR supplies a cloud texture
            cloud_texture_index: 0,
            sun_texture_index: 0, // 0 = procedural disc (pre-#478 fallback)
            cloud_scroll_1: [0.0, 0.0],
            cloud_tile_scale_1: 0.0,
            cloud_texture_index_1: 0,
            cloud_scroll_2: [0.0, 0.0],
            cloud_tile_scale_2: 0.0,
            cloud_texture_index_2: 0,
            cloud_scroll_3: [0.0, 0.0],
            cloud_tile_scale_3: 0.0,
            cloud_texture_index_3: 0,
            // None ⇒ shader fallback to AMBIENT_AO_FLOOR. Skyrim cells
            // overwrite from per-TOD-lerped WTHR.DALC.
            dalc_cube: None,
            weather: SkyWeatherParams::default(),
            weather_time_seconds: 0.0,
        }
    }
}

/// Per-frame draw-call counts written unconditionally by `draw_frame`
/// (i.e. NOT gated on `Some(timings)` the way [`FrameTimings`] is).
/// Read by the app via `VulkanContext::last_draw_call_stats` after
/// `draw_frame` returns. See #1258 / PERF-D3-NEW-03: the pre-batch
/// `DrawCommand` count (what the audit measured at 12,277/frame) is
/// computed app-side; this struct surfaces the post-batch GPU call
/// counts that actually drive cost.
#[derive(Default, Clone, Copy)]
pub struct DrawCallStats {
    /// Number of [`DrawBatch`] records after the merge loop at
    /// `draw.rs::DrawBatch` construction — one entry per
    /// `(mesh_handle, pipeline_key, two_sided, render_layer,
    /// depth-state)` group of `DrawCommand`s. Upper bound on the actual
    /// GPU call count; `cmd_draw_indexed_indirect` further compresses
    /// runs of same-pipeline same-layer batches into a single call (see
    /// `indirect_call_count` below).
    pub batch_count: u32,
    /// Number of `cmd_draw_indexed` + `cmd_draw_indexed_indirect`
    /// invocations actually recorded into the frame's command buffer
    /// for the main raster pass. Indirect grouping at
    /// `draw.rs::draw_record_loop` collapses runs of compatible
    /// batches into a single indirect call, so this is `<= batch_count`.
    /// Excludes the water, sky, UI, and composite passes — those run
    /// outside the batch loop and contribute O(1) draws each.
    pub indirect_call_count: u32,
}

/// Per-frame CPU timing breakdown returned by `draw_frame` when profiling.
/// All fields are nanoseconds; divide by 1_000_000.0 for milliseconds.
/// Only populated when `draw_frame` is called with `Some(timings)`.
#[derive(Default, Clone, Copy)]
pub struct FrameTimings {
    /// `wait_for_fences` — CPU stall waiting for previous GPU frame(s).
    /// If large, the bottleneck is GPU-side; CPU optimisation yields little.
    pub fence_wait_ns: u64,
    /// `build_instance_map` + `build_tlas` CPU work (instance list gather,
    /// AS build command record, TLAS barrier). GPU AS build runs async.
    pub tlas_build_ns: u64,
    /// Instance SSBO fill loop (773 × GpuInstance) + `upload_instances`
    /// memcpy + `upload_indirect_draws`. Dominant CPU-side work per frame.
    pub ssbo_build_ns: u64,
    /// `MeshRegistry::rebuild_geometry_ssbo` — the resumable global geometry
    /// SSBO copy (#3298), including its per-frame chunk. Drained from the
    /// registry rather than measured here because the call happens in
    /// `render_one_frame` before `draw_frame`, and it is a synchronous staged
    /// copy on a one-time command buffer, so no GPU timer can bracket it
    /// (#3467). Zero on every frame with no rebuild in flight.
    pub geometry_rebuild_ns: u64,
    /// `begin_render_pass` through `end_command_buffer` — Vulkan command
    /// recording for geometry, UI, SVGF, TAA, SSAO, composite.
    pub cmd_record_ns: u64,
    /// `queue_submit` + `queue_present` — driver overhead + vsync stall.
    pub submit_present_ns: u64,
    /// `vkAcquireNextImageKHR` — CPU stall waiting for the next
    /// swapchain image to become available. With FIFO present
    /// mode + a low swapchain image count, this is where the
    /// compositor / vsync block hides. Added in Phase 9 to close
    /// the "390 ms unaccounted with fence_wait + submit_present
    /// both trivial" gap.
    pub acquire_ns: u64,
}

/// Handle for requesting and retrieving screenshots from outside the render loop.
/// Outside-the-render-loop handle for depth capture (#3308). Deliberately
/// thinner than [`ScreenshotHandle`]: one consumer, so no owner tag and no
/// capture generation — see `depth_capture.rs`'s module doc for why those
/// are unnecessary here rather than merely omitted.
#[derive(Clone)]
pub struct DepthCaptureHandle {
    pub requested: Arc<AtomicBool>,
    pub result: Arc<Mutex<Option<byroredux_core::ecs::DepthCapture>>>,
    /// `Some(format_name)` when this device's selected depth format is one
    /// `depth_capture_record_copy` refuses to capture (#3570) — i.e.
    /// anything but `D32_SFLOAT`. `None` when capture is supported.
    ///
    /// #4003 — the refusal previously had no path back to the console: the
    /// result slot stayed empty forever and `depth.stats` re-armed on every
    /// call. Reported here at handle time (a property of the device, fixed
    /// for the session) so the console can say so on the first invocation
    /// rather than after a doomed round trip.
    pub unsupported_format: Option<String>,
}

pub struct ScreenshotHandle {
    /// Set to `true` to request a screenshot on the next frame.
    pub requested: Arc<AtomicBool>,
    /// After capture, the PNG bytes are placed here for retrieval.
    pub result: Arc<Mutex<Option<Vec<u8>>>>,
    /// Monotonic capture generation, shared with `ScreenshotBridge`
    /// (#1603). The renderer captures it at record time and only
    /// publishes the PNG if it still matches at readback time, so a
    /// cancelled-then-resumed straggler is discarded.
    pub generation: Arc<AtomicU64>,
}

impl Default for ScreenshotHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenshotHandle {
    pub fn new() -> Self {
        Self {
            requested: Arc::new(AtomicBool::new(false)),
            result: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Request a screenshot. Returns immediately; check `result` later.
    pub fn request(&self) {
        self.requested.store(true, Ordering::Release);
    }

    /// Take the screenshot result if available. Returns None if not ready.
    pub fn take_result(&self) -> Option<Vec<u8>> {
        // #1174 — recover from poison. Aliased to the same Arc<Mutex>
        // as `ScreenshotBridge.result`; matching policy.
        self.result.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}
