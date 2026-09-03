//! Top-level Vulkan context that owns the entire graphics state.

use super::acceleration::AccelerationManager;
use super::allocator::{self, SharedAllocator};
use super::bloom::BloomPipeline;
use super::caustic::CausticPipeline;
use super::composite::{CompositePipeline, HDR_FORMAT};
use super::compute::ClusterCullPipeline;
use super::debug;
use super::device::{self, QueueFamilyIndices};
use super::exposure::ExposureResource;
use super::frame_upscaler::FrameUpscaler;
use super::gbuffer::{
    GBuffer, ALBEDO_FORMAT, FSR_MASK_FORMAT, MESH_ID_FORMAT, MOTION_FORMAT, NORMAL_FORMAT,
    RAW_INDIRECT_FORMAT,
};
use super::instance;
use super::material::GpuMaterial;
use super::pipeline;
use super::presentation::PresentationPipeline;
use super::scene_buffer;
use super::ssao::SsaoPipeline;
use super::surface;
use super::svgf::SvgfPipeline;
use super::swapchain::{self, SwapchainState};
use super::sync::{self, FrameSync, MAX_FRAMES_IN_FLIGHT};
use super::taa::TaaPipeline;
use super::texture::Texture;
use super::upscaling::{FrameExtentSet, FsrTemporalState, RendererConfig, UpscalerMode};
use super::volumetrics::VolumetricsPipeline;
use super::water::WaterPipeline;
use crate::mesh::MeshRegistry;
use crate::texture_registry::TextureRegistry;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once, Weak};

/// Maximum number of skinned-mesh `SkinSlot`s the per-skinned-entity
/// pre-skin + BLAS refit pool can hold simultaneously. Each slot costs
/// `3 × MAX_FRAMES_IN_FLIGHT = 6` storage-buffer descriptors. Pinned
/// to the bone-palette pool's architectural ceiling
/// (`(MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1 = (196608 / 144) - 1 = 1364`
/// after #1284 step-2) so the descriptor pool never becomes the
/// dominant bottleneck — if you only bump one, the other becomes the
/// invisible cap and a fraction of skinned NPCs silently lose RT
/// shadows. Variable-stride packing (M29.5) is the proper structural
/// fix.
///
/// History:
/// - Pre-#900: 32. Prospector overflowed by 2 entities every frame.
/// - #900: bumped to 64 for M41-EQUIP Prospector (~30 skinned entities,
///   ~2× headroom).
/// - 2026-05-26: bumped to 192 after the saloon scene was observed
///   running with 64 allocated + 51 in `failed_skin_slots`
///   (`TLAS: ... 51 lack BLAS — skinned=51`). LRU eviction only fires
///   for idle slots, and every slot is in-use every frame in a populated
///   interior, so `failed_skin_slots` never clears and 51 NPC sub-meshes
///   permanently lose RT shadows.
/// - #1284 step-1: pinned to the bone-palette architectural ceiling
///   (then 340). Atomic Wrangler casino (FNV `FreesideAtomicWrangler`)
///   was the first cell observed where the bone-palette bump exposed
///   the descriptor pool as the new dominant cap: SkinSlotPool grew
///   226 → 340 but `SKIN_MAX_SLOTS` stayed at 192, so 148 NPCs failed
///   `create_slot` (descriptor sets) and lost RT shadows. Pinning the
///   two caps together avoids re-firing this exact bug on the next
///   bump.
/// - #1284 step-2: tracks the bone-palette bump to 1364 once
///   instrumented telemetry surfaced ~1040 distinct `SkinnedMesh`
///   allocation attempts per frame at Atomic Wrangler peak —
///   3× higher than the static NPC × sub-mesh estimate suggested.
///
/// Output-buffer memory is lazily allocated per `create_slot`, so
/// unused headroom is free; only the descriptor pool sizing
/// (1364 × 2 × 3 = 8184 storage-buffer descs) is paid up-front, and
/// that's still well below typical Vulkan limits (~1 M storage-buffer
/// descs per pool on modern desktop drivers).
pub const SKIN_MAX_SLOTS: u32 = ((crate::vulkan::scene_buffer::MAX_TOTAL_BONES
    / byroredux_core::ecs::components::MAX_BONES_PER_MESH)
    - 1) as u32;

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
    pub supplemental_texture_indices: [u32; super::material::supplemental_texture_slot::COUNT],
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
        use super::material::supplemental_texture_slot as slot;
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
        let mut h = rustc_hash::FxHasher::default();
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
                                                       // greyscale LUT bindless handle (#890 Stage 2c)
        h.write_u32(self.greyscale_lut_index);
        // #1147 Phase 2b — BGSM v>=8 translucency suite. Must mirror
        // the `to_gpu_material` field order so the hash stays
        // byte-equal-safe (#781 contract; pinned by
        // `material_hash_matches_gpu_material_field_hash`).
        h.write_u32(self.translucency_subsurface_color[0].to_bits());
        h.write_u32(self.translucency_subsurface_color[1].to_bits());
        h.write_u32(self.translucency_subsurface_color[2].to_bits());
        h.write_u32(self.translucency_transmissive_scale.to_bits());
        h.write_u32(self.translucency_turbulence.to_bits());
        // #1248 — per-material refractive index (offset 280). Trailing
        // write mirrors `hash_gpu_material_fields` so the contract pinned
        // by `material_hash_matches_gpu_material_field_hash` holds.
        h.write_u32(self.ior.to_bits());
        // #1249 — Disney diffuse lobe (offsets 284-292). Same lockstep
        // requirement as ior above.
        h.write_u32(self.subsurface.to_bits());
        h.write_u32(self.sheen.to_bits());
        h.write_u32(self.sheen_tint.to_bits());
        // #1250 — anisotropic GGX strength (offset 296). Same lockstep.
        h.write_u32(self.anisotropic.to_bits());
        for texture_index in &self.supplemental_texture_indices[..12] {
            h.write_u32(*texture_index);
        }
        // #2221 — animated shader color/float (offsets 348-360). Same
        // lockstep requirement as every field above.
        h.write_u32(self.shader_color[0].to_bits());
        h.write_u32(self.shader_color[1].to_bits());
        h.write_u32(self.shader_color[2].to_bits());
        h.write_u32(self.shader_float.to_bits());
        h.write_u32(self.glass_fresnel_color[0].to_bits());
        h.write_u32(self.glass_fresnel_color[1].to_bits());
        h.write_u32(self.glass_fresnel_color[2].to_bits());
        h.write_u32(self.glass_refraction_scale.to_bits());
        h.write_u32(self.glass_blur_scale.to_bits());
        h.write_u32(self.glass_blur_scale_factor.to_bits());
        h.write_u32(
            self.supplemental_texture_indices
                [super::material::supplemental_texture_slot::GLASS_ROUGHNESS_SCRATCH],
        );
        h.write_u32(
            self.supplemental_texture_indices
                [super::material::supplemental_texture_slot::GLASS_DIRT_OVERLAY],
        );
        h.write_u32(self.lighting_effect_1.to_bits());
        h.write_u32(self.lighting_effect_2.to_bits());
        h.write_u32(self.subsurface_rolloff.to_bits());
        h.write_u32(self.rimlight_power.to_bits());
        h.write_u32(self.backlight_power.to_bits());
        h.write_u32(self.fresnel_power.to_bits());
        h.write_u32(self.grayscale_to_palette_scale.to_bits());
        h.write_u32(
            self.supplemental_texture_indices
                [super::material::supplemental_texture_slot::LIGHTING_MASK],
        );
        h.write_u32(
            self.supplemental_texture_indices
                [super::material::supplemental_texture_slot::BACK_LIGHTING],
        );
        h.finish()
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
    /// interior lighting is a real bug a sealed roof only hides. That is
    /// correct for every consumer except one: `triangle.frag`'s
    /// window-portal escape, where a ray that clears the cell genuinely
    /// sees the outdoor sky, so the interior default transmitted
    /// clear-noon blue at every hour.
    ///
    /// Populated from the surviving `SkyParamsRes` (World-lifetime, not
    /// cell-lifetime — see #1199) and falling back to
    /// `Self::default().zenith_color` when no exterior has loaded this
    /// session. Uploaded as `GpuCamera::exterior_sky_tint` and read by
    /// exactly that one branch, so the interior bypass stays closed.
    pub exterior_zenith_color: [f32; 3],
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
            horizon_color: [0.5, 0.5, 0.45],
            // Pre-#541 fake `horizon * 0.3` baseline preserved as the
            // default; real WTHR-driven exterior cells overwrite from
            // their authored `SKY_LOWER` slot.
            lower_color: [0.15, 0.15, 0.135],
            sun_direction: [-0.4, 0.8, -0.45],
            sun_color: [1.0, 0.95, 0.8],
            sun_size: 0.9994, // cos(~2°) — visible disc, larger than real sun
            sun_intensity: 5.0,
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
            log::info!("BYROREDUX_RENDER_DEBUG = 0x{:x} (POM bypass={}, detail bypass={}, normals viz={}, tangent viz={}, normal-map bypass={}, normal-map force-on={}, render-layer viz={}, glass-passthru viz={}, specular-AA disable={}, half-Lambert-fill disable={}, vertex-color bypass={})",
                v, v & 1 != 0, v & 2 != 0, v & 4 != 0, v & 8 != 0, v & 0x10 != 0, v & 0x20 != 0, v & 0x40 != 0, v & 0x80 != 0, v & 0x100 != 0, v & 0x200 != 0, v & 0x400 != 0);
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

/// Parse the mutually-exclusive renderer diagnostic selected at launch.
///
/// Named modes are intentionally separate from `BYROREDUX_RENDER_DEBUG`'s
/// legacy bitmask: a full-frame correctness view is a canonical renderer
/// concern, not a game- or scene-specific bootstrap condition. Absent or
/// invalid input preserves the legacy selector so existing flag workflows
/// remain unchanged.
fn parse_render_debug_mode_env() -> super::render_debug::RenderDebugMode {
    use super::render_debug::RenderDebugMode;

    let Ok(value) = std::env::var("BYROREDUX_RENDER_DEBUG_MODE") else {
        return RenderDebugMode::default();
    };
    match value.parse::<RenderDebugMode>() {
        Ok(mode) => {
            log::info!("BYROREDUX_RENDER_DEBUG_MODE = {mode}");
            mode
        }
        Err(error) => {
            log::warn!("{error}; preserving the legacy render-debug selector");
            RenderDebugMode::default()
        }
    }
}

/// #1783 / CONC-D2-01 — decide whether `skin_compute` should be forced
/// off given whether `skin_palette` initialised successfully.
///
/// `skin_palette` is the SOLE producer of the bone-palette SSBO
/// (`create_device_local_uninit` — never zero-filled); `skin_compute`'s
/// `skin_vertices.comp` dispatch and its consumers (`record_skinned_
/// blas_refit`'s refit chain, the raster inline-skinning read) all READ
/// that buffer. If `skin_palette` failed to initialise while
/// `skin_compute` succeeded (a partial init failure — mid-init OOM or
/// pipeline-cache corruption), every `skin_compute.is_some()` consumer
/// gate would run against a buffer nothing ever wrote. Forcing
/// `skin_compute` to `None` here makes every existing
/// `skin_compute.is_some()` check an implicit `skin_palette.is_some()`
/// check too, without touching each consumer site individually.
///
/// Pure and generic over the pipeline type so the coupling logic is
/// unit-testable without a live Vulkan device.
fn couple_skin_compute_to_palette<T>(skin_compute: Option<T>, skin_palette_ok: bool) -> Option<T> {
    if skin_palette_ok {
        skin_compute
    } else {
        None
    }
}

