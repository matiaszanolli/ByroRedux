//! Top-level Vulkan context that owns the entire graphics state.

use super::acceleration::AccelerationManager;
use super::allocator::{self, SharedAllocator};
use super::bloom::BloomPipeline;
use super::caustic::CausticPipeline;
use super::composite::{CompositePipeline, HDR_FORMAT};
use super::compute::ClusterCullPipeline;
use super::debug;
use super::device::{self, QueueFamilyIndices};
use super::gbuffer::{
    GBuffer, ALBEDO_FORMAT, MESH_ID_FORMAT, MOTION_FORMAT, NORMAL_FORMAT, RAW_INDIRECT_FORMAT,
};
use super::instance;
use super::material::GpuMaterial;
use super::pipeline;
use super::scene_buffer;
use super::ssao::SsaoPipeline;
use super::surface;
use super::svgf::SvgfPipeline;
use super::swapchain::{self, SwapchainState};
use super::sync::{self, FrameSync, MAX_FRAMES_IN_FLIGHT};
use super::taa::TaaPipeline;
use super::texture::Texture;
use super::volumetrics::VolumetricsPipeline;
use crate::mesh::MeshRegistry;
use crate::texture_registry::TextureRegistry;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Maximum number of skinned-mesh `SkinSlot`s the per-skinned-entity
/// pre-skin + BLAS refit pool can hold simultaneously. Each slot costs
/// `3 × MAX_FRAMES_IN_FLIGHT = 6` storage-buffer descriptors. Sized to
/// cover M41-EQUIP Prospector load (~30 skinned entities) with ~2×
/// headroom; the architectural ceiling is `MAX_TOTAL_BONES /
/// MAX_BONES_PER_MESH = 256` (the bone-palette SSBO ceiling), so 64
/// stays well below and remains a pressure signal rather than a no-op.
/// Pre-#900 this was 32 and Prospector overflowed by 2 entities every
/// frame. See #900.
pub const SKIN_MAX_SLOTS: u32 = 64;

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
    /// to `GpuInstance.material_kind` for the fragment shader's
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
    /// [`material_flag::VERTEX_COLOR_EMISSIVE`](super::material::material_flag::VERTEX_COLOR_EMISSIVE)
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
            texture_index: self.texture_handle,
            normal_map_index: self.normal_map_index,
            dark_map_index: self.dark_map_index,
            glow_map_index: self.glow_map_index,
            detail_map_index: self.detail_map_index,
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
                    flags |= super::material::material_flag::VERTEX_COLOR_EMISSIVE;
                }
                flags
            },
            _pad_falloff: 0.0,
        }
    }

    /// Hash of the material-relevant DrawCommand fields, in lockstep
    /// with [`super::material::hash_gpu_material_fields`]. Fed to
    /// [`super::material::MaterialTable::intern_by_hash`] to skip the
    /// `to_gpu_material` construction on the ~97% dedup-hit path.
    /// See #781 / PERF-N4.
    ///
    /// **Lockstep contract**: this function MUST walk the same fields
    /// `to_gpu_material` reads, in the same order, mapping the
    /// `DrawCommand` source to the `GpuMaterial` destination 1:1. A
    /// drift between this walk and `to_gpu_material` would silently
    /// produce a hash that doesn't match `hash_gpu_material_fields(&cmd
    /// .to_gpu_material())`, causing dedup misses (perf regression) or
    /// — under collision in the index — silent miscoloring. The
    /// pinning test
    /// `material_hash_matches_gpu_material_field_hash` walks a fully-
    /// populated DrawCommand through both sides and asserts the hashes
    /// agree; debug builds also assert it inside `intern_by_hash`.
    pub fn material_hash(&self) -> u64 {
        use std::hash::Hasher;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        // PBR scalars + flags
        h.write_u32(self.roughness.to_bits());
        h.write_u32(self.metalness.to_bits());
        h.write_u32(self.emissive_mult.to_bits());
        // Must mirror the same OR composition as `to_gpu_material` so
        // the byte-level material hash stays in lockstep (#781 contract).
        let material_flags = {
            let mut flags = self.effect_shader_flags;
            if self.vertex_color_emissive {
                flags |= super::material::material_flag::VERTEX_COLOR_EMISSIVE;
            }
            flags
        };
        h.write_u32(material_flags);
        // Emissive RGB + specular_strength
        h.write_u32(self.emissive_color[0].to_bits());
        h.write_u32(self.emissive_color[1].to_bits());
        h.write_u32(self.emissive_color[2].to_bits());
        h.write_u32(self.specular_strength.to_bits());
        // Specular RGB + alpha_threshold
        h.write_u32(self.specular_color[0].to_bits());
        h.write_u32(self.specular_color[1].to_bits());
        h.write_u32(self.specular_color[2].to_bits());
        h.write_u32(self.alpha_threshold.to_bits());
        // Texture indices group A
        h.write_u32(self.texture_handle);
        h.write_u32(self.normal_map_index);
        h.write_u32(self.dark_map_index);
        h.write_u32(self.glow_map_index);
        // Texture indices group B
        h.write_u32(self.detail_map_index);
        h.write_u32(self.gloss_map_index);
        h.write_u32(self.parallax_map_index);
        h.write_u32(self.env_map_index);
        // env_mask + alpha_test_func + material_kind + material_alpha
        h.write_u32(self.env_mask_index);
        h.write_u32(self.alpha_test_func);
        h.write_u32(self.material_kind);
        h.write_u32(self.material_alpha.to_bits());
        // Parallax POM + UV offset
        h.write_u32(self.parallax_height_scale.to_bits());
        h.write_u32(self.parallax_max_passes.to_bits());
        h.write_u32(self.uv_offset[0].to_bits());
        h.write_u32(self.uv_offset[1].to_bits());
        // UV scale + diffuse RG
        h.write_u32(self.uv_scale[0].to_bits());
        h.write_u32(self.uv_scale[1].to_bits());
        h.write_u32(self.diffuse_color[0].to_bits());
        h.write_u32(self.diffuse_color[1].to_bits());
        // diffuse_b + ambient RGB
        h.write_u32(self.diffuse_color[2].to_bits());
        h.write_u32(self.ambient_color[0].to_bits());
        h.write_u32(self.ambient_color[1].to_bits());
        h.write_u32(self.ambient_color[2].to_bits());
        // Skyrim+ skin tint A/R/G/B (note GpuMaterial layout puts A
        // first within its vec4 for std430 packing — this walk
        // preserves that order to stay byte-equal-safe).
        h.write_u32(self.skin_tint_rgba[3].to_bits()); // A
        h.write_u32(self.skin_tint_rgba[0].to_bits()); // R
        h.write_u32(self.skin_tint_rgba[1].to_bits()); // G
        h.write_u32(self.skin_tint_rgba[2].to_bits()); // B
                                                       // hair tint RGB + multi_layer_envmap_strength
        h.write_u32(self.hair_tint_rgb[0].to_bits());
        h.write_u32(self.hair_tint_rgb[1].to_bits());
        h.write_u32(self.hair_tint_rgb[2].to_bits());
        h.write_u32(self.multi_layer_envmap_strength.to_bits());
        // Eye left + eye_cubemap_scale
        h.write_u32(self.eye_left_center[0].to_bits());
        h.write_u32(self.eye_left_center[1].to_bits());
        h.write_u32(self.eye_left_center[2].to_bits());
        h.write_u32(self.eye_cubemap_scale.to_bits());
        // Eye right + multi_layer_inner_thickness
        h.write_u32(self.eye_right_center[0].to_bits());
        h.write_u32(self.eye_right_center[1].to_bits());
        h.write_u32(self.eye_right_center[2].to_bits());
        h.write_u32(self.multi_layer_inner_thickness.to_bits());
        // refraction + multi_layer_inner_scale UV + sparkle_r
        h.write_u32(self.multi_layer_refraction_scale.to_bits());
        h.write_u32(self.multi_layer_inner_scale[0].to_bits());
        h.write_u32(self.multi_layer_inner_scale[1].to_bits());
        h.write_u32(self.sparkle_rgba[0].to_bits()); // sparkle_r
                                                     // sparkle GB + sparkle_intensity + falloff_start_angle
        h.write_u32(self.sparkle_rgba[1].to_bits()); // sparkle_g
        h.write_u32(self.sparkle_rgba[2].to_bits()); // sparkle_b
        h.write_u32(self.sparkle_rgba[3].to_bits()); // intensity
        h.write_u32(self.effect_falloff[0].to_bits()); // falloff_start_angle
                                                       // falloff_stop + opacities + soft_falloff_depth
        h.write_u32(self.effect_falloff[1].to_bits()); // falloff_stop_angle
        h.write_u32(self.effect_falloff[2].to_bits()); // start_opacity
        h.write_u32(self.effect_falloff[3].to_bits()); // stop_opacity
        h.write_u32(self.effect_falloff[4].to_bits()); // soft_falloff_depth
        h.finish()
    }
}

/// Sky rendering parameters passed per-frame to the composite shader.
/// Populated from WTHR records for exterior cells or a procedural fallback.
pub struct SkyParams {
    /// Zenith (top-of-sky) color, raw monitor-space per 0e8efc6.
    pub zenith_color: [f32; 3],
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
}

impl Default for SkyParams {
    fn default() -> Self {
        Self {
            zenith_color: [0.15, 0.3, 0.6],
            horizon_color: [0.5, 0.5, 0.45],
            // Pre-#541 fake `horizon * 0.3` baseline preserved as the
            // default; real WTHR-driven exterior cells overwrite from
            // their authored `SKY_LOWER` slot.
            lower_color: [0.15, 0.15, 0.135],
            sun_direction: [-0.4, 0.8, -0.45],
            sun_color: [1.0, 0.95, 0.8],
            sun_size: 0.9994, // cos(~2°) — visible disc, larger than real sun
            sun_intensity: 5.0,
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
        }
    }
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
    /// `begin_render_pass` through `end_command_buffer` — Vulkan command
    /// recording for geometry, UI, SVGF, TAA, SSAO, composite.
    pub cmd_record_ns: u64,
    /// `queue_submit` + `queue_present` — driver overhead + vsync stall.
    pub submit_present_ns: u64,
}