/// A water pipeline may only remain enabled when set 2 binding 0 can name a
/// live storage image. Either the real accumulator or the pre-created sink is
/// sufficient; losing both disables water instead of leaving an unwritten
/// descriptor that `water.frag` atomically writes.
fn couple_water_to_caustic_sink<T>(
    water: Option<T>,
    accumulator_ok: bool,
    placeholder_ok: bool,
) -> Option<T> {
    if accumulator_ok || placeholder_ok {
        water
    } else {
        None
    }
}

/// Byte size of one per-frame image-health counter buffer (EX-05 / #2736):
/// two `u32` atomics — non-finite RGB pixels and non-finite alpha pixels.
const IMAGE_HEALTH_BUFFER_BYTES: vk::DeviceSize = 8;

pub struct VulkanContext {
    // Ordered for drop safety — later fields are destroyed first.
    pub current_frame: usize,
    /// Immutable runtime renderer selection parsed by the application.
    pub renderer_config: RendererConfig,
    /// Central scene-render and presentation extents for this swapchain.
    pub frame_extents: FrameExtentSet,
    /// Monotonic frame counter for temporal effects (jitter seed,
    /// accumulation). Wraps at `u32::MAX` (~2.3 years at 60 FPS). When
    /// uploaded to `GpuCamera.position[3]` in `draw_frame` the value
    /// is masked to the bottom 24 bits before the `u32 → f32` cast so
    /// f32 mantissa precision (±1 above 2^24) doesn't freeze the RT
    /// noise patterns mid-session; see `draw.rs` upload site and
    /// #1161 / REN-D9-NEW-08 for the boundary analysis. TAA and
    /// SVGF read the raw u32 directly (no precision issue on the
    /// Rust side).
    pub frame_counter: u32,
    /// Exact publication state of `GpuCamera.flags[0]` for the last frame
    /// recorded by `draw_frame` (including first-slot post-build patching).
    rt_flag_last_frame: bool,
    /// The last TLAS build returned success and exposed a live handle.
    tlas_build_succeeded_last_frame: bool,
    /// Wall-clock-like animation time for stateless volumetric domain warp.
    /// Advanced only after a successful queue submit so failed/aborted frames
    /// cannot move density without producing the corresponding V-buffer.
    pub volumetric_time_seconds: f32,
    /// FSR-authored jitter sequence and one-frame reset state. Present only
    /// for the FSR path; TAA retains its independent legacy sequence.
    pub fsr_temporal: Option<FsrTemporalState>,
    /// Debug-only fragment-shader bypass flags piped through
    /// `GpuCamera.jitter[2]`. Read once from `BYROREDUX_RENDER_DEBUG`
    /// at construction; stays put for the process lifetime. Bits:
    ///   `0x1` — bypass parallax-occlusion (`sampleUV = baseUV`)
    ///   `0x2` — bypass detail-map modulation
    ///   `0x4` — output world-space normal (gbuffer + outColor) and exit
    ///   `0x8` — visualize per-fragment tangent presence (green/red)
    ///   `0x10` — bypass normal-map perturbation (geometric N only)
    ///   `0x20` — reserved no-op (#1035 / R16-01). Pre-#786 this
    ///            opted IN to normal-map perturbation while it was
    ///            off by default; since #786 closed the default
    ///            flipped back to on, so this bit is harmless and
    ///            kept only so legacy diagnostic scripts that set
    ///            it don't suddenly start tripping a different
    ///            behaviour. See `triangle.frag::DBG_RESERVED_20`.
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
    ///   `0x200` — reserved (the retired interior-only directional-fill
    ///            diagnostic; direct lighting now shares one BRDF/shadow
    ///            path across interior and exterior scenes).
    /// Env values are parsed as `0xN` hex or plain decimal; absent /
    /// invalid → 0 (all paths active, zero overhead). For ad-hoc
    /// bisection of texture / lighting artifacts. See engineering
    /// notes around the Dragonsreach "ghost carving" diagnosis.
    pub render_debug_flags: u32,
    /// Mutually-exclusive named correctness view. `LegacyFlags` preserves
    /// launch-time categorical selectors until `render.debug` chooses a mode.
    pub render_debug_mode: super::render_debug::RenderDebugMode,
    pub(super) pending_selected_ray_probe: Option<super::render_debug::SelectedRayProbeRequest>,
    pub(super) selected_ray_probe_result: Option<super::render_debug::SelectedRayProbeResult>,
    pub(super) next_selected_ray_probe_generation: u32,
    /// REND-#1451 — live-tunable point/spot attenuation knee fraction,
    /// uploaded into `GpuCamera.dof_params.z`. `knee = kneeFrac × cull
    /// radius` is the authored radius where the physical near-zone
    /// falloff sits (`pointSpotAtten` in triangle.frag). Default `0.5`
    /// (authored radius at half the cull radius — the
    /// `LIGHT_RANGE_EXTENSION = 2.0` geometry); lower ⇒ dimmer at the
    /// authored radius. Settable live via the `light.atten knee <f>`
    /// console command (routed through the `LightTuning` resource) so
    /// the controlled bench can sweep it with no rebuild.
    pub light_atten_knee: f32,
    /// REND-#1451 — when true, OR `DBG_LEGACY_LIGHT_ATTEN` into the
    /// per-frame debug bitmask so the shader reverts to the pre-fix
    /// window-only attenuation (75% at the authored radius). Lets the
    /// new vs legacy model be A/B'd live. Settable via `light.atten
    /// legacy on|off`; `BYROREDUX_RENDER_DEBUG=0x1000` does the same at
    /// launch.
    pub light_atten_legacy: bool,
    /// Previous frame's view-projection matrix (column-major [f32; 16]).
    /// Used to compute screen-space motion vectors in the vertex shader.
    /// On the very first frame, equals the current frame's viewProj (no motion).
    /// Camera-RELATIVE to `prev_render_origin` (#markarth-precision) — the
    /// upload site right-multiplies by `translation(O₂ − O₁)` so the matrix
    /// consumes current-origin-rebased positions (#1489 / REN2-04).
    pub prev_view_proj: [f32; 16],
    /// Absolute position paired with `prev_view_proj`, used to distinguish
    /// ordinary camera motion from a teleport/cut that must flush history.
    pub prev_camera_position: [f32; 3],
    /// The render origin `prev_view_proj` was built against (last frame's
    /// 4096-grid snap). Tracked so the uploaded previous-frame matrix can be
    /// origin-corrected on grid-crossing frames instead of producing one
    /// frame of full-screen garbage motion vectors (#1489 / REN2-04).
    pub prev_render_origin: [f32; 3],
    /// Previous frame's world-space camera forward vector (unit length).
    /// Paired with `prev_camera_position` as the second `camera_cut` signal
    /// (#2159): a real teleport/cut reorients the camera drastically in one
    /// frame, unlike ordinary translation or mouselook. Replaces a raw
    /// full-matrix `view_proj` delta, which misfired on both ordinary
    /// locomotion speeds and every render-origin grid crossing.
    pub prev_cam_forward: [f32; 3],
    // ── Per-frame scratch cluster ───────────────────────────────────────
    // The four `*_scratch` Vecs below (plus `terrain_tile_scratch` further
    // down in the struct definition) all follow the same amortization
    // pattern (#243 / #496): cleared + reserved at the top of
    // `draw_frame` and rebuilt in place from the live scene, so capacity
    // stays around across frames instead of heap-allocating fresh every
    // 16 ms. Drop runs once at shutdown — no explicit teardown required;
    // the Vec destructors release the backing allocations alongside the
    // rest of the context. Documented as a group per REN-D7-NEW-06 so
    // adding a new scratch Vec to the cluster is an obvious "matches
    // pattern" review.
    /// Per-frame scratch buffer for the GPU instance SSBO payload. Held on
    /// the context so that capacity amortizes across frames instead of
    /// heap-allocating fresh each `draw_frame`. Cleared + reserved at the
    /// top of draw_frame. See issue #243.
    gpu_instances_scratch: Vec<scene_buffer::GpuInstance>,
    /// Canonical scene lights plus delayed transported-combustion lights for
    /// the current frame. The input slice belongs to the application, so the
    /// renderer needs one reusable merge buffer before cluster upload.
    frame_lights_scratch: Vec<scene_buffer::GpuLight>,
    /// Previous rigid transforms keyed by stable draw/entity id. Updated only
    /// after queue submission succeeds, matching temporal GPU history.
    ///
    /// #2174 / D2-03 — `FxHashMap`, not the std default. Both maps are probed
    /// twice per rigid draw per frame (~29K probes on MedTek), which is the
    /// exact shape the renderer already standardizes `rustc_hash` on
    /// (`material.rs`'s material-dedup index, the skin-slot map). SipHash-1-3
    /// buys collision resistance nobody needs for a `u32` draw id we generate
    /// ourselves. Allocation behaviour is unrelated and already correct: the
    /// maps are `mem::take`n, cleared, and swapped in `draw.rs`, so no
    /// per-frame heap churn — hashing was the only remaining cost.
    previous_rigid_models: FxHashMap<u32, [f32; 16]>,
    /// Previous frame's caustic scene key — the light rig plus every
    /// caustic-source instance's placement, folded to a `u64` (#2468 /
    /// REN-D14-2026-08-07-01). The caustic accumulator's parked-camera
    /// EMA holds up to ~200 frames of history with no per-pixel
    /// invalidation of its own, so a change here (or a moved rigid
    /// instance, or a dirty skinned pose) has to reset it: otherwise a
    /// player standing still while a torch-carrying NPC walks past keeps
    /// a ~3 s caustic ghost of the old pool.
    prev_caustic_scene_key: u64,
    /// Current-frame map reused while assembling the next submitted history.
    current_rigid_models_scratch: FxHashMap<u32, [f32; 16]>,
    /// Previous transforms realigned to this frame's sorted instance indices.
    previous_models_scratch: Vec<scene_buffer::GpuPreviousModel>,
    /// Per-frame scratch buffer for draw batch metadata. Same lifecycle
    /// as `gpu_instances_scratch`. See issue #243.
    batches_scratch: Vec<draw::DrawBatch>,
    /// Per-frame scratch buffer for indirect draw commands. Replaces the
    /// per-frame `Vec::collect()` allocation that was untracked by the
    /// scratch-buffer pattern.
    indirect_draws_scratch: Vec<ash::vk::DrawIndexedIndirectCommand>,
    /// Set each frame right after `upload_indirect_draws` (`true` on
    /// success or a hash-matched skip, `false` on upload failure).
    /// `record_geometry_pass` ANDs this into `use_indirect` so a failed
    /// upload forces the direct-draw fallback for the whole frame instead
    /// of issuing `cmd_draw_indexed_indirect` over a buffer that still
    /// holds a stale or (on a slot's first use) uninitialized layout —
    /// GPU page-fault / TDR class, not a misrender. #2504 /
    /// D12-2026-08-07-02.
    indirect_upload_ok: bool,
    /// Per-frame scratch for the skin-compute dispatch walker
    /// (#1133 / PERF-D7-NEW-01). Pre-fix the skinned hot path
    /// allocated 3 fresh containers per frame; on Prospector that's
    /// ~9 reallocs × 34 NPCs × 60 fps ≈ 18 K reallocs/s. Same
    /// `mem::take` → `clear()` → `mem::replace` pattern as the
    /// instance / batch / indirect scratches above.
    ///
    /// #3045 / REN-D9-01 — `FxHashSet`, not the std default. #2923 converted
    /// the rest of the skinning path and stopped one field short of this one:
    /// `insert` runs once per skinned draw command per frame, which is the
    /// per-frame per-entity keyspace SipHash-1-3 is the wrong default for.
    /// Pinned by `pose_dirty_crosses_the_crate_boundary_without_siphash`.
    skin_dispatch_seen_scratch: FxHashSet<byroredux_core::ecs::storage::EntityId>,
    /// Sibling of `skin_dispatch_seen_scratch` — entity → SkinPushConstants
    /// + buffer handles for the per-frame compute dispatch.
    skin_dispatches_scratch: Vec<(
        byroredux_core::ecs::storage::EntityId,
        super::skin_compute::SkinPushConstants,
        vk::Buffer,
        u32,
        u32,
    )>,
    /// Sibling of `skin_dispatches_scratch` — first-sight BLAS BUILD
    /// queue for entities that don't yet have a SkinSlot or skinned
    /// BLAS. Drained by the batched on-cmd builder each frame.
    skin_first_sight_builds_scratch: Vec<(
        byroredux_core::ecs::storage::EntityId,
        vk::Buffer,
        u32,
        vk::Buffer,
        u32,
    )>,
    /// Sibling of `skin_first_sight_builds_scratch` — entities whose
    /// skinned BLAS was just BUILT (not refit) on `cmd` this frame
    /// (D6-05 / #1812). The refit loop right below the build batch
    /// skips these entirely: a full UPDATE against the identical
    /// vertex data the BUILD consumed moments earlier in the same
    /// command buffer is pure wasted work, not a correctness
    /// requirement — `accel`'s BLAS entry is already complete after
    /// the BUILD.
    ///
    /// #3045 SIBLING — `FxHashSet` for the same reason as
    /// `skin_dispatch_seen_scratch` above: the refit loop probes it once per
    /// skinned entity per frame. Same cluster, same keyspace, same miss.
    skin_built_this_frame_scratch: FxHashSet<byroredux_core::ecs::storage::EntityId>,

    // ── Screenshot capture ──────────────────────────────────────────
    screenshot_requested: Arc<AtomicBool>,
    screenshot_result: Arc<Mutex<Option<Vec<u8>>>>,
    /// Monotonic capture generation, shared with `ScreenshotBridge` (#1603).
    /// Captured into `screenshot_pending_readback` at record time; the
    /// readback only publishes its PNG when this still matches, so a
    /// capture cancelled mid-flight is not served to a later claimant.
    screenshot_generation: Arc<AtomicU64>,
    /// Staging buffer for screenshot readback (allocated on first capture).
    screenshot_staging: Option<(vk::Buffer, vk_alloc::Allocation, vk::DeviceSize)>,
    /// Extent + capture generation recorded at copy time; `Some` while the
    /// staging buffer holds data waiting for the fence.  The extent is stored
    /// here (not re-derived from the live swapchain) so a same-frame resize
    /// cannot corrupt the readback dimensions (#1448); the generation gates
    /// publication against an intervening `cancel()` (#1603).
    screenshot_pending_readback: Option<(vk::Extent2D, u64)>,

    // ── Depth capture (#3308) ───────────────────────────────────────
    /// Set by `DepthCaptureBridge::request`; consumed by the next frame's
    /// `depth_capture_record_copy`. Single-consumer, so unlike the
    /// screenshot bridge it carries no owner tag or generation — see
    /// `depth_capture.rs`'s module doc.
    depth_capture_requested: Arc<AtomicBool>,
    depth_capture_result: Arc<Mutex<Option<byroredux_core::ecs::DepthCapture>>>,
    /// Staging buffer for depth readback (allocated on first capture).
    depth_capture_staging: Option<(vk::Buffer, vk_alloc::Allocation, vk::DeviceSize)>,
    /// Render extent recorded at copy time; `Some` while the staging buffer
    /// holds data waiting for the fence. Stored here rather than re-derived
    /// so a same-frame resize cannot decode the new dimensions against the
    /// old copy — the same REG-02 / #1448 invariant the screenshot path has.
    depth_capture_pending_readback: Option<vk::Extent2D>,