/// Handle for requesting and retrieving screenshots from outside the render loop.
pub struct ScreenshotHandle {
    /// Set to `true` to request a screenshot on the next frame.
    pub requested: Arc<AtomicBool>,
    /// After capture, the PNG bytes are placed here for retrieval.
    pub result: Arc<Mutex<Option<Vec<u8>>>>,
}

impl ScreenshotHandle {
    pub fn new() -> Self {
        Self {
            requested: Arc::new(AtomicBool::new(false)),
            result: Arc::new(Mutex::new(None)),
        }
    }

    /// Request a screenshot. Returns immediately; check `result` later.
    pub fn request(&self) {
        self.requested.store(true, Ordering::Release);
    }

    /// Take the screenshot result if available. Returns None if not ready.
    pub fn take_result(&self) -> Option<Vec<u8>> {
        self.result.lock().unwrap().take()
    }
}

/// Parse the `BYROREDUX_RENDER_DEBUG` env var into a fragment-shader
/// debug-bypass bitmask. Accepts plain decimal (`3`) or hex (`0x3`).
/// Absent / invalid returns 0 — every bypass is off and the shader
/// branches are statically optimised away by the GPU's branch
/// predictor on a uniform-zero value.
fn parse_render_debug_flags_env() -> u32 {
    let Ok(s) = std::env::var("BYROREDUX_RENDER_DEBUG") else {
        return 0;
    };
    let s = s.trim();
    let parsed = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u32>().ok()
    };
    match parsed {
        Some(v) => {
            log::info!("BYROREDUX_RENDER_DEBUG = 0x{:x} (POM bypass={}, detail bypass={}, normals viz={}, tangent viz={}, normal-map bypass={}, normal-map force-on={}, render-layer viz={}, glass-passthru viz={}, specular-AA disable={}, half-Lambert-fill disable={})",
                v, v & 1 != 0, v & 2 != 0, v & 4 != 0, v & 8 != 0, v & 0x10 != 0, v & 0x20 != 0, v & 0x40 != 0, v & 0x80 != 0, v & 0x100 != 0, v & 0x200 != 0);
            v
        }
        None => {
            log::warn!(
                "BYROREDUX_RENDER_DEBUG = {:?} could not be parsed as u32; ignoring",
                s
            );
            0
        }
    }
}

pub struct VulkanContext {
    // Ordered for drop safety — later fields are destroyed first.
    pub current_frame: usize,
    /// Monotonic frame counter for temporal effects (jitter seed, accumulation).
    pub frame_counter: u32,
    /// Debug-only fragment-shader bypass flags piped through
    /// `GpuCamera.jitter[2]`. Read once from `BYROREDUX_RENDER_DEBUG`
    /// at construction; stays put for the process lifetime. Bits:
    ///   `0x1` — bypass parallax-occlusion (`sampleUV = baseUV`)
    ///   `0x2` — bypass detail-map modulation
    ///   `0x4` — output world-space normal (gbuffer + outColor) and exit
    ///   `0x8` — visualize per-fragment tangent presence (green/red)
    ///   `0x10` — bypass normal-map perturbation (geometric N only)
    ///   `0x20` — re-enable normal-map perturbation (DISABLED BY
    ///            DEFAULT in the 2026-05-03 chrome-regression
    ///            follow-up — see `triangle.frag::DBG_FORCE_NORMAL_MAP`).
    ///   `0x40` — visualize render-layer classification (#renderlayer):
    ///            Architecture grey, Clutter cyan, Actor magenta,
    ///            Decal yellow. Empirical validation that the
    ///            `RecordType::render_layer` classifier matches
    ///            expectation on real cells.
    ///   `0x80` — visualize the IOR refraction passthru loop
    ///            (`DBG_VIZ_GLASS_PASSTHRU`, #789 follow-up):
    ///            black=IOR not allowed, red=escaped to sky,
    ///            yellow=first-hit terminus, green=passthru ×1,
    ///            cyan=passthru ×2 + non-self terminus,
    ///            magenta=budget exhausted with terminus still on
    ///            same-texture glass.
    ///   `0x100` — disable specular antialiasing
    ///            (`DBG_DISABLE_SPECULAR_AA`, Kaplanyan-Hoffman 2016).
    ///            The default-on path widens GGX `roughness` by the
    ///            per-fragment normal-variance kernel so corrugated
    ///            normal maps don't band into bright/dark stripes at
    ///            distance (Nellis Museum was the canonical
    ///            regression). Set the bit to A/B against suspected
    ///            spec-AA-introduced softness.
    ///   `0x200` — disable isotropic-ambient interior-fill path
    ///            (`DBG_DISABLE_HALF_LAMBERT_FILL`, name kept for
    ///            backward compat with the original half-Lambert
    ///            iteration). The default-on path skips the
    ///            Lambert + GGX BRDF entirely for directionals
    ///            uploaded with `radius == -1` (interior cells'
    ///            "subtle aesthetic fill") and accumulates as
    ///            `lightColor * albedo * INTERIOR_FILL_AMBIENT_FACTOR`
    ///            — normal-INDEPENDENT injection so corrugated /
    ///            high-frequency normal maps can't band into stripes
    ///            (Nellis Museum was the canonical regression). Set
    ///            the bit to revert to legacy Lambert + GGX for A/B.
    /// Env values are parsed as `0xN` hex or plain decimal; absent /
    /// invalid → 0 (all paths active, zero overhead). For ad-hoc
    /// bisection of texture / lighting artifacts. See engineering
    /// notes around the Dragonsreach "ghost carving" diagnosis.
    pub render_debug_flags: u32,
    /// Previous frame's view-projection matrix (column-major [f32; 16]).
    /// Used to compute screen-space motion vectors in the vertex shader.
    /// On the very first frame, equals the current frame's viewProj (no motion).
    pub prev_view_proj: [f32; 16],
    /// Per-frame scratch buffer for the GPU instance SSBO payload. Held on
    /// the context so that capacity amortizes across frames instead of
    /// heap-allocating fresh each `draw_frame`. Cleared + reserved at the
    /// top of draw_frame. See issue #243.
    gpu_instances_scratch: Vec<scene_buffer::GpuInstance>,
    /// Per-frame scratch buffer for draw batch metadata. Same lifecycle
    /// as `gpu_instances_scratch`. See issue #243.
    batches_scratch: Vec<draw::DrawBatch>,
    /// Per-frame scratch buffer for indirect draw commands. Replaces the
    /// per-frame `Vec::collect()` allocation that was untracked by the
    /// scratch-buffer pattern.
    indirect_draws_scratch: Vec<ash::vk::DrawIndexedIndirectCommand>,

    // ── Screenshot capture ──────────────────────────────────────────
    screenshot_requested: Arc<AtomicBool>,
    screenshot_result: Arc<Mutex<Option<Vec<u8>>>>,
    /// Staging buffer for screenshot readback (allocated on first capture).
    screenshot_staging: Option<(vk::Buffer, vk_alloc::Allocation, vk::DeviceSize)>,
    /// True when the staging buffer contains valid data waiting for fence.
    screenshot_pending_readback: bool,

    frame_sync: FrameSync,
    command_buffers: Vec<vk::CommandBuffer>,
    command_pool: vk::CommandPool,
    /// Dedicated pool for one-time upload/transfer commands, separate from
    /// the per-frame draw pool. Vulkan requires external synchronization on
    /// VkCommandPool (VUID-vkAllocateCommandBuffers-commandPool-00044);
    /// keeping upload commands on a separate pool avoids contention with
    /// draw command buffer reset/recording.
    pub transfer_pool: vk::CommandPool,
    /// Persistent fence reused across one-time submits (texture upload,
    /// BLAS build, mesh staging copy). Saves per-call VkFence
    /// create/destroy overhead during cell load (#302). Mutex serializes
    /// concurrent callers — only one reset+wait cycle at a time.
    pub transfer_fence: Arc<Mutex<vk::Fence>>,
    framebuffers: Vec<vk::Framebuffer>,
    // Single VkImage shared across all frames-in-flight (NOT per-frame
    // like the G-buffer / TAA / SVGF / caustic / SSAO attachments).
    // Safe at MAX_FRAMES_IN_FLIGHT == 2 because the double-fence wait
    // at draw.rs:108-120 (#282) is equivalent to device-idle for prior
    // frames; bumping MAX_FRAMES_IN_FLIGHT requires per-frame depth or
    // an extended fence wait. The const_assert at sync.rs:8 enforces
    // the contract at workspace-build time. See #870.
    depth_image_view: vk::ImageView,
    depth_image: vk::Image,
    depth_allocation: Option<vk_alloc::Allocation>,
    pub mesh_registry: MeshRegistry,
    pub texture_registry: TextureRegistry,
    pub scene_buffers: scene_buffer::SceneBuffers,
    pub accel_manager: Option<AccelerationManager>,
    pub cluster_cull: Option<ClusterCullPipeline>,
    /// M29 GPU pre-skinning compute pipeline. `None` when RT is
    /// unsupported (no skinned-BLAS path to feed). Per-skinned-entity
    /// SkinSlots live in `skin_slots`; first-sight registration +
    /// per-frame dispatch + BLAS refit happen inside `draw_frame`.
    pub skin_compute: Option<super::skin_compute::SkinComputePipeline>,
    /// Per-skinned-entity SkinSlot — owns the skinned-vertex output
    /// buffer + per-frame descriptor sets. Populated lazily on first
    /// sight in draw_frame; entries are torn down on Drop. M40 cell
    /// streaming will eventually reclaim slots whose entities are
    /// despawned mid-session.
    pub skin_slots: std::collections::HashMap<
        byroredux_core::ecs::storage::EntityId,
        super::skin_compute::SkinSlot,
    >,
    /// Entities whose `create_slot` call returned `OUT_OF_POOL_MEMORY`
    /// (or otherwise errored) on a prior frame — gate the retry path
    /// in `draw_frame` against this set so a single failure logs one
    /// WARN instead of N (one per frame for the duration of the
    /// bench, observed at 58 WARN / 300 frames pre-fix on Prospector
    /// post-#896 B.2). Cleared whenever any LRU eviction frees a
    /// slot, since capacity opening up means a previously-failing
    /// entity's next attempt could now succeed. `EntityId` is
    /// generational so an entry can't poison a re-issued id. See #900.
    pub failed_skin_slots: std::collections::HashSet<byroredux_core::ecs::storage::EntityId>,
    /// Per-frame counters for the skinned-BLAS coverage path, written
    /// by `draw_frame` and copied into the [`byroredux_core::ecs::
    /// SkinCoverageStats`] resource by [`Self::fill_skin_coverage_stats`].
    /// Reset at the entry of every skinned section in `draw_frame`.
    pub last_skin_coverage_frame: super::skin_compute::SkinCoverageFrame,
    pub ssao: Option<SsaoPipeline>,
    pub composite: Option<CompositePipeline>,
    pub gbuffer: Option<GBuffer>,
    pub svgf: Option<SvgfPipeline>,
    /// TAA resolve pass — reprojects + clamps history to produce the final
    /// HDR image that composite samples. None when allocation fails; the
    /// fallback path feeds raw HDR directly into composite.
    pub taa: Option<TaaPipeline>,
    /// Caustic scatter pass (#321) — per-frame refracted-light accumulator
    /// sampled by the composite pass as a `usampler2D`. Created after SVGF
    /// and before composite so composite's binding 5 can point at its
    /// sampled views. Non-optional: the R32_UINT atomic storage image the
    /// pass needs is universally supported on desktop GPUs.
    pub caustic: Option<CausticPipeline>,
    /// Volumetric lighting pipeline (M55, Tier 8). Phase 1 ships a
    /// no-op clear of the per-frame froxel volume — the plumbing is
    /// in place; visual output lands in subsequent phases (density+
    /// lighting injection, ray-march integration, composite sampling).
    /// `None` when 3D-image allocation or pipeline creation fails on
    /// initial setup; the dispatch site is gated on `Some` so a
    /// failure simply skips the pass for the rest of the session.
    pub volumetrics: Option<VolumetricsPipeline>,
    /// Bloom pyramid pipeline (M58, Tier 8). Reads the scene HDR
    /// after TAA, produces a multi-scale blurred bright-content
    /// texture that composite adds back to `combined` before the
    /// ACES tone-map. `None` when the down/up image-pyramid
    /// allocation fails; composite's bloom binding falls back to a
    /// black dummy so the additive contribution becomes a no-op.
    pub bloom: Option<BloomPipeline>,
    /// Permanent-failure latch for the TAA compute pass. Set on the
    /// first `taa.dispatch` error in a session. When set: the TAA
    /// dispatch is skipped on every subsequent frame and composite's
    /// binding 0 has been rebound to the raw HDR views (via
    /// `CompositePipeline::fall_back_to_raw_hdr`), so the picture
    /// keeps updating without temporal AA instead of freezing on
    /// whatever TAA last wrote. Reset in `recreate_swapchain` since
    /// all pass resources are rebuilt there. See #479.
    pub taa_failed: bool,
    /// Same latch for SVGF — silences warn spam after the first
    /// permanent failure, escalates to `error!` once. Composite keeps
    /// sampling the stale indirect on subsequent frames (rebinding
    /// to raw-indirect is more invasive and deferred until a real
    /// lost-device repro). See #479 SIBLING.
    pub svgf_failed: bool,
    /// Frames remaining in the SVGF temporal-α recovery window. When
    /// non-zero, the SVGF temporal pass uses an elevated α (0.5) for
    /// both color and moments so the noisy current frame gets more
    /// weight after a discontinuity (cell load, weather flip, fast
    /// camera turn). Decremented once per `draw_frame`. The cell
    /// loader / weather system bumps this via
    /// [`Self::signal_temporal_discontinuity`]. Schied 2017 §4 floor
    /// (0.2) takes over once the counter reaches 0. See #674 / DEN-4.
    pub svgf_recovery_frames: u32,
    /// Same latch for the caustic scatter pass. Composite keeps
    /// sampling the stale accumulator; on the failure mode the
    /// caustic contribution is at most one frame stale for the rest
    /// of the session — a visible-but-non-destructive degradation.
    /// See #479 SIBLING.
    pub caustic_failed: bool,
    pipeline_cache: vk::PipelineCache,
    /// Opaque pipeline (depth write on, no blend). Two-sided rendering
    /// uses dynamic `cmd_set_cull_mode` per draw, not a separate
    /// pipeline (#930) — pre-#930 there were two pipelines whose only
    /// difference was static cull state, but with `vk::DynamicState::
    /// CULL_MODE` the static value is ignored, so they compiled to
    /// identical machine code.
    pipeline: vk::Pipeline,
    /// Lazy cache of blended pipelines, keyed by `(src, dst)` from
    /// `NiAlphaProperty.flags` (Gamebryo `AlphaFunction` enum). Each
    /// entry has depth-write disabled, blend on with the exact factor
    /// pair the source NIF authored. See #392 for why this replaced the
    /// earlier 6-pipeline `(opaque|alpha|additive) × (one|two)-sided`
    /// scheme: collapsing 11×11 = 121 possible Gamebryo factor pairs
    /// down to two `Alpha`/`Additive` buckets dropped half the
    /// pipeline-state information for content that depends on it (glass
    /// modulation, premultiplied alpha, etc.). Post-#930: `two_sided`
    /// dropped from the key (same dynamic-CULL_MODE rationale as the
    /// opaque pipeline) — halves the cache size and removes a redundant
    /// `cmd_bind_pipeline` per `two_sided` flip in the alpha-blend pass.
    blend_pipeline_cache: HashMap<(u8, u8), vk::Pipeline>,
    pipeline_ui: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    /// Mesh handle for the fullscreen quad used by UI overlay.
    pub ui_quad_handle: Option<u32>,
    /// Mesh handle for the unit XY quad used by the CPU particle billboard
    /// path (#401). Emitter entities push one DrawCommand per live particle
    /// referencing this handle, with the per-particle position + size baked
    /// into the model matrix and the camera-facing rotation precomputed
    /// CPU-side. The existing instanced batching from #272 collapses all
    /// per-frame particle draws into a single instanced cmd_draw_indexed.
    pub particle_quad_handle: Option<u32>,
    /// Cell-load-time registry of active terrain splat tiles. Parallel
    /// to the mesh / texture registries; maps a tile slot (0..1023) to
    /// its 8 bindless texture indices. Uploaded to the `GpuTerrainTile`
    /// SSBO once per cell load and referenced by fragment shaders via
    /// `(instance.flags >> 16) & 0xFFFF`. Vacant slots are tracked in
    /// a free list. See #470.
    terrain_tiles: Vec<Option<scene_buffer::GpuTerrainTile>>,
    /// LIFO free list of vacant terrain tile slots.
    terrain_tile_free_list: Vec<u32>,
    /// Set when `allocate_terrain_tile` / `free_terrain_tile` mutated
    /// the slab. Checked on the next `draw_frame`, which uploads the
    /// fresh slab through the staging pool into the single DEVICE_LOCAL
    /// SSBO and clears the flag. Pre-#497 this was a per-frame-in-flight
    /// countdown against a HOST_VISIBLE double-buffered SSBO; the buffer
    /// is static until the next cell transition so a single DEVICE_LOCAL
    /// allocation is the correct shape.
    terrain_tiles_dirty: bool,
    /// Persistent scratch buffer reused across frames to stage the 1024
    /// `GpuTerrainTile` slab before upload. Same amortization pattern as
    /// `gpu_instances_scratch` — fresh `Vec::collect()` every dirty
    /// frame was 32 KB × MAX_FRAMES_IN_FLIGHT of heap churn per cell
    /// transition. See #496.
    terrain_tile_scratch: Vec<scene_buffer::GpuTerrainTile>,
    render_pass: vk::RenderPass,
    swapchain_state: SwapchainState,

    pub allocator: Option<SharedAllocator>,

    /// Graphics queue, wrapped in a Mutex for Vulkan-required external
    /// synchronization (VUID-vkQueueSubmit-queue-00893). All queue
    /// submissions (draw_frame, texture/buffer uploads) must lock this.
    pub graphics_queue: Arc<Mutex<vk::Queue>>,
    /// Present queue for vkQueuePresentKHR. When graphics and present
    /// queue families are the same (common on desktop GPUs), this is an
    /// `Arc::clone` of `graphics_queue` — a single Mutex protects the
    /// shared VkQueue handle. When they differ, it's an independent
    /// Mutex wrapping the separate present queue. See #284 (C2-03).
    pub present_queue: Arc<Mutex<vk::Queue>>,
    pub queue_indices: QueueFamilyIndices,
    pub device: ash::Device,
    pub device_caps: device::DeviceCapabilities,
    pub physical_device: vk::PhysicalDevice,
    depth_format: vk::Format,

    surface: vk::SurfaceKHR,
    surface_loader: ash::khr::surface::Instance,

    debug_messenger: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,

    pub instance: ash::Instance,
    pub entry: ash::Entry,
}