    frame_sync: FrameSync,
    /// Per-frame image-health counter buffers (EX-05 / #2736).
    ///
    /// Two `u32`s each — non-finite RGB pixels, non-finite alpha pixels —
    /// written atomically by `presentation.frag` and read back on this slot's
    /// next fence wait, then CPU-zeroed at that same point.
    ///
    /// #2740 (REN-D4-04) — a fence signal only guarantees the *device*-side
    /// access scope has completed; it does NOT by itself make that write
    /// host-visible on a non-coherent memory type (that needs an explicit
    /// `vkInvalidateMappedMemoryRanges`). This buffer's host read/write is
    /// safe with no barrier ONLY because `create_host_visible` allocates
    /// `MemoryLocation::CpuToGpu`, and gpu-allocator 0.27's `CpuToGpu` preset
    /// requires `HOST_COHERENT` (not merely prefers it) — see
    /// `collect_image_health`'s doc comment (`context/resources.rs`) for the
    /// full explanation. That is a property of this specific allocator
    /// version, not a Vulkan-spec guarantee from the fence alone.
    ///
    /// Owned here rather than by `PresentationPipeline` because they must
    /// survive a swapchain recreate, which rebuilds that pipeline wholesale.
    image_health_buffers: Vec<super::buffer::GpuBuffer>,
    /// Last completed frame's counters, `(non_finite_rgb, non_finite_alpha)`.
    image_health_last: (u32, u32),
    /// Running total since process start, so a transient NaN that appears on
    /// one frame is still visible to a gate that samples later.
    image_health_total: (u64, u64),
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
    // Soft-particle depth fade — sampleable copy of last frame's opaque
    // depth, bound to triangle.frag set 1 binding 15. See the creation
    // comment in `new()` and the per-frame copy in `draw.rs`.
    depth_history_image: vk::Image,
    depth_history_view: vk::ImageView,
    depth_history_allocation: Option<vk_alloc::Allocation>,
    depth_history_sampler: vk::Sampler,
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
    /// Per-pass GPU timer (#1194 / PERF-DIM7-INSTR). Brackets the
    /// skin compute dispatch loop, skinned BLAS refit loop, and TAA
    /// compute dispatch with `VkQueryPool` TIMESTAMP queries. `None`
    /// when the driver lacks `timestampComputeAndGraphics` (extremely
    /// rare on desktop GPUs — `VK_KHR_acceleration_structure` mandates
    /// it on any device exposing RT). Read by `fill_skin_coverage_stats`
    /// and surfaced through the `skin.coverage` console + bench
    /// summary so PERF-DIM7-01 / -02 / -03 (#1195 / #1196 / #1197)
    /// can be measured rather than guessed.
    pub gpu_timers: Option<super::gpu_timers::GpuPerFrameTimers>,
    /// M29.5 — GPU bone-palette compute pipeline. Reads the per-frame
    /// `bone_world[]` + `bind_inverses[]` input SSBOs (held on
    /// [`SceneBuffers`]) and writes the existing palette SSBO that
    /// [`skin_compute`] (M29.3, RT) + raster `triangle.vert` consume.
    /// `None` when RT is unsupported — matches [`skin_compute`]'s
    /// gating. With the orphaned legacy `upload_bones` path now
    /// removed (M29.5 cleanup), a `None` here means the palette
    /// buffer is never written, raster reads uninitialised data,
    /// and skinned content renders as garbage. The engine's
    /// VRAM-baseline policy makes RT mandatory, so this is treated
    /// as dead-on-arrival rather than a supported degradation.
    pub skin_palette: Option<super::skin_compute::SkinPaletteComputePipeline>,
    /// Per-skinned-entity SkinSlot — owns the skinned-vertex output
    /// buffer + per-frame descriptor sets. Populated lazily on first
    /// sight in draw_frame; entries are torn down on Drop. M40 cell
    /// streaming will eventually reclaim slots whose entities are
    /// despawned mid-session.
    ///
    /// #3061 — `FxHashMap`. `draw_frame`'s skinned pass probes this once per
    /// skinned draw command per frame; the key is an engine-generated
    /// `EntityId`, never attacker-chosen, so SipHash-1-3 buys nothing here.
    pub skin_slots:
        FxHashMap<byroredux_core::ecs::storage::EntityId, super::skin_compute::SkinSlot>,
    /// #3231 — per-skinned-entity `MorphSlot` (shared delta + private
    /// weight buffer for GPU morph-target blending). Deliberately separate from
    /// `skin_slots` — see `morph_compute`'s module doc for why.
    /// Populated once at spawn time (not lazily like `skin_slots` —
    /// the morph-target delta data only exists in `ImportedMesh` at
    /// that point); entries are torn down on Drop or explicit
    /// cell-unload despawn.
    ///
    /// #3061 SIBLING — `FxHashMap` for the same reason as `skin_slots`:
    /// `draw.rs` probes it per draw command per frame.
    pub morph_slots:
        FxHashMap<byroredux_core::ecs::storage::EntityId, super::morph_compute::MorphSlot>,
    /// Mesh-handle keyed weak references to the immutable morph delta
    /// allocations held by `morph_slots`. Weak entries make the cache a
    /// lookup index, not an additional VRAM owner: after the last entity
    /// slot is evicted, its `Arc<MorphDelta>` destroys the delta buffer and
    /// the dead key is pruned by the eviction pass.
    pub(crate) morph_delta_cache: FxHashMap<u32, Weak<super::morph_compute::MorphDelta>>,
    /// Entities whose `create_slot` call returned `OUT_OF_POOL_MEMORY`
    /// (or otherwise errored) on a prior frame — gate the retry path
    /// in `draw_frame` against this set so a single failure logs one
    /// WARN instead of N (one per frame for the duration of the
    /// bench, observed at 58 WARN / 300 frames pre-fix on Prospector
    /// post-#896 B.2). Cleared whenever any LRU eviction frees a
    /// slot, since capacity opening up means a previously-failing
    /// entity's next attempt could now succeed. A stale entry cannot
    /// poison a re-issued id because `EntityId`s are **never recycled** —
    /// `World::spawn` is monotonic and `despawn` does not reclaim (#372).
    /// `EntityId` is a bare `u32` with no generation field
    /// (`crates/core/src/ecs/storage.rs`); the owning invariant, and the
    /// reason it is deliberately not generational, is documented on
    /// `World::spawn` in `crates/core/src/ecs/world.rs`. Every
    /// entity-keyed renderer cache here (`skin_slots`, `morph_slots`,
    /// `failed_skin_slots`, `failed_skin_blas`, the
    /// `pending_*_unload_victims` lists) rests on that same never-recycled
    /// rule — not on generational tagging, which the ECS does not have.
    /// See #900, #3375.
    ///
    /// Drop contract (REN-D7-NEW-03): purely host-side state, no
    /// Vulkan handles or device memory involved — the HashSet
    /// destructor at context teardown is sufficient, no explicit
    /// clear required. Adding entries that hold device-side
    /// resources here would invalidate this contract.
    /// #3061 — `FxHashSet`; gated once per skinned entity per frame.
    pub failed_skin_slots: FxHashSet<byroredux_core::ecs::storage::EntityId>,
    /// Entities whose first-sight `build_skinned_blas_batched_on_cmd`
    /// returned an error on a prior frame — the BLAS sibling of
    /// [`Self::failed_skin_slots`] (#2802 / REN-D9-2026-08-12-03).
    ///
    /// The slot set gates *slot allocation* only, and a build failure
    /// leaves the slot behind, so `needs_slot` is false on the next
    /// frame and that gate never fires. The entity therefore re-entered
    /// the size query + build every frame and logged **two** WARNs per
    /// frame: the build failure itself, plus the refit that then could
    /// not find the BLAS the build never produced. Both fire precisely
    /// under the VRAM pressure that caused the failure.
    ///
    /// Cleared with `failed_skin_slots` on any LRU eviction — the same
    /// "capacity opened up, retry once" rule, for the same reason (an
    /// AS allocation failure is a memory-pressure signal, and eviction
    /// is the event that changes it).
    ///
    /// Drop contract: same as `failed_skin_slots` — host-side ids only,
    /// no Vulkan handles.
    /// #3061 — `FxHashSet`, as `failed_skin_slots`.
    pub failed_skin_blas: FxHashSet<byroredux_core::ecs::storage::EntityId>,
    /// Cell-unload victims pending skin-slot teardown. Populated by
    /// `unload_cell`, drained by the per-frame eviction pass at the
    /// top of `draw_frame` (after the fence wait that retires any
    /// in-flight command buffer referencing the slot's output buffer).
    ///
    /// **Why a queue and not immediate `destroy_slot`** (#1003):
    /// `skin_pipeline.destroy_slot` is unconditional and synchronous —
    /// caller must guarantee no in-flight command buffer references
    /// the slot's output buffer. The eviction pass already runs
    /// post-fence-wait and is therefore safe. `unload_cell` runs
    /// outside `draw_frame` and pre-fix relied on the eviction pass
    /// catching despawned entities within ~3 frames; cell-unload-
    /// without-render-tick (headless smoke tests, paused world)
    /// silently retained slots indefinitely. Routing through this
    /// queue makes the teardown window deterministic.
    ///
    /// **Drop contract**: tinier than `skin_slots` because each entry
    /// is a `u32`-shaped `EntityId`; the actual slot is held in
    /// `skin_slots` until drain. The eviction pass moves the entry
    /// out of `skin_slots` and into `destroy_slot` in a single step,
    /// so any race between `unload_cell` and `Drop` resolves cleanly
    /// (a victim queued but not drained still has its slot in
    /// `skin_slots`, which `Drop` tears down via the bulk loop at
    /// `mod.rs:1965`).
    pub pending_skin_unload_victims: Vec<byroredux_core::ecs::storage::EntityId>,
    /// #3231 — the `morph_slots` sibling of `pending_skin_unload_victims`.
    /// Same reasoning, same drain site (folded into the same eviction
    /// pass in `skinned_blas_refit.rs` rather than a separate sweep,
    /// since `MorphSlot` lifetime already tracks `SkinSlot`'s 1:1 —
    /// v1 only creates a `MorphSlot` for a mesh that also has skin
    /// data, see `mesh_instance::spawn_mesh_instance`). No `failed_*`
    /// sibling — unlike `SkinSlot`, `MorphSlot` has no lazy first-sight
    /// retry path to gate; a spawn-time create failure just logs and
    /// leaves the entity with no slot for its lifetime.
    pub pending_morph_unload_victims: Vec<byroredux_core::ecs::storage::EntityId>,
    /// Per-frame counters for the skinned-BLAS coverage path, written
    /// by `draw_frame` and copied into the [`byroredux_core::ecs::
    /// SkinCoverageStats`] resource by [`Self::fill_skin_coverage_stats`].
    /// #2112 / D6-01 — reset at the top of `draw_frame`, before the
    /// early-return framebuffers-empty guard (same reasoning as
    /// `skin_dispatch_ran` below), so a bailed frame reads zero instead
    /// of retaining the previous frame's counts.
    pub last_skin_coverage_frame: super::skin_compute::SkinCoverageFrame,
    /// Per-frame draw-call counts written by `draw_frame` and read by
    /// the app's per-frame stats wiring to populate `DebugStats`. Distinct
    /// from the input `DrawCommand` count (which lives on the caller side
    /// as `draw_commands.len()`) — these are the post-batch GPU call
    /// counts that actually drive cost. See #1258 / PERF-D3-NEW-03 for
    /// the misdiagnosis history; pre-fix only the pre-batch input count
    /// was surfaced as "Draws" via `DebugStats::draw_call_count`.
    /// Reset at the top of `draw_frame`; populated after the batch
    /// merge + indirect-grouping passes.
    pub last_draw_call_stats: DrawCallStats,
    /// #1796 / D6-02 — set `false` at the top of every `draw_frame` call,
    /// flipped `true` only once `record_skinned_blas_refit` actually runs
    /// (i.e. `draw_frame` got past both early-return guards: the empty-
    /// framebuffers check and `ERROR_OUT_OF_DATE_KHR`). The CPU-side pose
    /// hash commit (`SkinSlotPool::try_mark_pose_dirty`, called from
    /// `build_render_data` *before* `draw_frame`) runs unconditionally,
    /// so an early return leaves the dirty-gate baseline advanced past a
    /// dispatch that never happened. The caller checks this flag after
    /// `draw_frame` returns and calls `SkinSlotPool::
    /// rollback_pending_pose_commits` when it reads `false`, undoing the
    /// premature commit so the next frame's comparison stays honest.
    pub skin_dispatch_ran: bool,
    /// D6-04 / #1811 — consecutive frames where no skinned entity's pose
    /// changed and no `bind_inverses` upload was pending. Reset to `0` on
    /// any dirty frame; once it exceeds `MAX_FRAMES_IN_FLIGHT`, every
    /// per-frame `bone_world` buffer copy has already been refreshed with
    /// today's (unchanged) values at least once, so the bone_world upload,
    /// its device copy, and the `skin_palette.comp` dispatch are all safe
    /// to skip until the next dirty frame. See `draw_frame`'s bone_world
    /// upload section for the read side.
    pub clean_skin_frames: u32,
    pub ssao: Option<SsaoPipeline>,
    /// 1×1 white "AO = 1.0" stand-in for scene binding 7, and a 1×1
    /// storage sink for `WaterPipeline` set 2 (#2141 / #2142).
    ///
    /// Both consumers bind their descriptor unconditionally every frame —
    /// `triangle.frag` samples `aoTexture` with no gate, and the water
    /// draw issues `imageAtomicAdd` against set 2 — so when the optional
    /// pass behind one of them fails to create (init) or re-create
    /// (resize), simply setting the pass to `None` leaves the descriptor
    /// naming a *destroyed* image. Rebinding to these keeps the degraded
    /// state pointing at live memory.
    ///
    /// Created once at init, deliberately not on the failure path itself:
    /// the realistic trigger is VRAM exhaustion, where allocating more is
    /// exactly what won't work. `None` only if that init allocation fails,
    /// in which case the affected bindings keep today's behaviour.
    pub placeholder_ao: Option<super::placeholder::PlaceholderImage>,
    pub placeholder_caustic_sink: Option<super::placeholder::PlaceholderImage>,
    /// FSR/presentation exposure producer — a persistent 1x1 `R32_SFLOAT`
    /// texture holding the single fixed HDR exposure value. It is the source
    /// of truth the FSR dispatch samples and the presentation tonemap reads,
    /// so the two cannot drift into independent constants. `None` if
    /// allocation failed; presentation falls back to
    /// [`super::exposure::DEFAULT_EXPOSURE`].
    pub exposure: Option<ExposureResource>,
    pub composite: Option<CompositePipeline>,
    /// Render-resolution scene → output-resolution HDR reconstruction.
    /// In TAA mode (and if FSR initialization fails), this records the native
    /// Vulkan bridge through the same explicit frame-graph seam.
    pub frame_upscaler: Option<FrameUpscaler>,
    /// Output-resolution exposure/tone-map pass into the acquired swapchain
    /// image. Kept separate from scene composition so FSR sees linear HDR.
    pub presentation: Option<PresentationPipeline>,
    pub gbuffer: Option<GBuffer>,
    pub svgf: Option<SvgfPipeline>,
    /// ReSTIR-DI direct-shadow reservoir buffers (screen-sized, ping-pong
    /// per frame-in-flight). Read/written by `triangle.frag` via scene-set
    /// bindings 16/17 for temporal shadow-sample reuse. See `vulkan::restir`.
    pub reservoir_buffers: super::restir::ReservoirBuffers,
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
    /// Procedural volumetric-lighting pipeline: render-resolution-derived
    /// froxel V-buffer, temporal density history, TLAS/BLAS visibility, and
    /// pre-integrated composite output. `None` only when initialization fails.
    pub volumetrics: Option<VolumetricsPipeline>,
    /// Bloom pyramid pipeline (M58, Tier 8). Reads the scene HDR
    /// after TAA, produces a multi-scale blurred bright-content
    /// texture that composite adds back to `combined` before the
    /// ACES tone-map. `None` when the down/up image-pyramid
    /// allocation fails; engine initialization fails in that case
    /// because composite requires the bloom output view for binding 7
    /// (see construction guard at `VulkanContext::new`). Unlike other
    /// optional pipelines (water, ssao), bloom cannot be soft-skipped.
    pub bloom: Option<BloomPipeline>,
    /// Water surface pipeline — renders `WaterPlane` entities as
    /// transparent draws inside the main render pass (subpass 0)
    /// after all opaque + alpha-blend triangles have submitted.
    /// `None` when pipeline creation fails on initial setup; the
    /// draw site is gated on `Some` so a failure simply skips water
    /// rendering for the rest of the session (same robustness policy
    /// as every other optional pipeline in the renderer).
    pub water: Option<WaterPipeline>,
    /// Per-FIF R32_UINT accumulator for water-side caustic synthesis
    /// (#1255 / Phase C of #1210). Cleared BEFORE the main render
    /// pass each frame so `water.frag`'s `imageAtomicAdd` calls in
    /// the main pass accumulate against zeros; composite samples it
    /// alongside the existing `caustic.causticTex`. `None` when
    /// image creation failed (degrades gracefully — water renders
    /// without caustic contribution, same as pre-#1255 behaviour).
    pub water_caustic_accum: Option<super::water_caustic::WaterCausticAccum>,
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
    /// Same latch for the caustic scatter pass. Unlike SVGF/TAA's
    /// "keeps sampling stale content" degradation, the caustic skip path
    /// (this latch, or a frame with no TLAS yet) explicitly clears its
    /// accumulator slot instead — see [`Self::caustic_cleared_on_skip`]
    /// and #2507. See #479 SIBLING.
    pub caustic_failed: bool,
    /// Per-frame-in-flight latch: `true` once this slot's caustic
    /// accumulator has been cleared for the current skip streak (#2507).
    /// Composite samples `causticTex` with no validity gate, so a skipped
    /// dispatch (TLAS absent, or `caustic_failed`) must clear the slot
    /// rather than leave its last-written, camera-motion-blind contents
    /// to be re-composited every subsequent frame. Reset to `false` the
    /// moment a real dispatch runs (so the *next* skip streak clears
    /// again) and on resize (fresh slot images, stale latch state would
    /// otherwise skip the needed post-resize clear).
    pub caustic_cleared_on_skip: [bool; MAX_FRAMES_IN_FLIGHT],
    /// Per-frame-in-flight latch for the volumetrics gate-off arm (#3685) —
    /// same shape as [`Self::caustic_cleared_on_skip`], reusing the same
    /// pure `skip_clear_decision` state machine. `record_volumetrics_pass`
    /// skips its dispatch whenever `requires_dispatch` is false (no medium,
    /// no fog volumes, no lingering combustion) or the TLAS/cluster/geometry
    /// inputs aren't ready yet; composite reads the integrated froxel
    /// volume unconditionally every frame, so the first skip of a streak
    /// still needs a neutral-frame clear — this latch is what stops every
    /// *subsequent* skip in that streak from re-clearing an already-zero
    /// volume. Reset to `false` the moment a real dispatch runs (or fails —
    /// see the comment at the call site) and on resize.
    pub volumetrics_cleared_on_skip: [bool; MAX_FRAMES_IN_FLIGHT],
    pipeline_cache: vk::PipelineCache,
    /// Opaque pipeline (depth write on, no blend). Two-sided rendering
    /// uses dynamic `cmd_set_cull_mode` per draw, not a separate
    /// pipeline (#930) — pre-#930 there were two pipelines whose only
    /// difference was static cull state, but with `vk::DynamicState::
    /// CULL_MODE` the static value is ignored, so they compiled to
    /// identical machine code.
    pipeline: vk::Pipeline,
    /// Opaque wireframe pipeline (`polygon_mode = LINE`). `None` when
    /// the device lacks `fillModeNonSolid`; the draw-time selector
    /// falls back to `pipeline` (filled) when this is `None`. #869.
    pipeline_wireframe: Option<vk::Pipeline>,
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
    /// Cache key: `(src, dst, wireframe, preserve_opaque_gbuffer)`. The
    /// wireframe boolean was added under #869 — entries are independent
    /// per polygon mode so a blend material with `NiWireframeProperty`
    /// gets its own pipeline. The final axis isolates refractive glass,
    /// whose HDR output blends normally while writes to opaque G-buffer
    /// attachments are masked off. Wireframe is only reachable when
    /// `caps.fill_mode_non_solid_supported` is true; callers must gate.
    ///
    /// #3061 — `FxHashMap`. `record_geometry_pass` does a `contains_key` per
    /// batch per frame; the key is four engine-derived material bits with a
    /// tiny bounded domain, so there is nothing for SipHash's HashDoS
    /// resistance to protect.
    blend_pipeline_cache: FxHashMap<(u8, u8, bool, bool), vk::Pipeline>,
    /// Per-frame scratch — the set of distinct blend cache keys seen in
    /// this frame's batch list. Used by the pre-pop walk
    /// in `draw_frame` to skip the full per-batch `contains_key` sweep
    /// when every seen key is already in `blend_pipeline_cache`. Cleared
    /// at the top of the walk; capacity persists across frames for
    /// amortized churn-free reuse. #1259 / PERF-D3-NEW-04.
    /// #3061 — `FxHashSet`, the per-frame half of `blend_pipeline_cache`.
    blend_seen_scratch: FxHashSet<(u8, u8, bool, bool)>,
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
    /// fresh high-water prefix into the single DEVICE_LOCAL SSBO and clears
    /// the flag. Pre-#497 this was a per-frame-in-flight countdown against a
    /// HOST_VISIBLE double-buffered SSBO; the buffer is static until the next
    /// cell transition so a single DEVICE_LOCAL allocation is the correct
    /// shape.
    terrain_tiles_dirty: bool,
    /// Persistent scratch buffer reused across frames to stage the live
    /// high-water `GpuTerrainTile` prefix before upload. Same amortization
    /// pattern as `gpu_instances_scratch`; fresh `Vec::collect()` every
    /// dirty frame was 32 KB × MAX_FRAMES_IN_FLIGHT of heap churn per cell
    /// transition. See #496 / #3664.
    terrain_tile_scratch: Vec<scene_buffer::GpuTerrainTile>,
    render_pass: vk::RenderPass,
    swapchain_state: SwapchainState,