impl VulkanContext {
    /// Full Vulkan initialization chain:
    /// 1. Load Vulkan entry points
    /// 2. Create instance + validation layers
    /// 3. Set up debug messenger
    /// 4. Create surface
    /// 5. Pick physical device
    /// 6. Create logical device + queues
    /// 7. Create swapchain
    /// 8. Create render pass
    /// 9. Create framebuffers
    /// 10. Create command pool + command buffers
    /// 11. Create synchronization objects
    pub fn new(
        display_handle: RawDisplayHandle,
        window_handle: RawWindowHandle,
        window_size: [u32; 2],
    ) -> Result<Self> {
        // 1. Entry
        // SAFETY: Loads the Vulkan shared library (libvulkan.so / vulkan-1.dll).
        // Must be called before any other Vulkan function. The Entry must
        // outlive all objects created through it (guaranteed by struct field order).
        let entry = unsafe { ash::Entry::load().context("Failed to load Vulkan loader")? };
        log::info!("Vulkan loader ready");

        // 2. Instance
        let vk_instance = instance::create_instance(&entry, display_handle)?;

        // 3. Debug messenger
        let debug_messenger = if cfg!(debug_assertions) {
            Some(debug::create_debug_messenger(&vk_instance, &entry)?)
        } else {
            None
        };

        // 4. Surface
        let surface_loader = ash::khr::surface::Instance::new(&entry, &vk_instance);
        let vk_surface =
            surface::create_surface(&entry, &vk_instance, display_handle, window_handle)?;

        // 5. Physical device + capability probe
        let (physical_device, queue_indices, device_caps) =
            device::pick_physical_device(&vk_instance, &surface_loader, vk_surface)?;

        // 6. Query supported depth format
        let depth_format = find_depth_format(&vk_instance, physical_device)?;

        // 7. Logical device + queues (enables RT extensions when available)
        let (device, raw_graphics_queue, raw_present_queue) = device::create_logical_device(
            &vk_instance,
            physical_device,
            queue_indices,
            &device_caps,
        )?;
        let graphics_queue = Arc::new(Mutex::new(raw_graphics_queue));
        // When graphics and present use the same queue family, share the
        // same Mutex to avoid two locks wrapping one VkQueue handle (#284).
        let present_queue = if queue_indices.graphics == queue_indices.present {
            Arc::clone(&graphics_queue)
        } else {
            Arc::new(Mutex::new(raw_present_queue))
        };

        // 7. GPU allocator (buffer_device_address required for RT acceleration structures)
        let gpu_allocator = allocator::create_allocator(
            &vk_instance,
            &device,
            physical_device,
            device_caps.ray_query_supported,
        )?;

        // 8. Swapchain
        let swapchain_state = swapchain::create_swapchain(
            &vk_instance,
            &device,
            physical_device,
            &surface_loader,
            vk_surface,
            queue_indices,
            window_size,
            vk::SwapchainKHR::null(), // no old swapchain on initial creation
        )?;

        // 9. Depth resources
        let (depth_image, depth_image_view, depth_allocation) = create_depth_resources(
            &device,
            &gpu_allocator,
            swapchain_state.extent,
            depth_format,
        )?;

        // 10. Main render pass: 6 color attachments (HDR + G-buffer +
        // raw_indirect + albedo) + depth.
        let render_pass = create_render_pass(
            &device,
            HDR_FORMAT,
            NORMAL_FORMAT,
            MOTION_FORMAT,
            MESH_ID_FORMAT,
            RAW_INDIRECT_FORMAT,
            ALBEDO_FORMAT,
            depth_format,
        )?;

        // 10. Command pools: one for per-frame draw commands (RESET_COMMAND_BUFFER),
        //     one for one-time upload/transfer commands (separate pool to avoid
        //     contention — Vulkan requires external sync on VkCommandPool).
        let command_pool = create_command_pool(&device, queue_indices.graphics)?;
        let transfer_pool = create_transfer_pool(&device, queue_indices.graphics)?;

        // Persistent fence for one-time submits (#302). Created unsignaled;
        // every use calls reset_fences then wait_for_fences.
        let transfer_fence = Arc::new(Mutex::new(unsafe {
            device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .context("create transfer fence")?
        }));

        // 11. Texture registry with checkerboard fallback.
        // Bindless array size is driven by the device limit (query in
        // device.rs, clamped at the R16_UINT mesh_id ceiling) instead of a
        // hardcoded 1024 that large cells would silently overflow. See #425.
        let mut texture_registry = TextureRegistry::new(
            &device,
            &gpu_allocator,
            swapchain_state.images.len() as u32,
            device_caps.max_bindless_sampled_images,
            device_caps.max_sampler_anisotropy,
        )?;
        let checkerboard = super::texture::generate_checkerboard(256, 256, 32);
        // One-shot 256×256 fallback — `None` pool skips the overhead of
        // the first pool entry that would otherwise linger for the rest
        // of the session.
        let fallback_texture = Texture::from_rgba(
            &device,
            &gpu_allocator,
            &graphics_queue,
            transfer_pool,
            256,
            256,
            &checkerboard,
            texture_registry.shared_sampler,
            None,
        )?;
        texture_registry.set_fallback(&device, fallback_texture)?;

        // 12. Scene buffers (light SSBO + camera UBO + optional TLAS, descriptor set 1)
        let scene_buffers = scene_buffer::SceneBuffers::new(
            &device,
            &gpu_allocator,
            device_caps.ray_query_supported,
        )?;
        // #921 — seed slot-0 identity in the DEVICE bone palette so the
        // first frame's binding-12 read (which targets the OTHER frame
        // slot, never written yet) doesn't surface uninitialized memory
        // through the vertex shader's bone fetch.
        scene_buffers.seed_identity_bones(&device, &graphics_queue, transfer_pool)?;

        // 12b. Acceleration manager (RT only) — build empty TLAS so descriptors are valid
        let mut scene_buffers = scene_buffers;
        let accel_manager = if device_caps.ray_query_supported {
            let mut accel = AccelerationManager::new(
                &vk_instance,
                &device,
                physical_device,
                device_caps.min_accel_struct_scratch_offset_alignment,
            );
            // Build an empty TLAS per frame-in-flight slot via one-time command
            // buffers so all descriptor sets have a valid acceleration structure
            // from frame 0. Each build blocks until complete (fence wait inside
            // with_one_time_commands), so no overlap between builds.
            let empty_draws: Vec<DrawCommand> = Vec::new();
            let empty_map: Vec<Option<u32>> = Vec::new();
            for f in 0..MAX_FRAMES_IN_FLIGHT {
                super::texture::with_one_time_commands_reuse_fence(
                    &device,
                    &graphics_queue,
                    transfer_pool,
                    &transfer_fence,
                    |cmd| unsafe {
                        accel
                            .build_tlas(&device, &gpu_allocator, cmd, &empty_draws, &empty_map, f)
                            .context("initial empty TLAS build")
                    },
                )?;
                if let Some(tlas_handle) = accel.tlas_handle(f) {
                    scene_buffers.write_tlas(&device, f, tlas_handle);
                }
            }
            Some(accel)
        } else {
            None
        };

        // 12b. Pipeline cache (load from disk if available).
        // Created before ANY pipeline-create call so every compile
        // writes into the shared cache — warm-start second-launch
        // skips most driver IR compilation (#426). The on-disk
        // header is validated against the running device's
        // vendorID / deviceID / pipelineCacheUUID before the bytes
        // reach the driver — defense in depth against tampered or
        // post-upgrade-stale files (SAFE-11 / #91).
        let pipeline_cache = load_or_create_pipeline_cache(&vk_instance, physical_device, &device)?;

        // 12c. Cluster cull compute pipeline (light culling)
        let cluster_cull = match ClusterCullPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            scene_buffers.light_buffers(),
            scene_buffers.camera_buffers(),
            scene_buffers.light_buffer_size(),
            scene_buffers.camera_buffer_size(),
        ) {
            Ok(cc) => {
                // Write cluster buffer references into scene descriptor sets.
                for f in 0..MAX_FRAMES_IN_FLIGHT {
                    scene_buffers.write_cluster_buffers(
                        &device,
                        f,
                        cc.grid_buffer(f),
                        cc.grid_buffer_size(),
                        cc.index_buffer(f),
                        cc.index_buffer_size(),
                    );
                }
                Some(cc)
            }
            Err(e) => {
                log::warn!(
                    "Cluster cull pipeline creation failed: {e} — falling back to all-lights loop"
                );
                None
            }
        };

        // 12d. Skin compute pipeline (M29 Phase 2). RT-required: when
        // ray queries aren't supported there's no BLAS refit path to
        // feed, so the pipeline is dead weight. Created with the max
        // slot ceiling matching `MAX_TOTAL_BONES / MAX_BONES_PER_MESH
        // = 32` skinned meshes — same ceiling the bone-palette upload
        // path enforces in `build_render_data`. Buffer bindings are
        // deferred to per-dispatch (cell-transition robustness).
        let skin_compute = if device_caps.ray_query_supported {
            // See module-level `SKIN_MAX_SLOTS` const for the rationale.
            match super::skin_compute::SkinComputePipeline::new(
                &device,
                pipeline_cache,
                SKIN_MAX_SLOTS,
            ) {
                Ok(sc) => Some(sc),
                Err(e) => {
                    log::warn!(
                        "Skin compute pipeline creation failed: {e} — \
                         skinned RT shadows disabled (raster inline-skinning unaffected)"
                    );
                    None
                }
            }
        } else {
            None
        };

        // 14. Graphics pipeline (with depth test + descriptor set layouts for set 0 + set 1)
        let pipelines = pipeline::create_triangle_pipeline(
            &device,
            render_pass,
            swapchain_state.extent,
            texture_registry.descriptor_set_layout,
            scene_buffers.descriptor_set_layout,
            pipeline_cache,
        )?;

        // 15. UI overlay pipeline (no depth, alpha blend, passthrough shaders)
        let pipeline_ui = pipeline::create_ui_pipeline(
            &device,
            render_pass,
            swapchain_state.extent,
            pipelines.layout,
            pipeline_cache,
        )?;

        // 14a. SSAO pipeline (reads depth buffer after render pass)
        let ssao = match SsaoPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            depth_image_view,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(s) => {
                // Transition AO image from UNDEFINED to SHADER_READ_ONLY_OPTIMAL
                // so the first frame's fragment shader sees a valid layout (1.0 =
                // no occlusion). Without this, sampling UNDEFINED is UB.
                if let Err(e) =
                    unsafe { s.initialize_ao_images(&device, &graphics_queue, transfer_pool) }
                {
                    log::warn!("SSAO AO image init failed: {e}");
                }
                for f in 0..MAX_FRAMES_IN_FLIGHT {
                    scene_buffers.write_ao_texture(&device, f, s.ao_image_views[f], s.ao_sampler);
                }
                Some(s)
            }
            Err(e) => {
                log::warn!("SSAO pipeline creation failed: {e} — no ambient occlusion");
                None
            }
        };

        // 14a-bis. Volumetrics pipeline (M55 Phase 1 — no-op clear).
        // Allocates the per-frame-in-flight 3D froxel volumes
        // (160×90×128 RGBA16F, ~14 MiB / slot) and the compute
        // pipeline that clears them. Subsequent phases will replace
        // the clear with real density + lighting injection and
        // ray-march integration. Skipped silently on failure — the
        // dispatch site is gated on `Some` so the rest of the
        // pipeline stays unaffected.
        let volumetrics = match VolumetricsPipeline::new(&device, &gpu_allocator, pipeline_cache) {
            Ok(v) => {
                if let Err(e) =
                    unsafe { v.initialize_layouts(&device, &graphics_queue, transfer_pool) }
                {
                    log::warn!("Volumetrics froxel layout init failed: {e}");
                }
                Some(v)
            }
            Err(e) => {
                log::warn!("Volumetrics pipeline creation failed: {e} — no volumetric lighting");
                None
            }
        };

        // 14. Mesh registry (empty — meshes uploaded by the application)
        let mesh_registry = MeshRegistry::new();

        // 14b. G-buffer: all auxiliary attachments (normal, motion, mesh_id,
        // raw_indirect, albedo). Created BEFORE composite because composite's
        // descriptor sets reference the raw_indirect + albedo views.
        let gbuffer = Some(GBuffer::new(
            &device,
            &gpu_allocator,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        )?);
        let gbuffer_ref = gbuffer.as_ref().expect("gbuffer must exist");

        // Transition all G-buffer images from UNDEFINED to
        // SHADER_READ_ONLY_OPTIMAL so the "previous frame" slot is in a
        // valid layout on the very first frame (SVGF temporal pass binds
        // the previous frame's mesh_id/motion/raw_indirect for sampling).
        if let Err(e) =
            unsafe { gbuffer_ref.initialize_layouts(&device, &graphics_queue, transfer_pool) }
        {
            log::warn!("G-buffer layout init failed: {e}");
        }

        // Collect G-buffer views up-front so svgf, composite, and main
        // framebuffer creation can reference them.
        let n_frames = MAX_FRAMES_IN_FLIGHT;
        let raw_indirect_views: Vec<vk::ImageView> = (0..n_frames)
            .map(|i| gbuffer_ref.raw_indirect_view(i))
            .collect();
        let motion_views_seed: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.motion_view(i)).collect();
        let mesh_id_views_seed: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.mesh_id_view(i)).collect();
        // #650 / SH-5 — SVGF needs the GBuffer normal attachments too
        // for the 2×2 consistency loop's normal-cone rejection. Pulled
        // up from below the SVGF init so the new binding is wired at
        // pipeline-creation time.
        let normal_views_for_svgf: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.normal_view(i)).collect();
        let albedo_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.albedo_view(i)).collect();

        // 14b2. SVGF temporal denoiser — reads raw_indirect + motion +
        // mesh_id from the G-buffer, writes accumulated_indirect images
        // that the composite pass will sample in place of raw_indirect.
        // Created before composite so composite's descriptor sets can
        // reference SVGF's indirect_history views.
        let mut svgf = match SvgfPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            &raw_indirect_views,
            &motion_views_seed,
            &mesh_id_views_seed,
            &normal_views_for_svgf,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("SVGF pipeline creation failed: {e} — falling back to raw indirect");
                None
            }
        };
        // Transition history images UNDEFINED → GENERAL so first dispatch
        // and first descriptor sampling see a valid layout.
        if let Some(ref s) = svgf {
            if let Err(e) = unsafe { s.initialize_layouts(&device, &graphics_queue, transfer_pool) }
            {
                log::warn!("SVGF layout init failed: {e} — disabling SVGF");
                // Destroy partially-initialized pipeline.
                if let Some(mut pipe) = svgf.take() {
                    unsafe { pipe.destroy(&device, &gpu_allocator) };
                }
            }
        }

        // Composite samples SVGF's accumulated indirect (GENERAL layout)
        // when SVGF is available, else falls back to raw G-buffer indirect
        // (SHADER_READ_ONLY_OPTIMAL layout).
        let (composite_indirect_views, indirect_is_general): (Vec<vk::ImageView>, bool) =
            if let Some(ref s) = svgf {
                ((0..n_frames).map(|i| s.indirect_view(i)).collect(), true)
            } else {
                (raw_indirect_views.clone(), false)
            };

        // 14b-bis. Caustic scatter pass (#321). Sits between SVGF and
        // composite so composite's binding 5 can sample its R32_UINT
        // accumulator. The compute shader fires ray queries against the
        // TLAS and uses the full set of per-FIF scene buffers, so all of
        // those need to exist (they do — this runs after SceneBuffers and
        // AccelerationManager are built).
        let normal_views_seed: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.normal_view(i)).collect();
        let mut caustic: Option<CausticPipeline> = match CausticPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            depth_image_view,
            &normal_views_seed,
            &mesh_id_views_seed,
            scene_buffers.light_buffers(),
            scene_buffers.light_buffer_size(),
            scene_buffers.camera_buffers(),
            scene_buffers.camera_buffer_size(),
            scene_buffers.instance_buffers(),
            scene_buffers.instance_buffer_size(),
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(c) => Some(c),
            Err(e) => {
                return Err(anyhow::anyhow!("Caustic pipeline creation failed: {e}"));
            }
        };
        if let Some(ref c) = caustic {
            if let Err(e) = unsafe { c.initialize_layouts(&device, &graphics_queue, transfer_pool) }
            {
                log::warn!("Caustic layout init failed: {e} — disabling caustic");
                if let Some(mut pipe) = caustic.take() {
                    unsafe { pipe.destroy(&device, &gpu_allocator) };
                }
            }
        }
        // Build caustic view list for composite. When caustic is disabled
        // we reuse the mesh_id views as a harmless placeholder (composite
        // samples with texelFetch as usampler2D; R16_UINT is narrower than
        // R32_UINT but SPIR-V's usampler2D reads undefined-for-bits-above-
        // format anyway, yielding small values and ~zero caustic). This
        // avoids a dedicated dummy image while keeping the descriptor slot
        // populated.
        let caustic_views: Vec<vk::ImageView> = match caustic {
            Some(ref c) => (0..n_frames).map(|i| c.sampled_view(i)).collect(),
            None => mesh_id_views_seed.clone(),
        };

        // 14c. Composite pipeline: owns HDR intermediates + tone-map pass.
        // Its descriptor sets sample HDR (owned by composite), indirect
        // (from SVGF or raw G-buffer), and albedo (G-buffer).
        // Volumetric views (M55 Phase 3) — composite samples the
        // pre-integrated `(∫inscatter, T_cum)` volume per fragment
        // with one sampler3D tap. Hard requirement: composite's
        // binding 6 is `sampler3D`, so a None volumetrics pipeline
        // can't be papered over with a 2D fallback view. If pipeline
        // creation failed earlier, refuse to build composite. The
        // 14 MiB × 2 / slot 3D-image allocation is universally
        // supported on RT-class GPUs, so this only fires under exotic
        // hardware / driver pathologies.
        let volumetric_views: Vec<vk::ImageView> = match volumetrics.as_ref() {
            Some(v) => v.integrated_views(),
            None => {
                return Err(anyhow::anyhow!(
                    "Volumetric pipeline failed to initialize — composite \
                     requires the integrated 3D froxel volume for binding 6 \
                     (M55 Phase 3). Check earlier 'volumetrics' WARN logs."
                ));
            }
        };

        // 14b-bis. Bloom pipeline (M58 Phase 1). Allocates the down/up
        // mip pyramids — does NOT need any input views at this stage
        // because the scene HDR view is rebound per-frame in
        // `dispatch()`. Constructed before composite so we can pass
        // its output views into composite's binding 7. Soft-fail
        // skipped: composite needs SOME view for binding 7, and the
        // smaller image-view allocations are universally supported,
        // so if this fails we fall through hard like volumetrics.
        let bloom = match BloomPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            swapchain_state.extent,
        ) {
            Ok(b) => {
                if let Err(e) =
                    unsafe { b.initialize_layouts(&device, &graphics_queue, transfer_pool) }
                {
                    log::warn!("Bloom pyramid layout init failed: {e}");
                }
                Some(b)
            }
            Err(e) => {
                log::warn!("Bloom pipeline creation failed: {e} — no bloom this session");
                None
            }
        };
        let bloom_views: Vec<vk::ImageView> = match bloom.as_ref() {
            Some(b) => b.output_views(),
            None => {
                return Err(anyhow::anyhow!(
                    "Bloom pipeline failed to initialize — composite \
                     requires the bloom output view for binding 7 (M58). \
                     Check earlier 'bloom' WARN logs."
                ));
            }
        };
        let mut composite = match CompositePipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            swapchain_state.format.format,
            &swapchain_state.image_views,
            &composite_indirect_views,
            indirect_is_general,
            &albedo_views,
            depth_image_view,
            &caustic_views,
            &volumetric_views,
            &bloom_views,
            texture_registry.descriptor_set_layout,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(c) => Some(c),
            Err(e) => {
                return Err(anyhow::anyhow!("Composite pipeline creation failed: {e}"));
            }
        };
        // Snapshot composite's HDR image views into an owned Vec so the
        // subsequent &mut borrow of `composite` (for TAA rewire) doesn't
        // conflict with the main-framebuffer creation below.
        let hdr_views_owned: Vec<vk::ImageView> = composite
            .as_ref()
            .expect("composite must exist after construction")
            .hdr_image_views
            .clone();

        // 14d. TAA resolve pass — needs the composite's HDR views (created
        // above) as its "current HDR" input, plus per-FIF motion + mesh_id.
        // If creation succeeds, composite's HDR descriptor is rewired to
        // sample TAA's output; otherwise we keep the raw HDR path.
        let mut taa = match TaaPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            &hdr_views_owned,
            &motion_views_seed,
            &mesh_id_views_seed,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(t) => Some(t),
            Err(e) => {
                log::warn!("TAA pipeline creation failed: {e} — falling back to raw HDR");
                None
            }
        };
        if let Some(ref t) = taa {
            if let Err(e) = unsafe { t.initialize_layouts(&device, &graphics_queue, transfer_pool) }
            {
                log::warn!("TAA layout init failed: {e} — disabling TAA");
                if let Some(mut pipe) = taa.take() {
                    unsafe { pipe.destroy(&device, &gpu_allocator) };
                }
            }
        }
        // Swap composite's HDR binding to TAA output so tone-map samples
        // the anti-aliased image. When TAA is disabled composite keeps its
        // original raw-HDR descriptors.
        if let (Some(ref t), Some(ref mut c)) = (taa.as_ref(), composite.as_mut()) {
            let taa_views: Vec<vk::ImageView> = (0..n_frames).map(|i| t.output_view(i)).collect();
            c.rebind_hdr_views(&device, &taa_views, vk::ImageLayout::GENERAL);
        }

        // 15. Main framebuffers: one per frame-in-flight slot, binding that
        // slot's HDR + normal + motion + mesh_id + raw_indirect + albedo
        // views + shared depth view.
        let hdr_views: &[vk::ImageView] = &hdr_views_owned;
        let normal_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.normal_view(i)).collect();
        let motion_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.motion_view(i)).collect();
        let mesh_id_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.mesh_id_view(i)).collect();
        let framebuffers = create_main_framebuffers(
            &device,
            render_pass,
            hdr_views,
            &normal_views,
            &motion_views,
            &mesh_id_views,
            &raw_indirect_views,
            &albedo_views,
            depth_image_view,
            swapchain_state.extent,
        )?;

        // 16. Command buffers — one per frame-in-flight (NOT per swapchain
        // image). The in_flight fence is per-frame, so tying command buffer
        // reuse to the same index makes the fence → cmd-buf relationship
        // direct and obvious. See #259.
        let command_buffers =
            allocate_command_buffers(&device, command_pool, sync::MAX_FRAMES_IN_FLIGHT)?;

        // 17. Sync objects
        let frame_sync = sync::create_sync_objects(&device, swapchain_state.images.len())?;

        log::info!("Vulkan context fully initialized");

        Ok(Self {
            entry,
            instance: vk_instance,
            debug_messenger,
            surface_loader,
            surface: vk_surface,
            physical_device,
            depth_format,
            device,
            device_caps,
            queue_indices,
            graphics_queue,
            present_queue,
            swapchain_state,
            allocator: Some(gpu_allocator),
            render_pass,
            pipeline_cache,
            pipeline: pipelines.opaque,
            blend_pipeline_cache: HashMap::new(),
            pipeline_ui,
            pipeline_layout: pipelines.layout,
            ui_quad_handle: None,
            particle_quad_handle: None,
            terrain_tiles: vec![None; scene_buffer::MAX_TERRAIN_TILES],
            // Free list seeded with every slot in reverse order so
            // `pop()` returns slots in ascending order (deterministic
            // test behaviour).
            terrain_tile_free_list: (0..scene_buffer::MAX_TERRAIN_TILES as u32).rev().collect(),
            terrain_tiles_dirty: false,
            terrain_tile_scratch: Vec::new(),
            mesh_registry,
            texture_registry,
            scene_buffers,
            accel_manager,
            cluster_cull,
            skin_compute,
            skin_slots: std::collections::HashMap::new(),
            failed_skin_slots: std::collections::HashSet::new(),
            last_skin_coverage_frame: super::skin_compute::SkinCoverageFrame::default(),
            ssao,
            composite,
            gbuffer,
            svgf,
            taa,
            caustic,
            volumetrics,
            bloom,
            taa_failed: false,
            svgf_failed: false,
            svgf_recovery_frames: 0,
            caustic_failed: false,
            depth_allocation: Some(depth_allocation),
            depth_image,
            depth_image_view,
            framebuffers,
            command_pool,
            transfer_pool,
            transfer_fence,
            command_buffers,
            frame_sync,
            current_frame: 0,
            frame_counter: 0,
            render_debug_flags: parse_render_debug_flags_env(),
            // Initialize to identity; first frame will overwrite with current
            // viewProj so motion vector is zero on the first frame.
            prev_view_proj: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            gpu_instances_scratch: Vec::new(),
            batches_scratch: Vec::new(),
            indirect_draws_scratch: Vec::new(),
            screenshot_requested: Arc::new(AtomicBool::new(false)),
            screenshot_result: Arc::new(Mutex::new(None)),
            screenshot_staging: None,
            screenshot_pending_readback: false,
        })
    }

    /// Synchronously drain every deferred-destroy queue across the
    /// three resource registries (BLAS, mesh buffers, textures),
    /// regardless of per-entry countdowns / frame-id aging. Intended
    /// for the App's window-close shutdown sweep — after
    /// `cell_loader::unload_cell` has populated the queues but before
    /// `self.renderer.take()` runs `VulkanContext::Drop`. See
    /// #732 / LIFE-H2.
    ///
    /// Issues a `device_wait_idle` first so the queued resources can't
    /// be referenced by any in-flight command buffer. After this call
    /// the deferred-destroy queue counters reach zero, the per-entry
    /// `GpuBuffer` / `Texture` structs are dropped (releasing each
    /// entry's `Arc<Mutex<Allocator>>` clone), and the gpu-allocator's
    /// internal slabs are returned to its free-list. The destroy chain
    /// inside `Drop` would do the same drain inline; calling it
    /// explicitly here moves the queue release out of the
    /// `if let Some(ref alloc)` block in `Drop` so the intent is
    /// visible at the App's shutdown call site, and gives the
    /// allocator unwrap a chance at a smaller `Arc` strong count.
    ///
    /// No-op when `accel_manager` or the allocator are absent (headless
    /// / pre-init paths).
    pub fn flush_pending_destroys(&mut self) {
        let Some(allocator) = self.allocator.clone() else {
            return;
        };
        // SAFETY: `device_wait_idle` settles all in-flight command
        // buffers — required precondition for both
        // `AccelerationManager::drain_pending_destroys` and the texture
        // / mesh registry drains.
        unsafe {
            let _ = self.device.device_wait_idle();
        }
        if let Some(accel) = self.accel_manager.as_mut() {
            unsafe {
                accel.drain_pending_destroys(&self.device, &allocator);
            }
        }
        self.mesh_registry
            .drain_deferred_destroy(&self.device, &allocator);
        self.texture_registry
            .drain_pending_destroys(&self.device, &allocator);
    }

    /// Run a closure in a one-time-submit command buffer, reusing the
    /// persistent transfer fence (#302). Prefer this over the free-function
    /// `with_one_time_commands` to avoid per-call fence create/destroy.
    pub fn with_transfer_commands<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(vk::CommandBuffer) -> Result<()>,
    {
        super::texture::with_one_time_commands_reuse_fence(
            &self.device,
            &self.graphics_queue,
            self.transfer_pool,
            &self.transfer_fence,
            f,
        )
    }

    /// Look up the cached blended pipeline for a given Gamebryo
    /// `(src, dst)` factor pair, or create + cache it on first use.
    /// The cache is keyed by the raw `NiAlphaProperty.flags` nibbles,
    /// so identical factor pairs across different materials share one
    /// pipeline. Two-sided rendering uses dynamic `cmd_set_cull_mode`
    /// per draw — see [`crate::vulkan::pipeline::PipelineKey`] (#930).
    ///
    /// Returns the cached pipeline on cache hit (no allocation, no
    /// device call). On cache miss, creates a pipeline through
    /// [`pipeline::create_blend_pipeline`] and inserts it.
    ///
    /// Pipelines created here are tied to the current render pass and
    /// must be destroyed and re-created on swapchain recreate
    /// ([`recreate_swapchain`](Self::recreate_swapchain)).
    pub fn get_or_create_blend_pipeline(&mut self, src: u8, dst: u8) -> Result<vk::Pipeline> {
        let key = (src, dst);
        if let Some(&pipe) = self.blend_pipeline_cache.get(&key) {
            return Ok(pipe);
        }
        let pipe = pipeline::create_blend_pipeline(
            &self.device,
            self.render_pass,
            self.swapchain_state.extent,
            self.pipeline_cache,
            self.pipeline_layout,
            src,
            dst,
        )?;
        self.blend_pipeline_cache.insert(key, pipe);
        Ok(pipe)
    }

    /// Get a handle for requesting screenshots from outside the render loop.
    pub fn screenshot_handle(&self) -> ScreenshotHandle {
        ScreenshotHandle {
            requested: Arc::clone(&self.screenshot_requested),
            result: Arc::clone(&self.screenshot_result),
        }
    }

    /// Signal a temporal discontinuity (cell load, weather flip, fast
    /// camera turn) so the SVGF temporal pass uses an elevated α for
    /// `frames` upcoming frames. The current frame and `frames - 1`
    /// after it run with α = 0.5 (color + moments) instead of the
    /// 0.2 steady-state floor; this gives the freshly-noisy current
    /// frame more weight while history variance settles.
    ///
    /// Calls accumulate via `max` — bumping by 5 mid-recovery extends
    /// the window rather than truncating it. Schied 2017 §4 / #674.
    ///
    /// Also resets the TAA history-reset window so TAA's resolved
    /// indirect doesn't keep trailing the SVGF recovery — without
    /// the paired reset TAA would ghost newly-streamed geometry for
    /// ~30 frames at 60 FPS while SVGF's elevated-α window already
    /// faded. See #801.
    pub fn signal_temporal_discontinuity(&mut self, frames: u32) {
        self.svgf_recovery_frames = self.svgf_recovery_frames.max(frames);
        if let Some(ref mut taa) = self.taa {
            taa.signal_history_reset();
        }
    }

    /// Snapshot every persistent CPU-side scratch `Vec` owned by the
    /// renderer (R6). The rows land on the [`ScratchTelemetry`]
    /// resource via [`crate::vulkan::context::VulkanContext`] each
    /// frame and are surfaced by the `ctx.scratch` console command.
    ///
    /// **Maintenance**: every persistent `Vec` scratch declared in this
    /// crate must show up here. Adding a new scratch field on
    /// `VulkanContext` (or its sub-managers) without a row added below
    /// reintroduces the pre-R6 blind spot where scratches grow with
    /// zero observability.
    ///
    /// Reuses the caller's `Vec` to avoid a per-frame allocation in
    /// the telemetry path itself. Capacity stabilises at the number of
    /// declared scratches after the first frame.
    pub fn fill_scratch_telemetry(&self, rows: &mut Vec<byroredux_core::ecs::ScratchRow>) {
        use byroredux_core::ecs::ScratchRow;
        use std::mem::size_of;

        rows.clear();
        rows.push(ScratchRow {
            name: "gpu_instances_scratch",
            len: self.gpu_instances_scratch.len(),
            capacity: self.gpu_instances_scratch.capacity(),
            elem_size_bytes: size_of::<scene_buffer::GpuInstance>(),
        });
        rows.push(ScratchRow {
            name: "batches_scratch",
            len: self.batches_scratch.len(),
            capacity: self.batches_scratch.capacity(),
            elem_size_bytes: size_of::<draw::DrawBatch>(),
        });
        rows.push(ScratchRow {
            name: "indirect_draws_scratch",
            len: self.indirect_draws_scratch.len(),
            capacity: self.indirect_draws_scratch.capacity(),
            elem_size_bytes: size_of::<vk::DrawIndexedIndirectCommand>(),
        });
        rows.push(ScratchRow {
            name: "terrain_tile_scratch",
            len: self.terrain_tile_scratch.len(),
            capacity: self.terrain_tile_scratch.capacity(),
            elem_size_bytes: size_of::<scene_buffer::GpuTerrainTile>(),
        });
        if let Some(accel) = &self.accel_manager {
            let (len, capacity) = accel.tlas_instances_scratch_telemetry();
            rows.push(ScratchRow {
                name: "tlas_instances_scratch",
                len,
                capacity,
                elem_size_bytes: size_of::<vk::AccelerationStructureInstanceKHR>(),
            });
        } else {
            rows.push(ScratchRow {
                name: "tlas_instances_scratch",
                len: 0,
                capacity: 0,
                elem_size_bytes: size_of::<vk::AccelerationStructureInstanceKHR>(),
            });
        }
    }

    /// Snapshot the skinned-BLAS coverage counters from the last
    /// `draw_frame` invocation. Filled into the
    /// [`byroredux_core::ecs::SkinCoverageStats`] resource each frame by
    /// the engine binary, alongside `fill_scratch_telemetry`, and
    /// surfaced by the `skin.coverage` console command.
    ///
    /// The `failed_entity_ids` snapshot caps at 16 IDs to keep the
    /// resource cheap to copy; the full count is in `slots_failed`. IDs
    /// are sampled in HashSet iteration order (non-deterministic) — fine
    /// for diagnostic spot-checks via `byro-dbg`, not a stable
    /// regression key.
    pub fn fill_skin_coverage_stats(&self, stats: &mut byroredux_core::ecs::SkinCoverageStats) {
        let f = self.last_skin_coverage_frame;
        stats.dispatches_total = f.dispatches_total;
        stats.first_sight_attempted = f.first_sight_attempted;
        stats.first_sight_succeeded = f.first_sight_succeeded;
        stats.refits_attempted = f.refits_attempted;
        stats.refits_succeeded = f.refits_succeeded;
        stats.slots_active = self.skin_slots.len() as u32;
        stats.slot_pool_capacity = if self.skin_compute.is_some() {
            SKIN_MAX_SLOTS
        } else {
            0
        };
        stats.slots_failed = self.failed_skin_slots.len() as u32;
        stats.failed_entity_ids.clear();
        for &eid in self.failed_skin_slots.iter().take(16) {
            stats.failed_entity_ids.push(eid);
        }
    }

    // draw_frame is in draw.rs
    // build_blas_for_mesh, register_ui_quad, swapchain_extent, log_memory_usage are in resources.rs
    // recreate_swapchain is in resize.rs
}