    pub allocator: Option<SharedAllocator>,
    /// One-shot latch for the allocator's high-memory warning. Sampling is
    /// allowed at renderer and cell boundaries without spamming a sustained
    /// breach once per transition. Scoped to this context so a new device
    /// gets a fresh warning opportunity.
    pub(crate) memory_warning_once: Once,

    /// Debug-UI overlay pass (Phase 4 of the debug-UI plan). `None`
    /// until [`Self::init_egui`] is called — the binary opts in at
    /// boot after the window + allocator are live. Drawn into the
    /// swapchain image immediately after composite + before the
    /// screenshot copy.
    pub egui_pass: Option<super::egui_pass::EguiPass>,
    /// Per-frame egui handoff: `(context, output)` stashed by
    /// [`Self::submit_egui_frame`] right before `draw_frame`,
    /// consumed by `draw_frame` after composite. `None` on frames
    /// where the overlay is hidden — the egui pass simply skips
    /// for the frame.
    pub egui_pending_output: Option<(egui::Context, egui::FullOutput)>,

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

// CoreDevice + VulkanContext::new()'s construction chain moved to
// init.rs (#1749 / TD1-004): build_core_device, SwapchainResources,
// build_swapchain_and_resources, build_pipelines_and_finish, new().

impl VulkanContext {
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
                // SAFETY: the `device_wait_idle` above has settled all in-flight
                // command buffers, so the acceleration structures queued for
                // destruction are no longer referenced by any pending GPU work;
                // `self.device` + `allocator` are live and own them.
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
    pub fn get_or_create_blend_pipeline(
        &mut self,
        src: u8,
        dst: u8,
        wireframe: bool,
        preserve_opaque_gbuffer: bool,
    ) -> Result<vk::Pipeline> {
        // Downgrade wireframe → fill if the device doesn't support
        // `vk::PolygonMode::LINE`. #869 — Oblivion vanilla ships zero
        // wireframe meshes so the fallback is invisible to content.
        let wireframe = wireframe && self.device_caps.fill_mode_non_solid_supported;
        let key = (src, dst, wireframe, preserve_opaque_gbuffer);
        if let Some(&pipe) = self.blend_pipeline_cache.get(&key) {
            return Ok(pipe);
        }
        let pipe = pipeline::create_blend_pipeline(
            pipeline::BlendPipelineCtx {
                device: &self.device,
                render_pass: self.render_pass,
                extent: self.frame_extents.render,
                pipeline_cache: self.pipeline_cache,
                pipeline_layout: self.pipeline_layout,
            },
            src,
            dst,
            wireframe,
            preserve_opaque_gbuffer,
        )?;
        self.blend_pipeline_cache.insert(key, pipe);
        Ok(pipe)
    }

    /// Get a handle for requesting screenshots from outside the render loop.
    pub fn screenshot_handle(&self) -> ScreenshotHandle {
        ScreenshotHandle {
            requested: Arc::clone(&self.screenshot_requested),
            result: Arc::clone(&self.screenshot_result),
            generation: Arc::clone(&self.screenshot_generation),
        }
    }