// Method implementations split across submodules:
mod draw;
mod helpers;
mod resize;
mod resources;
mod screenshot;

impl Drop for VulkanContext {
    fn drop(&mut self) {
        // SAFETY: device_wait_idle ensures all GPU work is complete before
        // destroying resources. Destruction follows reverse-creation order
        // to satisfy Vulkan object lifetime requirements.
        unsafe {
            let _ = self.device.device_wait_idle();

            self.destroy_screenshot_staging();

            self.frame_sync.destroy(&self.device);
            // Destroy persistent transfer fence (#302). device_wait_idle
            // above ensures it's not signaled in-flight.
            {
                let fence = *self
                    .transfer_fence
                    .lock()
                    .expect("transfer fence lock poisoned");
                self.device.destroy_fence(fence, None);
            }
            self.device.destroy_command_pool(self.transfer_pool, None);
            self.device
                .free_command_buffers(self.command_pool, &self.command_buffers);
            self.device.destroy_command_pool(self.command_pool, None);
            destroy_main_framebuffers(&self.device, &mut self.framebuffers);
            // Destroy texture registry, scene buffers, and acceleration structures.
            if let Some(ref alloc) = self.allocator {
                self.texture_registry.destroy(&self.device, alloc);
                self.scene_buffers.destroy(&self.device, alloc);
                // M29 — destroy SkinSlots BEFORE the SkinComputePipeline
                // because slots own descriptor sets allocated from the
                // pipeline's descriptor pool. Pool destruction implicitly
                // frees the sets but the FREE_DESCRIPTOR_SET flag means
                // we should explicitly free them through the pipeline
                // first to keep the validation layer quiet. The ordering
                // also matches the static `accel_manager` teardown
                // pattern (skinned_blas before pipeline scratch buffers).
                if let Some(ref skin) = self.skin_compute {
                    let slots = std::mem::take(&mut self.skin_slots);
                    for (_eid, slot) in slots {
                        skin.destroy_slot(&self.device, alloc, slot);
                    }
                }
                if let Some(ref mut accel) = self.accel_manager {
                    // Drop per-skinned-entity BLAS before the manager's
                    // own destroy() runs — the BlasEntry buffer + accel
                    // structure are owned by the manager but not in
                    // `blas_entries`, so manager.destroy() wouldn't see
                    // them. Walk + drop here.
                    for eid in accel.skinned_blas_entities() {
                        accel.drop_skinned_blas(eid);
                    }
                    accel.destroy(&self.device, alloc);
                }
                if let Some(ref mut cc) = self.cluster_cull {
                    cc.destroy(&self.device, alloc);
                }
                if let Some(ref mut sc) = self.skin_compute {
                    sc.destroy(&self.device);
                }
                if let Some(ref mut ssao) = self.ssao {
                    ssao.destroy(&self.device, alloc);
                }
                if let Some(ref mut composite) = self.composite {
                    composite.destroy(&self.device, alloc);
                }
                if let Some(ref mut caustic) = self.caustic {
                    caustic.destroy(&self.device, alloc);
                }
                if let Some(ref mut vol) = self.volumetrics {
                    vol.destroy(&self.device, alloc);
                }
                if let Some(ref mut b) = self.bloom {
                    b.destroy(&self.device, alloc);
                }
                if let Some(ref mut svgf) = self.svgf {
                    svgf.destroy(&self.device, alloc);
                }
                if let Some(ref mut taa) = self.taa {
                    taa.destroy(&self.device, alloc);
                }
                if let Some(ref mut gbuffer) = self.gbuffer {
                    gbuffer.destroy(&self.device, alloc);
                }
            }

            // Destroy depth resources before the allocator.
            // Helper enforces order: view → image → free allocation. The
            // image must be destroyed while its bound memory is still
            // valid (Vulkan spec VUID-vkFreeMemory-memory-00677). Same
            // helper used by recreate_swapchain — see #33 / R-10.
            if let Some(ref allocator) = self.allocator {
                destroy_depth_resources(
                    &self.device,
                    allocator,
                    &mut self.depth_image_view,
                    &mut self.depth_image,
                    &mut self.depth_allocation,
                );
            }

            destroy_render_pass_pipelines(
                &self.device,
                &mut self.pipeline,
                &mut self.blend_pipeline_cache,
                &mut self.pipeline_ui,
            );
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            // Meshes after pipelines: pipelines consume meshes at draw time,
            // so meshes should outlive the pipelines that reference them.
            if let Some(ref alloc) = self.allocator {
                self.mesh_registry.destroy_all(&self.device, alloc);
            }
            // Save pipeline cache to disk before destroying.
            save_pipeline_cache(&self.device, self.pipeline_cache);
            self.device
                .destroy_pipeline_cache(self.pipeline_cache, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.swapchain_state.destroy(&self.device);
            // Drop the allocator before destroying the device.
            // take() extracts from Option, then try_unwrap gets the inner
            // Mutex if we hold the last Arc, then into_inner gives us the
            // Allocator which we drop — running its cleanup while the device
            // is still alive.
            if let Some(alloc_arc) = self.allocator.take() {
                match std::sync::Arc::try_unwrap(alloc_arc) {
                    Ok(mutex) => drop(mutex.into_inner().expect("allocator lock poisoned")),
                    Err(arc) => {
                        // #665 / LIFE-L1 — the strong-count clones live
                        // inside `GpuBuffer` / `Texture` / `StagingPool`
                        // fields that haven't naturally dropped yet.
                        // Pre-fix the code logged a warning, hit
                        // `debug_assert!(false, …)` (silent in release
                        // builds), and FELL THROUGH to
                        // `device.destroy_device` below. The natural-
                        // Drop pass that runs once this method returns
                        // would then release those Arc clones; when the
                        // last one drops, the inner `Allocator` runs
                        // its destructor, which calls `vkFreeMemory`
                        // on whatever sub-allocations are still tracked
                        // — against a destroyed `VkDevice`. Driver-
                        // level use-after-free.
                        //
                        // Safer in release: leak the device + surface +
                        // instance + debug messenger handles entirely.
                        // The natural-Drop pass below now happens with
                        // a still-valid device, the late `vkFreeMemory`
                        // calls succeed against alive memory, and the
                        // OS reaps the leaked Vulkan handles at process
                        // exit. Debug builds still hit the assertion
                        // so the leak source is investigatable in CI.
                        log::error!(
                            "GPU allocator has {} outstanding references — \
                             leaking allocator + device + surface + instance to avoid \
                             use-after-free on driver-side `vkFreeMemory` of late \
                             natural-Drop allocations. Process must terminate to reclaim.",
                            std::sync::Arc::strong_count(&arc),
                        );
                        debug_assert!(false, "GPU allocator leaked: outstanding Arc references");
                        return;
                    }
                }
            }
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((ref utils, messenger)) = self.debug_messenger {
                utils.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
        log::info!("Vulkan context destroyed cleanly");
    }
}

// Helper functions are in helpers.rs — use helpers:: prefix.
use helpers::{
    allocate_command_buffers, create_command_pool, create_depth_resources,
    create_main_framebuffers, create_render_pass, create_transfer_pool, destroy_depth_resources,
    destroy_main_framebuffers, destroy_render_pass_pipelines, find_depth_format,
    load_or_create_pipeline_cache, save_pipeline_cache,
};

#[cfg(test)]
mod draw_command_tests {
    use super::super::material::{hash_gpu_material_fields, MaterialTable};
    use super::*;

    /// Build a fully-populated `DrawCommand` with distinct, non-default
    /// values for every material-relevant field. Used by the lockstep
    /// contract test below — distinct values per field guarantee that
    /// any drift between `material_hash` and `to_gpu_material` shows up
    /// (a missing field on either walk would produce a hash mismatch).
    fn fully_populated_draw_command() -> DrawCommand {
        DrawCommand {
            // Per-DRAW state (NOT material-relevant; whatever values).
            mesh_handle: 7,
            texture_handle: 0xCAFE_F00D,
            model_matrix: [0.0; 16],
            alpha_blend: true,
            src_blend: 6,
            dst_blend: 7,
            two_sided: false,
            is_decal: false,
            render_layer: byroredux_core::ecs::components::RenderLayer::Architecture,
            bone_offset: 0,
            // Material-relevant fields — every one distinct.
            normal_map_index: 11,
            dark_map_index: 12,
            glow_map_index: 13,
            detail_map_index: 14,
            gloss_map_index: 15,
            parallax_map_index: 16,
            parallax_height_scale: 0.07,
            parallax_max_passes: 8.0,
            env_map_index: 17,
            env_mask_index: 18,
            alpha_threshold: 0.42,
            alpha_test_func: 4,
            roughness: 0.31,
            metalness: 0.79,
            emissive_mult: 1.5,
            emissive_color: [0.11, 0.22, 0.33],
            specular_strength: 0.91,
            specular_color: [0.44, 0.55, 0.66],
            diffuse_color: [0.71, 0.72, 0.73],
            ambient_color: [0.81, 0.82, 0.83],
            vertex_offset: 0,
            index_offset: 0,
            vertex_count: 0,
            sort_depth: 0,
            in_tlas: false,
            in_raster: true,
            avg_albedo: [0.0; 3],
            material_kind: 5,
            z_test: true,
            z_write: true,
            z_function: 3,
            terrain_tile_index: None,
            entity_id: 99,
            uv_offset: [0.125, 0.250],
            uv_scale: [1.5, 2.5],
            material_alpha: 0.875,
            skin_tint_rgba: [0.91, 0.92, 0.93, 0.94],
            hair_tint_rgb: [0.61, 0.62, 0.63],
            multi_layer_envmap_strength: 0.37,
            eye_left_center: [1.1, 1.2, 1.3],
            eye_cubemap_scale: 0.55,
            eye_right_center: [2.1, 2.2, 2.3],
            multi_layer_inner_thickness: 0.018,
            multi_layer_refraction_scale: 0.022,
            multi_layer_inner_scale: [3.5, 4.5],
            sparkle_rgba: [0.81, 0.82, 0.83, 0.84],
            effect_falloff: [0.10, 0.20, 0.30, 0.40, 0.50],
            material_id: 0,
            vertex_color_emissive: true,
            // Fully-populated scaffold — set every bit so any future
            // `material_hash` walk that forgets one fails the lockstep
            // contract test (`material_hash_matches_gpu_material_field_hash`).
            effect_shader_flags: crate::vulkan::material::material_flag::EFFECT_SOFT
                | crate::vulkan::material::material_flag::EFFECT_PALETTE_COLOR
                | crate::vulkan::material::material_flag::EFFECT_PALETTE_ALPHA
                | crate::vulkan::material::material_flag::EFFECT_LIT,
        }
    }

    /// Lockstep contract for #781 / PERF-N4. `DrawCommand::material_hash`
    /// MUST produce the same u64 as `hash_gpu_material_fields(&cmd
    /// .to_gpu_material())` for any DrawCommand. A drift between the
    /// two field walks (e.g. adding a field to `to_gpu_material` but
    /// forgetting it in `material_hash`) breaks dedup correctness:
    /// distinct DrawCommands that build the same GpuMaterial would hash
    /// differently and never collapse. Pin the invariant on a fully-
    /// populated DrawCommand so every live field contributes.
    #[test]
    fn material_hash_matches_gpu_material_field_hash() {
        let cmd = fully_populated_draw_command();
        let h_cmd = cmd.material_hash();
        let h_mat = hash_gpu_material_fields(&cmd.to_gpu_material());
        assert_eq!(
            h_cmd, h_mat,
            "DrawCommand::material_hash drifted from hash_gpu_material_fields \
             (cmd hash {:#018x}, gpu_material hash {:#018x}). One walk has a \
             field the other doesn't — update both in lockstep.",
            h_cmd, h_mat,
        );
    }

    /// Two DrawCommands with identical material fields must dedup to
    /// the same id through the `intern_by_hash` path, even when their
    /// per-DRAW state (mesh_handle, model_matrix, sort_depth) differs.
    /// That's the whole point of the table.
    #[test]
    fn intern_by_hash_dedups_identical_materials() {
        let mut table = MaterialTable::new();
        let mut a = fully_populated_draw_command();
        a.mesh_handle = 1;
        a.entity_id = 100;
        let mut b = fully_populated_draw_command();
        b.mesh_handle = 999;
        b.entity_id = 200;
        // Same material fields → same hash → same id.
        let id_a = table.intern_by_hash(a.material_hash(), || a.to_gpu_material());
        let id_b = table.intern_by_hash(b.material_hash(), || b.to_gpu_material());
        assert_eq!(id_a, id_b, "identical materials must collapse to one id");
        // Slot 0 is the seeded neutral default; user's material is fresh.
        assert_ne!(id_a, 0, "user material distinct from neutral default");
        // Hit + miss = 2 user interns; len = 2 (neutral + user).
        assert_eq!(table.interned_count(), 2);
        assert_eq!(table.len(), 2);
    }

    /// Two DrawCommands with different material fields must NOT dedup.
    /// Verified by tweaking a single field on one command and asserting
    /// the resulting ids differ.
    #[test]
    fn intern_by_hash_distinguishes_distinct_materials() {
        let mut table = MaterialTable::new();
        let a = fully_populated_draw_command();
        let mut b = fully_populated_draw_command();
        b.roughness = 0.99; // single-field difference
        let id_a = table.intern_by_hash(a.material_hash(), || a.to_gpu_material());
        let id_b = table.intern_by_hash(b.material_hash(), || b.to_gpu_material());
        assert_ne!(id_a, id_b, "distinct materials must get distinct ids");
        assert_eq!(table.len(), 3); // neutral + a + b
    }

    /// On a hit, `intern_by_hash` MUST NOT invoke the factory closure
    /// in release builds — that's the whole perf win. Use a `Cell`
    /// counter to verify. (In debug builds the closure DOES run for
    /// the byte-equality assert, which is fine — we exercise debug
    /// behaviour separately via the contract test.)
    #[cfg(not(debug_assertions))]
    #[test]
    fn intern_by_hash_skips_factory_on_hit_in_release() {
        use std::cell::Cell;
        let mut table = MaterialTable::new();
        let cmd = fully_populated_draw_command();
        let h = cmd.material_hash();
        // First insert (miss) — factory runs.
        let calls = Cell::new(0);
        table.intern_by_hash(h, || {
            calls.set(calls.get() + 1);
            cmd.to_gpu_material()
        });
        assert_eq!(calls.get(), 1, "miss path must invoke factory once");
        // Second insert with the same hash (hit) — factory must NOT run.
        table.intern_by_hash(h, || {
            calls.set(calls.get() + 1);
            cmd.to_gpu_material()
        });
        assert_eq!(
            calls.get(),
            1,
            "hit path must skip factory in release; calls jumped to {}",
            calls.get(),
        );
    }
}