    /// Get a handle for requesting depth captures from outside the render
    /// loop (#3308). Feeds `byroredux_core::ecs::DepthCaptureBridge`.
    pub fn depth_capture_handle(&self) -> DepthCaptureHandle {
        DepthCaptureHandle {
            requested: Arc::clone(&self.depth_capture_requested),
            result: Arc::clone(&self.depth_capture_result),
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
        if let Some(ref mut fsr) = self.fsr_temporal {
            fsr.signal_reset();
        }
        if let Some(ref mut volumetrics) = self.volumetrics {
            volumetrics.signal_history_reset();
        }
        // The first frame after a discontinuity must not encode object motion
        // against transforms from the retired scene/camera history.
        self.previous_rigid_models.clear();
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
    /// Refresh the world-visible upscaler telemetry. Only rewrites the
    /// string when the described state actually changed (context creation,
    /// resize, preset switch, or a latched dispatch failure), so the steady
    /// state costs one string compare per frame rather than an allocation.
    pub fn fill_upscaler_telemetry(&self, telemetry: &mut byroredux_core::ecs::UpscalerTelemetry) {
        // #2821 — carry the bracket's `_active` flag across with the value.
        // `0.0` from an absent timer pool, an upscaler that never dispatched,
        // and a genuinely sub-microsecond upscale are three different states;
        // `ctx.upscaler` used to print all three as `0.000`.
        let (upscale_ms, upscale_active) =
            self.gpu_timers.as_ref().map_or((0.0, false), |timers| {
                let snapshot = timers.last_snapshot();
                (snapshot.upscale_ms, snapshot.upscale_active)
            });
        telemetry.gpu_ms = upscale_ms;
        telemetry.gpu_ms_active = upscale_active;
        let Some(ref upscaler) = self.frame_upscaler else {
            telemetry.summary.clear();
            return;
        };
        let summary = upscaler.telemetry();
        if telemetry.summary != summary {
            telemetry.summary = summary;
        }
    }

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
            name: "frame_lights_scratch",
            len: self.frame_lights_scratch.len(),
            capacity: self.frame_lights_scratch.capacity(),
            elem_size_bytes: size_of::<scene_buffer::GpuLight>(),
        });
        rows.push(ScratchRow {
            name: "previous_models_scratch",
            len: self.previous_models_scratch.len(),
            capacity: self.previous_models_scratch.capacity(),
            elem_size_bytes: size_of::<scene_buffer::GpuPreviousModel>(),
        });
        rows.push(ScratchRow {
            name: "batches_scratch",
            len: self.batches_scratch.len(),
            capacity: self.batches_scratch.capacity(),
            elem_size_bytes: size_of::<draw::DrawBatch>(),
        });
        // #2486 / D5-01 — the two rigid-motion history maps are members of
        // the same per-frame scratch cluster (`clear` + `reserve` + shrink),
        // so they belong in the same report. Like the `skin_dispatch_seen`
        // HashSet row above, `capacity × elem_size` under-counts a hash
        // table's real footprint (no control bytes / load-factor slack);
        // it is a proportional signal, not an allocator-accurate figure.
        for (name, map) in [
            ("previous_rigid_models", &self.previous_rigid_models),
            (
                "current_rigid_models_scratch",
                &self.current_rigid_models_scratch,
            ),
        ] {
            rows.push(ScratchRow {
                name,
                len: map.len(),
                capacity: map.capacity(),
                elem_size_bytes: size_of::<(u32, [f32; 16])>(),
            });
        }
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
        // #1133 — skin-path scratches. The HashSet's heap footprint
        // isn't directly measurable through the public API; report
        // its `len` against `capacity` for what we can see.
        rows.push(ScratchRow {
            name: "skin_dispatch_seen_scratch",
            len: self.skin_dispatch_seen_scratch.len(),
            capacity: self.skin_dispatch_seen_scratch.capacity(),
            elem_size_bytes: size_of::<byroredux_core::ecs::storage::EntityId>(),
        });
        rows.push(ScratchRow {
            name: "skin_dispatches_scratch",
            len: self.skin_dispatches_scratch.len(),
            capacity: self.skin_dispatches_scratch.capacity(),
            elem_size_bytes: size_of::<(
                byroredux_core::ecs::storage::EntityId,
                super::skin_compute::SkinPushConstants,
                vk::Buffer,
                u32,
                u32,
            )>(),
        });
        rows.push(ScratchRow {
            name: "skin_first_sight_builds_scratch",
            len: self.skin_first_sight_builds_scratch.len(),
            capacity: self.skin_first_sight_builds_scratch.capacity(),
            elem_size_bytes: size_of::<(
                byroredux_core::ecs::storage::EntityId,
                vk::Buffer,
                u32,
                vk::Buffer,
                u32,
            )>(),
        });
        rows.push(ScratchRow {
            name: "skin_built_this_frame_scratch",
            len: self.skin_built_this_frame_scratch.len(),
            capacity: self.skin_built_this_frame_scratch.capacity(),
            elem_size_bytes: size_of::<byroredux_core::ecs::storage::EntityId>(),
        });
        if let Some(accel) = &self.accel_manager {
            let (len, capacity) = accel.tlas_instances_scratch_telemetry();
            rows.push(ScratchRow {
                name: "tlas_instances_scratch",
                len,
                capacity,
                elem_size_bytes: size_of::<vk::AccelerationStructureInstanceKHR>(),
            });
            // #3693 — the two sibling `AccelerationManager` scratches next
            // to `tlas_instances_scratch` that never got a row.
            let (len, capacity) = accel.tlas_addresses_scratch_telemetry();
            rows.push(ScratchRow {
                name: "tlas_addresses_scratch",
                len,
                capacity,
                elem_size_bytes: size_of::<u64>(),
            });
            let (len, capacity) = accel.tlas_missing_samples_scratch_telemetry();
            rows.push(ScratchRow {
                name: "tlas_missing_samples_scratch",
                len,
                capacity,
                elem_size_bytes: size_of::<String>(),
            });
        } else {
            rows.push(ScratchRow {
                name: "tlas_instances_scratch",
                len: 0,
                capacity: 0,
                elem_size_bytes: size_of::<vk::AccelerationStructureInstanceKHR>(),
            });
            rows.push(ScratchRow {
                name: "tlas_addresses_scratch",
                len: 0,
                capacity: 0,
                elem_size_bytes: size_of::<u64>(),
            });
            rows.push(ScratchRow {
                name: "tlas_missing_samples_scratch",
                len: 0,
                capacity: 0,
                elem_size_bytes: size_of::<String>(),
            });
        }
        // #3061 / dim_2 converted this to `FxHashSet` for its hasher; #3693
        // is the telemetry half that was never added alongside it. Same
        // len/capacity-only caveat as the other hash-container rows above.
        rows.push(ScratchRow {
            name: "blend_seen_scratch",
            len: self.blend_seen_scratch.len(),
            capacity: self.blend_seen_scratch.capacity(),
            elem_size_bytes: size_of::<(u8, u8, bool, bool)>(),
        });
        if let Some(water) = &self.water {
            let (len, capacity) = water.param_scratch_telemetry();
            rows.push(ScratchRow {
                name: "water_param_scratch",
                len,
                capacity,
                elem_size_bytes: size_of::<super::water::GpuWaterParams>(),
            });
        } else {
            rows.push(ScratchRow {
                name: "water_param_scratch",
                len: 0,
                capacity: 0,
                elem_size_bytes: size_of::<super::water::GpuWaterParams>(),
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
        stats.dispatches_skipped = f.dispatches_skipped;
        stats.first_sight_attempted = f.first_sight_attempted;
        stats.first_sight_succeeded = f.first_sight_succeeded;
        stats.refits_attempted = f.refits_attempted;
        stats.refits_succeeded = f.refits_succeeded;
        // #2803 — host-side chain cost, same-frame (not fence-lagged
        // like the GPU brackets below).
        stats.cpu_skin_chain_ms = f.cpu_skin_chain_ns as f32 / 1.0e6;
        // #1194 — GPU timer snapshot. Zeros when timer unavailable
        // (driver lacks timestamp support) or first pipelined cycle
        // hasn't completed.
        if let Some(ref timers) = self.gpu_timers {
            let snap = timers.last_snapshot();
            stats.gpu_skin_dispatch_ms = snap.skin_dispatch_ms;
            stats.gpu_skin_palette_ms = snap.skin_palette_ms;
            stats.gpu_skin_blas_refit_ms = snap.skin_blas_refit_ms;
            stats.gpu_taa_ms = snap.taa_ms;
            stats.gpu_main_render_ms = snap.main_render_ms;
            stats.gpu_tlas_build_ms = snap.tlas_build_ms;
            stats.gpu_cluster_cull_ms = snap.cluster_cull_ms;
            stats.gpu_svgf_ms = snap.svgf_ms;
            stats.gpu_composite_ms = snap.composite_ms;
            stats.gpu_ssao_ms = snap.ssao_ms;
            stats.gpu_bloom_ms = snap.bloom_ms;
            stats.gpu_caustic_splat_ms = snap.caustic_splat_ms;
            stats.gpu_volumetrics_ms = snap.volumetrics_ms;
            stats.gpu_upscale_ms = snap.upscale_ms;
            stats.gpu_presentation_ms = snap.presentation_ms;
            stats.gpu_depth_history_copy_ms = snap.depth_history_copy_ms;
            // #2513 / REN-D20-NEW-03 — copy the "did this bracket actually
            // run" flags too, closing the gap #2278 opened at the producer
            // but nothing downstream ever read.
            stats.gpu_skin_dispatch_active = snap.skin_dispatch_active;
            stats.gpu_skin_palette_active = snap.skin_palette_active;
            stats.gpu_skin_blas_refit_active = snap.skin_blas_refit_active;
            stats.gpu_taa_active = snap.taa_active;
            stats.gpu_main_render_active = snap.main_render_active;
            stats.gpu_tlas_build_active = snap.tlas_build_active;
            stats.gpu_cluster_cull_active = snap.cluster_cull_active;
            stats.gpu_svgf_active = snap.svgf_active;
            stats.gpu_composite_active = snap.composite_active;
            stats.gpu_ssao_active = snap.ssao_active;
            stats.gpu_bloom_active = snap.bloom_active;
            stats.gpu_caustic_splat_active = snap.caustic_splat_active;
            stats.gpu_volumetrics_active = snap.volumetrics_active;
            stats.gpu_upscale_active = snap.upscale_active;
            stats.gpu_presentation_active = snap.presentation_active;
            stats.gpu_depth_history_copy_active = snap.depth_history_copy_active;
        } else {
            stats.gpu_skin_dispatch_ms = 0.0;
            stats.gpu_skin_palette_ms = 0.0;
            stats.gpu_skin_blas_refit_ms = 0.0;
            stats.gpu_taa_ms = 0.0;
            stats.gpu_main_render_ms = 0.0;
            stats.gpu_tlas_build_ms = 0.0;
            stats.gpu_cluster_cull_ms = 0.0;
            stats.gpu_svgf_ms = 0.0;
            stats.gpu_composite_ms = 0.0;
            stats.gpu_ssao_ms = 0.0;
            stats.gpu_bloom_ms = 0.0;
            stats.gpu_caustic_splat_ms = 0.0;
            stats.gpu_volumetrics_ms = 0.0;
            stats.gpu_upscale_ms = 0.0;
            stats.gpu_presentation_ms = 0.0;
            stats.gpu_depth_history_copy_ms = 0.0;
            // No `GpuPerFrameTimers` at all (driver lacks timestamp
            // support) — every bracket is inactive, not just zero.
            stats.gpu_skin_dispatch_active = false;
            stats.gpu_skin_palette_active = false;
            stats.gpu_skin_blas_refit_active = false;
            stats.gpu_taa_active = false;
            stats.gpu_main_render_active = false;
            stats.gpu_tlas_build_active = false;
            stats.gpu_cluster_cull_active = false;
            stats.gpu_svgf_active = false;
            stats.gpu_composite_active = false;
            stats.gpu_ssao_active = false;
            stats.gpu_bloom_active = false;
            stats.gpu_caustic_splat_active = false;
            stats.gpu_volumetrics_active = false;
            stats.gpu_upscale_active = false;
            stats.gpu_presentation_active = false;
            stats.gpu_depth_history_copy_active = false;
        }
        stats.slots_active = self.skin_slots.len() as u32;
        stats.slot_pool_capacity = if self.skin_compute.is_some() {
            SKIN_MAX_SLOTS
        } else {
            0
        };
        stats.slots_failed = self.failed_skin_slots.len() as u32;
        let (morph_slots, morph_bytes) = self.morph_memory_usage();
        stats.morph_slots = morph_slots;
        stats.morph_bytes = morph_bytes;
        stats.failed_entity_ids.clear();
        for &eid in self.failed_skin_slots.iter().take(16) {
            stats.failed_entity_ids.push(eid);
        }
    }

    /// Join the last frame's RT publication, TLAS membership, and clustered
    /// light capacity counters into the engine's durable integrity resource.
    pub fn fill_rt_integrity_stats(&self, stats: &mut byroredux_core::ecs::RtIntegrityStats) {
        let tlas = self
            .accel_manager
            .as_ref()
            .map(super::acceleration::AccelerationManager::integrity_snapshot)
            .unwrap_or_default();
        let cluster = self
            .cluster_cull
            .as_ref()
            .map(super::compute::ClusterCullPipeline::latest_telemetry)
            .unwrap_or_default();
        let (lights_submitted, lights_uploaded) = self.scene_buffers.light_upload_counts();

        stats.frame = tlas.frame;
        stats.sampled = self.frame_counter > 0;
        stats.rt_supported = self.device_caps.ray_query_supported;
        stats.rt_flag = self.rt_flag_last_frame;
        stats.tlas_build_succeeded = self.tlas_build_succeeded_last_frame;
        stats.tlas_eligible = tlas.eligible;
        stats.tlas_emitted = tlas.emitted;
        stats.missing_skinned_blas = tlas.missing_skinned_blas;
        stats.missing_rigid_blas = tlas.missing_rigid_blas;
        stats.missing_ssbo_instance = tlas.missing_ssbo_instance;
        stats.lights_submitted = lights_submitted;
        stats.lights_uploaded = lights_uploaded;
        stats.lights_dropped = lights_submitted.saturating_sub(lights_uploaded);
        stats.cluster_sampled = cluster.sampled;
        stats.cluster_overflowed = cluster.overflowed_clusters;
        stats.cluster_dropped = cluster.dropped_lights;
        stats.cluster_max_lights = cluster.max_lights;
    }

    // draw_frame is in draw.rs
    // register_ui_quad, swapchain_extent, log_memory_usage are in resources.rs
    // recreate_swapchain is in resize.rs

    /// Initialise the debug-UI overlay pass (Phase 4 of the
    /// debug-UI plan). Called once by the binary after
    /// `VulkanContext::new` returns and the allocator is wired into
    /// the world. Idempotent — repeated calls reuse the existing
    /// pass instead of leaking GPU resources.
    pub fn init_egui(&mut self, in_flight_frames: usize) -> anyhow::Result<()> {
        if self.egui_pass.is_some() {
            return Ok(());
        }
        let allocator = self.allocator.clone().ok_or_else(|| {
            anyhow::anyhow!("VulkanContext::init_egui: allocator not initialised")
        })?;
        let pass = super::egui_pass::EguiPass::new(
            self.device.clone(),
            allocator,
            self.swapchain_state.format.format,
            &self.swapchain_state.image_views,
            self.swapchain_state.extent,
            in_flight_frames,
        )?;
        self.egui_pass = Some(pass);
        Ok(())
    }

    /// Stash one frame's egui context + `FullOutput`. The next
    /// `draw_frame` consumes it after composite. Called from the
    /// binary's main loop right before invoking `draw_frame`. No-op
    /// when [`Self::egui_pass`] hasn't been initialised — the
    /// overlay is opt-in.
    ///
    /// #2247 (REN-D20-01) — if a previous frame's output is still
    /// pending (an earlier `draw_frame` returned early, or otherwise
    /// skipped the egui block, before consuming it), merge into it via
    /// `FullOutput::append` instead of overwriting. `append` accumulates
    /// `textures_delta` (so a skipped frame's texture uploads/frees still
    /// reach the GPU) while keeping only the newest `shapes`/
    /// `pixels_per_point` — the same semantics egui's own multi-frame
    /// integrations use.
    pub fn submit_egui_frame(&mut self, ctx: egui::Context, output: egui::FullOutput) {
        if self.egui_pass.is_some() {
            self.egui_pending_output = Some(merge_egui_pending_output(
                self.egui_pending_output.take(),
                ctx,
                output,
            ));
        }
    }
}

/// Pure merge logic behind [`VulkanContext::submit_egui_frame`], split out
/// so it's unit-testable without a live Vulkan device. See that method's
/// doc comment / #2247 for the rationale.
fn merge_egui_pending_output(
    pending: Option<(egui::Context, egui::FullOutput)>,
    ctx: egui::Context,
    output: egui::FullOutput,
) -> (egui::Context, egui::FullOutput) {
    match pending {
        Some((_, mut older)) => {
            older.append(output);
            (ctx, older)
        }
        None => (ctx, output),
    }
}

#[cfg(test)]
mod egui_pending_output_tests {
    use super::*;
    use egui::epaint::ImageDelta;
    use egui::{Color32, ColorImage, TextureId, TextureOptions};

    fn full_output_with_delta(set_id: u64, free_id: u64) -> egui::FullOutput {
        let mut output = egui::FullOutput::default();
        output.textures_delta.set.push((
            TextureId::Managed(set_id),
            ImageDelta::full(
                ColorImage::new([1, 1], vec![Color32::WHITE]),
                TextureOptions::default(),
            ),
        ));
        output.textures_delta.free.push(TextureId::Managed(free_id));
        output
    }

    /// Regression for #2247 (REN-D20-01): a skipped frame's pending
    /// `textures_delta` must survive into whichever `submit_egui_frame`
    /// call actually gets consumed next, instead of being silently
    /// dropped by a plain overwrite.
    #[test]
    fn pending_output_accumulates_textures_delta_instead_of_overwriting() {
        let frame_a = full_output_with_delta(1, 10);
        let frame_b = full_output_with_delta(2, 20);

        let (_, merged) = merge_egui_pending_output(
            Some((egui::Context::default(), frame_a)),
            egui::Context::default(),
            frame_b,
        );

        assert_eq!(
            merged
                .textures_delta
                .set
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            vec![TextureId::Managed(1), TextureId::Managed(2)],
            "both frames' texture uploads must survive the merge, oldest first"
        );
        assert_eq!(
            merged.textures_delta.free,
            vec![TextureId::Managed(10), TextureId::Managed(20)],
            "both frames' texture frees must survive the merge"
        );
    }

    /// No pending output (the common case — every frame is consumed
    /// before the next is submitted) must pass the new output through
    /// unchanged, not silently wrap or duplicate it.
    #[test]
    fn no_pending_output_passes_through_unchanged() {
        let frame = full_output_with_delta(1, 10);
        let (_, merged) = merge_egui_pending_output(None, egui::Context::default(), frame);
        assert_eq!(merged.textures_delta.set.len(), 1);
        assert_eq!(merged.textures_delta.free, vec![TextureId::Managed(10)]);
    }
}

// Method implementations split across submodules:
mod draw;
pub use draw::FrameInputs;
mod assemble_camera_and_lights;
mod begin_frame_recording;
mod build_and_upload_instances;
mod depth_capture;
mod dispatch_skin_and_cluster;
mod geometry_pass;
mod helpers;
mod init;
mod post_passes;
mod render_debug;
mod resize;
mod resources;
mod screenshot;
mod skinned_blas_refit;
mod sync_and_acquire_frame;
mod teardown;

// destroy_allocator_owned_resources + impl Drop for VulkanContext moved
// to teardown.rs (#1749 / TD1-004) — the mirror image of init.rs.

// Helper functions are in helpers.rs — use helpers:: prefix.
use helpers::{
    allocate_command_buffers, check_gbuffer_color_formats, create_command_pool,
    create_depth_history_sampler, create_depth_resources, create_main_framebuffers,
    create_render_pass, create_transfer_pool, destroy_depth_resources, destroy_main_framebuffers,
    destroy_render_pass_pipelines, find_depth_format, init_depth_history_layout,
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
            no_sorter: false,
            wireframe: false,
            flat_shading: false,
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
            // #1248 — distinct non-default IOR so the material_hash
            // walk and to_gpu_material round-trip both surface this
            // field, mirroring the other intentionally-non-default
            // values in this fixture.
            ior: 1.45,
            // #1249 — distinct non-default Disney lobe values so the
            // hash walk exercises each independently.
            subsurface: 0.42,
            sheen: 0.18,
            sheen_tint: 0.66,
            // #1250 — distinct non-default anisotropy so the hash
            // walk exercises the field independently.
            anisotropic: 0.27,
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
            // Non-zero LUT handle so the hash-walk contract covers this
            // field (zero would dedup with the default and hide a drift).
            greyscale_lut_index: 7,
            supplemental_texture_indices: [
                31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46,
            ],
            // Non-zero translucency so the hash-walk covers these
            // fields too (#1147 Phase 2b).
            translucency_subsurface_color: [0.5, 0.4, 0.3],
            translucency_transmissive_scale: 1.5,
            translucency_turbulence: 0.25,
            // #2221 — distinct non-default shader color/float so the
            // hash-walk contract covers these fields independently.
            shader_color: [0.15, 0.25, 0.35],
            shader_float: 0.45,
            glass_fresnel_color: [0.81, 0.72, 0.63],
            glass_refraction_scale: 0.09,
            glass_blur_scale: 0.31,
            glass_blur_scale_factor: 1.7,
            lighting_effect_1: 0.21,
            lighting_effect_2: 0.22,
            subsurface_rolloff: 0.23,
            rimlight_power: 2.4,
            backlight_power: 0.25,
            fresnel_power: 4.6,
            grayscale_to_palette_scale: 0.27,
            is_water: false,
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

/// Regression for #1783 / CONC-D2-01. `couple_skin_compute_to_palette` is
/// the fault-injection seam for the two-pipeline coupling: a live Vulkan
/// device is needed to actually exercise `SkinComputePipeline::new` /
/// `SkinPaletteComputePipeline::new` failing, but the coupling DECISION
/// itself is pure and fully covered here.
#[cfg(test)]
mod skin_pipeline_coupling_tests {
    use super::couple_skin_compute_to_palette;

    /// The bug this issue is about: `skin_palette` failed to initialise
    /// while `skin_compute` succeeded. Must force `skin_compute` off too,
    /// so every `skin_compute.is_some()` consumer gate
    /// (`record_skinned_blas_refit`'s refit dispatch chain) is skipped —
    /// otherwise it would dispatch against the never-written palette SSBO.
    #[test]
    fn palette_failure_forces_compute_off_even_if_compute_succeeded() {
        let skin_compute = Some(42u32); // stand-in for SkinComputePipeline
        assert_eq!(couple_skin_compute_to_palette(skin_compute, false), None);
    }

    /// Both pipelines up (the common RT-capable path) — `skin_compute`
    /// must pass through unchanged.
    #[test]
    fn both_present_leaves_compute_untouched() {
        let skin_compute = Some(42u32);
        assert_eq!(
            couple_skin_compute_to_palette(skin_compute, true),
            Some(42u32)
        );
    }

    /// RT unsupported (both `None` before this call) — stays `None`, not
    /// a regression of the existing "no RT" behavior.
    #[test]
    fn both_absent_stays_none() {
        let skin_compute: Option<u32> = None;
        assert_eq!(couple_skin_compute_to_palette(skin_compute, false), None);
    }

    /// Degenerate case the real call site never produces (construction
    /// always gates both pipelines on the same `device_caps.ray_query_
    /// supported`, so `skin_compute = None, skin_palette_ok = true`
    /// shouldn't occur) — still must not panic or fabricate a value.
    #[test]
    fn compute_absent_but_palette_ok_stays_none() {
        let skin_compute: Option<u32> = None;
        assert_eq!(couple_skin_compute_to_palette(skin_compute, true), None);
    }
}

/// Regression for #3138: the water pipeline must never survive init with an
/// unwritten storage-image descriptor.
#[cfg(test)]
mod water_caustic_coupling_tests {
    use super::couple_water_to_caustic_sink;

    #[test]
    fn either_real_or_placeholder_sink_keeps_water_enabled() {
        assert_eq!(
            couple_water_to_caustic_sink(Some(7u32), true, false),
            Some(7)
        );
        assert_eq!(
            couple_water_to_caustic_sink(Some(7u32), false, true),
            Some(7)
        );
        assert_eq!(
            couple_water_to_caustic_sink(Some(7u32), true, true),
            Some(7)
        );
    }

    #[test]
    fn losing_both_sinks_disables_water() {
        assert_eq!(couple_water_to_caustic_sink(Some(7u32), false, false), None);
    }
}

/// #2174 / D2-03 + PERF-D4-03 — pin the hasher on the rigid motion-history
/// maps.
///
/// A source assertion rather than a type assertion: the two fields are
/// private and live on a struct that needs a real Vulkan device to build, so
/// there is no value to inspect from a test. The declaration text is the
/// only device-free observable, and it is exactly what regressed — `33d9a468`
/// reintroduced `std::collections::HashMap` at a new per-draw site after
/// #1368 had removed it elsewhere, which is how this went unnoticed.
#[cfg(test)]
mod rigid_history_hasher_tests {
    // #1749 / TD1-004 — the field DECLARATIONS (`VulkanContext`'s struct
    // definition) stayed in `mod.rs`; the field CONSTRUCTION
    // (`FxHashMap::default()`, in the final struct literal) moved to
    // `init.rs`'s `build_pipelines_and_finish` along with the rest of
    // `VulkanContext::new`. Concatenate both so the checks below still see
    // everything they need in one string. `init.rs` carries no
    // `#[cfg(test)]` section of its own, so only `mod.rs`'s own test
    // modules need stripping — done first, before the concat, so a
    // needle can't match this file's own assertion strings.
    fn production_src() -> String {
        const CONTEXT_MOD_RS: &str = include_str!("mod.rs");
        let mod_production = CONTEXT_MOD_RS
            .split_once("\n#[cfg(test)]")
            .expect("context/mod.rs lost its test modules")
            .0;
        format!("{mod_production}\n{}", include_str!("init.rs"))
    }

    #[test]
    fn rigid_motion_history_maps_are_not_siphash() {
        let src = production_src();
        for field in ["previous_rigid_models", "current_rigid_models_scratch"] {
            let decl = format!("{field}: FxHashMap<u32, [f32; 16]>");
            assert!(
                src.contains(&decl),
                "`{field}` is no longer declared `{decl}` — these two maps are \
                 probed twice per rigid draw per frame (~29K probes on MedTek), \
                 which is what makes SipHash-1-3 the wrong default here (#2174)",
            );
        }
        // #3061 — this used to assert the WHOLE file contained exactly two
        // `FxHashMap::default()` calls. That was a proxy for "neither
        // rigid-history map reverted to `HashMap::new()`", and it stopped
        // being one the moment an unrelated field was converted: the count
        // broke while both maps it cares about were still correct. Name the
        // two constructions instead — strictly stronger, and it says what it
        // means.
        for field in ["previous_rigid_models", "current_rigid_models_scratch"] {
            assert!(
                src.contains(&format!("{field}: FxHashMap::default(),")),
                "`{field}` is no longer CONSTRUCTED as an `FxHashMap` — a \
                 `HashMap::new()` crept back into it (#2174)",
            );
        }
        assert_eq!(
            src.matches("std::collections::HashMap::new()").count(),
            0,
            "a std `HashMap::new()` reappeared in the context constructor \
             (#2174 / #3061)",
        );
    }

    /// #2923 / PERF-D1-01 — the skinning path's half of the same pattern.
    /// `#1368` and `#2174` both stopped at the `crates/renderer` boundary;
    /// the skinning path crosses it, so `record_skinned_blas_refit`'s
    /// once-per-skinned-BLAS `pose_dirty.contains()` probe was forced back
    /// onto SipHash purely because `SkinSlotPool::pose_dirty()` returned a
    /// std `HashSet` and `FrameInputs` pinned that type in its public
    /// signature. Pinned here as well as in `skin_slot_pool.rs` because
    /// either side alone can drag the other back.
    #[test]
    fn pose_dirty_crosses_the_crate_boundary_without_siphash() {
        for (src, what) in [
            (
                include_str!("draw.rs"),
                "FrameInputs.pose_dirty — the field that pins the type across \
                 the core/renderer boundary",
            ),
            (
                include_str!("skinned_blas_refit.rs"),
                "record_skinned_blas_refit's pose_dirty parameter — probed once \
                 per skinned BLAS entry per frame",
            ),
        ] {
            assert!(
                src.contains("rustc_hash::FxHashSet<EntityId>"),
                "{what} must stay `FxHashSet` (#2923)"
            );
            assert!(
                !src.contains("std::collections::HashSet<EntityId>"),
                "{what} reverted to std's SipHash-1-3 (#2923)"
            );
        }

        // #3045 / REN-D9-01 then #3061 / PERF-D6-01 — the rest of the
        // cluster #2923 stopped short of. Declared on `VulkanContext` rather
        // than in a signature, so they carry the struct's own spelling;
        // asserted here rather than in the loop above for that reason alone.
        //
        // Each entry is (field, the declared type text). Spelled per field
        // rather than checked with a blanket "no std collection in this file"
        // sweep, which would pass today only by accident of what happens to
        // have been converted and would say nothing about the next addition.
        let src = production_src();
        for (field, ty) in [
            (
                "skin_dispatch_seen_scratch",
                "FxHashSet<byroredux_core::ecs::storage::EntityId>",
            ),
            (
                "skin_built_this_frame_scratch",
                "FxHashSet<byroredux_core::ecs::storage::EntityId>",
            ),
            (
                "failed_skin_slots",
                "FxHashSet<byroredux_core::ecs::storage::EntityId>",
            ),
            (
                "failed_skin_blas",
                "FxHashSet<byroredux_core::ecs::storage::EntityId>",
            ),
            ("blend_seen_scratch", "FxHashSet<(u8, u8, bool, bool)>"),
            (
                "blend_pipeline_cache",
                "FxHashMap<(u8, u8, bool, bool), vk::Pipeline>",
            ),
        ] {
            let decl = format!("{field}: {ty},");
            assert!(
                src.contains(&decl),
                "`{field}` is no longer declared `{decl}` — every one of these \
                 is probed once per skinned entity (or per blend batch) per \
                 frame on the dispatch path the hot-path hashing rule names, \
                 and none is keyed by anything an attacker chooses, so \
                 SipHash-1-3 is the wrong default here (#3061, following \
                 #1368 / #2174 / #2923 / #3045)",
            );
        }
        // `skin_slots` and `morph_slots` wrap across two lines under rustfmt,
        // so match their declarations rather than a single-line `field: ty,`.
        for (field, decl) in [
            (
                "skin_slots",
                "pub skin_slots:\n        FxHashMap<byroredux_core::ecs::storage::EntityId, \
                 super::skin_compute::SkinSlot>,",
            ),
            (
                "morph_slots",
                "pub morph_slots:\n        FxHashMap<byroredux_core::ecs::storage::EntityId, \
                 super::morph_compute::MorphSlot>,",
            ),
        ] {
            assert!(
                src.contains(decl),
                "`{field}` is no longer an `FxHashMap` — `draw.rs` probes it \
                 once per draw command per frame (#3061)",
            );
        }
    }

    /// #3682 — same cluster (#1368 → #2174 → #2923 → #3045 → #3061), but
    /// the fix taken here is the issue's own first-choice option rather
    /// than its "minimum" fallback: instead of swapping
    /// `std::collections::HashMap` for `FxHashMap` on a per-`TextureHandle`
    /// map, the two hottest per-draw reads in this cluster (`has_alpha` /
    /// `avg_rgb`, read once or twice per `GpuInstance` built — up to ~4k
    /// probes/frame) were moved onto `TextureEntry` itself. `TextureHandle`
    /// is a dense index into `textures: Vec<TextureEntry>`, so there is
    /// nothing left to hash. `texture_registry.rs` sat outside every prior
    /// `context/`-only sweep in this lineage; extends the corpus into it so
    /// the cluster can't quietly re-grow there.
    #[test]
    fn texture_alpha_and_avg_rgb_are_not_hashed_by_texture_handle() {
        let src = include_str!("../../texture_registry.rs");
        for field in ["texture_has_alpha", "texture_avg_rgb"] {
            assert!(
                !src.contains(&format!("{field}: HashMap<TextureHandle"))
                    && !src.contains(&format!("{field}: FxHashMap<TextureHandle")),
                "`{field}` reappeared as a per-handle hash map — \
                 `TextureHandle` is a dense index into `textures: \
                 Vec<TextureEntry>`, so this belongs on `TextureEntry` \
                 itself, not behind any hasher (#3682)",
            );
        }
        for field_decl in ["has_alpha: bool,", "avg_rgb: Option<[f32; 3]>,"] {
            assert!(
                src.contains(field_decl),
                "`TextureEntry` no longer declares `{field_decl}` — the \
                 per-draw read in the `GpuInstance` build loop depends on \
                 this being a direct `Vec` index (#3682)",
            );
        }
    }
}

/// #3693 — `fill_scratch_telemetry`'s own doc states the maintenance rule:
/// every persistent `Vec` (or hash-container) scratch declared in this
/// crate must show up as a row. `VulkanContext::new` needs a live Vulkan
/// device to actually call the function, so (matching this file's
/// `rigid_history_hasher_tests` pattern) this pins the four rows the issue
/// found missing at the source level instead.
#[cfg(test)]
mod scratch_telemetry_coverage_tests {
    /// Scoped to the production portion of this file (everything before
    /// its first `#[cfg(test)]` module) — this module's own assertions
    /// reference the same row-name strings they check for, so an unscoped
    /// search would self-match regardless of the producer's state. Same
    /// convention `rigid_history_hasher_tests::production_src` and
    /// #3674/#3675/#3690 established.
    fn production_src() -> &'static str {
        let full_src = include_str!("mod.rs");
        full_src
            .split_once("\n#[cfg(test)]")
            .expect("context/mod.rs lost its test modules")
            .0
    }

    #[test]
    fn fill_scratch_telemetry_covers_all_four_previously_missing_scratches() {
        let full_src = production_src();
        let fn_start = full_src
            .find("pub fn fill_scratch_telemetry(")
            .expect("fill_scratch_telemetry must still exist");
        let fn_end = full_src[fn_start..]
            .find("\n    pub fn fill_skin_coverage_stats(")
            .map(|rel| fn_start + rel)
            .expect("fill_skin_coverage_stats must still follow fill_scratch_telemetry");
        let body = &full_src[fn_start..fn_end];

        for name in [
            "blend_seen_scratch",
            "tlas_addresses_scratch",
            "tlas_missing_samples_scratch",
            "water_param_scratch",
        ] {
            assert!(
                body.contains(&format!("name: \"{name}\"")),
                "`fill_scratch_telemetry` no longer emits a row named \
                 \"{name}\" — every persistent scratch declared in this \
                 crate must show up here, per the function's own \
                 maintenance rule (#3693)",
            );
        }
    }
}

// #2480 / REN-D23-2026-08-07-01 — FSR startup-failure must promote to TAA,
// not silently stay at the reduced FSR render extent with no temporal AA.
// `VulkanContext::new` needs a live Vulkan device + a forced FSR SDK
// failure to exercise end-to-end, so (matching this file's
// `rigid_history_hasher_tests` pattern above) this pins the fix at the
// source level.
#[cfg(test)]
mod fsr_startup_failure_promotes_to_taa_tests {
    // #1749 / TD1-004 — the FSR-promotion tail of `VulkanContext::new`
    // (frame_upscaler build → struct literal → promotion guard →
    // set_upscaler_mode → Ok(context)) moved to `init.rs`'s
    // `build_pipelines_and_finish`.
    fn production_src() -> &'static str {
        include_str!("init.rs")
    }

    #[test]
    fn new_promotes_to_taa_when_fsr_context_creation_failed() {
        let src = production_src();

        let upscaler_built_pos = src
            .find("let frame_upscaler = FrameUpscaler::new(")
            .expect("VulkanContext::new must still construct the frame upscaler");
        let struct_open_pos = src.find("let mut context = Self {").expect(
            "VulkanContext::new must bind the constructed context to a \
                     mutable local so a post-construction promotion can run \
                     before it is wrapped in Ok(..) (#2480)",
        );
        let guard_pos = src
            .find("matches!(context.renderer_config.upscaler, UpscalerMode::Fsr3(_))")
            .expect("the promotion must be gated on the mode actually being FSR");
        let dispatch_check_pos = src
            .find("is_some_and(FrameUpscaler::is_fsr_dispatch_active)")
            .expect(
                "the promotion must check is_fsr_dispatch_active() — `context: None` \
                 after a failed FSR create is exactly what that reports",
            );
        let switch_pos = src
            .find("context.set_upscaler_mode(UpscalerMode::Taa, window_size)")
            .expect(
                "startup FSR failure must promote via set_upscaler_mode(Taa, ..) — \
                 the same rollback-safe runtime switch the --upscaler CLI flag uses, \
                 not a bespoke construction-time TAA build (#2480)",
            );
        let return_pos = src
            .find("Ok(context)")
            .expect("VulkanContext::new must return the (possibly promoted) context");

        assert!(upscaler_built_pos < struct_open_pos);
        assert!(
            struct_open_pos < guard_pos,
            "the guard must run AFTER the struct exists"
        );
        assert!(guard_pos < dispatch_check_pos && dispatch_check_pos < switch_pos);
        assert!(
            switch_pos < return_pos,
            "the promotion must run before the constructor returns — a caller \
             must never observe the pre-promotion, reduced-extent state"
        );
    }
}
