//! Per-frame scene data buffers for multi-light rendering.
//!
//! Manages an SSBO for the light array and a UBO for camera data,
//! with per-frame-in-flight double-buffering to avoid write-after-read hazards.
//! Bound as descriptor set 1 in the pipeline layout.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;

/// Maximum lights we can upload per frame. The SSBO is pre-allocated to this size.
/// 512 lights × 48 bytes = 24 KB per frame — trivial.
const MAX_LIGHTS: usize = 512;

/// Maximum bones we can upload per frame across all skinned meshes.
/// 4096 × 64 B = 256 KB/frame. Slot 0 is a reserved identity fallback
/// (used by rigid vertices through the sum-of-weights escape hatch and
/// by `SkinnedMesh` bones that failed to resolve). The remaining 4095
/// slots are assigned sequentially per skinned mesh, with each mesh
/// consuming `MAX_BONES_PER_MESH` (128) slots for simplicity. That
/// gives ~31 skinned meshes per frame — more than enough for a cell
/// full of actors.
pub const MAX_TOTAL_BONES: usize = 4096;

/// Slot 0 of the bone palette is always the identity matrix.
pub const IDENTITY_BONE_SLOT: u32 = 0;

/// Maximum instances per frame. 8192 × 352 B = 2.81 MB/frame — trivial.
/// Covers large exterior cells with multiple loaded cells (~5000+ references).
pub const MAX_INSTANCES: usize = 8192;

/// Maximum number of `VkDrawIndexedIndirectCommand` entries held in
/// the per-frame indirect buffer. Each entry is 20 bytes, so 8192
/// entries × 20 B = 160 KB per frame × 3 frames-in-flight = 480 KB
/// total. Sized generously — real scenes with instanced batching from
/// #272 rarely emit more than a few hundred entries. See #309.
pub const MAX_INDIRECT_DRAWS: usize = 8192;

/// Maximum number of `GpuTerrainTile` slots held in the per-frame
/// terrain-tile SSBO. 1024 × 32 B = 32 KB per frame — one slot per
/// terrain-mesh entity. A 3×3 loaded-cell grid emits 9 tiles; larger
/// exterior loads stay well under the cap. Capped at 65535 by the
/// 16-bit index packed into `GpuInstance.flags` (bits 16..31). See #470.
pub const MAX_TERRAIN_TILES: usize = 1024;

/// Per-instance flag bits on [`GpuInstance::flags`].
/// Kept in lockstep with the inline comments in `draw.rs` flag assembly
/// and with the fragment shader's `flags & N` checks.
pub const INSTANCE_FLAG_NON_UNIFORM_SCALE: u32 = 1 << 0;
pub const INSTANCE_FLAG_ALPHA_BLEND: u32 = 1 << 1;
pub const INSTANCE_FLAG_CAUSTIC_SOURCE: u32 = 1 << 2;
/// Terrain splat bit — tells the fragment shader to consume the
/// per-vertex splat weights (locations 6/7) and sample the 8 layer
/// textures indexed by `GpuTerrainTile` at the tile index packed into
/// the top 16 bits of `flags`. See #470.
pub const INSTANCE_FLAG_TERRAIN_SPLAT: u32 = 1 << 3;
/// Bit offset for the terrain tile index inside `GpuInstance.flags`.
/// `(flags >> INSTANCE_TERRAIN_TILE_SHIFT) & 0xFFFF` yields the tile slot.
pub const INSTANCE_TERRAIN_TILE_SHIFT: u32 = 16;
pub const INSTANCE_TERRAIN_TILE_MASK: u32 = 0xFFFF;

/// Engine-synthesized material kinds for [`GpuInstance::material_kind`].
///
/// The low range (0..=19) is reserved for Skyrim+
/// `BSLightingShaderProperty.shader_type` values the NIF importer
/// forwards verbatim — `SkinTint`, `HairTint`, `EyeEnvmap`, etc.
/// (see #344). The high range (100..) is reserved for kinds the
/// engine classifies itself from heuristics against the NIF material.
///
/// `Glass` is the first such kind (#Tier C Phase 2): alpha-blend
/// material, metalness < 0.3, not a decal. The fragment shader branches
/// on this value to dispatch the RT reflection + refraction path —
/// replaces the pre-Phase-2 per-pixel `texColor.a` heuristic that
/// flickered across textures. Callers (`render.rs`) must compute the
/// kind BEFORE populating `DrawCommand.material_kind`.
pub const MATERIAL_KIND_GLASS: u32 = 100;

/// `EffectShader` (`#706` / FX-1): Skyrim+ `BSEffectShaderProperty`
/// surface — fire flames, magic auras, glow rings, force fields, dust
/// planes, decals over emissive cones. The fragment shader branches on
/// this value to short-circuit lit shading: no scene point/spot lights,
/// no ambient, no GI bounce reads — output is `emissive_color *
/// emissive_mult * texColor.rgba`. Without this branch, fires get
/// modulated by every nearby lantern + ambient term + RT GI bounce,
/// producing rainbow-tinted flames where Bethesda authored a pure
/// orange/yellow additive surface.
///
/// Callers (`render.rs`) override the base shader_type-derived kind
/// to this value when `Material.effect_shader.is_some()`. Pre-existing
/// effect-shader data (falloff cone, greyscale palette, lighting_influence)
/// captured via #345 rides through on the same instance — the variant
/// branch in the fragment shader is the missing renderer-side dispatch
/// (SK-D3-02 follow-up).
pub const MATERIAL_KIND_EFFECT_SHADER: u32 = 101;

/// Per-terrain-tile data uploaded to the terrain-tile SSBO each cell load.
/// Indexed in the fragment shader via
/// `(instance.flags >> 16) & 0xFFFF` when the splat bit is set.
/// 8 × u32 = 32 bytes, std430-compatible.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GpuTerrainTile {
    /// Bindless texture indices for layers 0-7. Unused slots are 0
    /// (the fallback "error" texture); splat weights for unused layers
    /// are zero so the fragment's `mix` is a no-op.
    pub layer_texture_index: [u32; 8],
}

/// Per-instance data uploaded to the instance SSBO each frame.
///
/// The vertex shader reads `instances[gl_InstanceIndex]` instead of push
/// constants, enabling instanced drawing: consecutive draws with the same
/// mesh + pipeline can be batched into a single `cmd_draw_indexed` call.
///
/// **CRITICAL**: All fields use scalar types (f32/u32) or vec4-equivalent
/// `[f32; 4]` — NEVER `[f32; 3]`. In std430 layout, a vec3 is aligned to
/// 16 bytes (same as vec4), which would silently mismatch a tightly-packed
/// `#[repr(C)]` Rust struct where `[f32; 3]` is only 12 bytes.
///
/// **Shader Struct Sync**: the matching `struct GpuInstance` declaration
/// in `triangle.vert`, `triangle.frag`, `ui.vert` and `caustic_splat.comp`
/// MUST be updated in lockstep. The `struct_gpuinstance_matches_all_shaders`
/// test below greps the four .comp/.vert/.frag files for the final trailing
/// u32 slot name — when you add a field here, update the expected suffix
/// in the assertion and rename the sentinel to match the new last field.
///
/// Layout: 352 bytes per instance, 16-byte aligned (22×16). Grew from
/// 192 → 224 (#492, UV + material_alpha) → 320 (#562, Skyrim+
/// BSLightingShaderProperty variant payloads — skin tint, hair tint,
/// eye envmap centers, parallax-occ / multi-layer parallax, sparkle)
/// → 352 (#221, NiMaterialProperty diffuse + ambient colors). The
/// growth pattern is strictly append-only — every existing offset is
/// preserved so shader struct mirrors only need to add fields, not
/// renumber. The `size_of::<GpuInstance>() == 352` test below asserts
/// the invariant; shader-side `GpuInstance` must match.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuInstance {
    pub model: [[f32; 4]; 4],  // 64 B, offset 0
    pub texture_index: u32,    // 4 B, offset 64
    pub bone_offset: u32,      // 4 B, offset 68
    pub normal_map_index: u32, // 4 B, offset 72
    pub roughness: f32,        // 4 B, offset 76
    pub metalness: f32,        // 4 B, offset 80
    pub emissive_mult: f32,    // 4 B, offset 84
    /// Emissive RGB + specular_strength packed as vec4 to avoid vec3 alignment.
    pub emissive_r: f32, // 4 B, offset 88
    pub emissive_g: f32,       // 4 B, offset 92
    pub emissive_b: f32,       // 4 B, offset 96
    pub specular_strength: f32, // 4 B, offset 100
    /// Specular RGB + padding packed to avoid vec3.
    pub specular_r: f32, // 4 B, offset 104
    pub specular_g: f32,       // 4 B, offset 108
    pub specular_b: f32,       // 4 B, offset 112
    /// Offset into the global vertex SSBO (in vertices, not bytes).
    pub vertex_offset: u32, // 4 B, offset 116
    /// Offset into the global index SSBO (in indices, not bytes).
    pub index_offset: u32, // 4 B, offset 120
    /// Vertex count for this mesh (for bounds checking).
    pub vertex_count: u32, // 4 B, offset 124
    /// Alpha test threshold [0,1]. 0.0 = no alpha test. #263.
    pub alpha_threshold: f32, // 4 B, offset 128
    /// Alpha test comparison function (Gamebryo TestFunction). #263.
    pub alpha_test_func: u32, // 4 B, offset 132
    /// Bindless texture index for dark/lightmap (0 = none). #264.
    pub dark_map_index: u32, // 4 B, offset 136
    /// Pre-computed average albedo for GI bounce approximation.
    /// Avoids 11 divergent memory ops per GI ray hit by replacing
    /// full UV lookup + texture sample with a single SSBO read.
    pub avg_albedo_r: f32, // 4 B, offset 140
    pub avg_albedo_g: f32,     // 4 B, offset 144
    pub avg_albedo_b: f32,     // 4 B, offset 148
    /// Per-instance flags.
    ///   bit 0 — has non-uniform scale (needs inverse-transpose normal transform). See #273.
    ///   bit 1 — `alpha_blend` enabled (NiAlphaProperty blend bit). Used by the
    ///           fragment shader for its `isGlass`/`isWindow` classification.
    ///   bit 2 — caustic source: mesh is a plausible refractive surface
    ///           (alpha-blend, non-metal). The caustic compute pass scatters
    ///           caustic splats from every pixel whose instance has this bit. #321.
    pub flags: u32, // 4 B, offset 152
    /// `BSLightingShaderProperty.shader_type` enum value (0–19), used by
    /// the fragment shader's per-variant dispatch (SkinTint, HairTint,
    /// EyeEnvmap, SparkleSnow, MultiLayerParallax, …). 0 = Default lit
    /// — the safe fall-through for non-Skyrim+ meshes that have no
    /// BSLightingShaderProperty backing. Repurposed from the previous
    /// `_pad1` field. Plumbing only here — the actual variant branches
    /// in `triangle.frag` land per-variant in follow-up PRs. See #344.
    pub material_kind: u32, // 4 B, offset 156
    /// Bindless texture index for the glow / self-illumination map
    /// (NiTexturingProperty slot 4 on Oblivion/FO3/FNV; BSShaderTextureSet
    /// slot 2 on Skyrim+). 0 = no glow map; fragment shader falls back
    /// to the inline `emissive_color` × `emissive_mult` constant.
    /// See #399 (OBL-D4-H3).
    pub glow_map_index: u32, // 4 B, offset 160
    /// Bindless texture index for the detail overlay (NiTexturingProperty
    /// slot 2). Sampled at 2× UV scale and modulated into the base
    /// albedo (`base.rgb *= detail.rgb * 2`). 0 = no detail map.
    pub detail_map_index: u32, // 4 B, offset 164
    /// Bindless texture index for the gloss map
    /// (NiTexturingProperty slot 3). Per Gamebryo 2.3
    /// `HandleGlossMap(... pkGlossiness)` the .r channel feeds the
    /// **glossiness / shininess** (Phong exponent) channel, which the
    /// fragment shader uses to modulate per-texel `roughness` — gloss
    /// = 1.0 → authored roughness, gloss = 0.0 → fully rough (dull).
    /// 0 = no gloss map. See #704 / O4-06.
    pub gloss_map_index: u32, // 4 B, offset 168
    /// Bindless texture index for the parallax / height map
    /// (`BSShaderTextureSet` slot 3). FO3/FNV `shader_type = 3`
    /// (Parallax_Shader_Index_15) and `shader_type = 7`
    /// (Parallax_Occlusion) — plus Skyrim+ ParallaxOcc /
    /// MultiLayerParallax — drive POM ray-marching off this slot.
    /// 0 = no parallax map; the fragment shader skips the POM branch
    /// and falls back to flat normal-mapped sampling. See #453.
    pub parallax_map_index: u32, // 4 B, offset 172
    /// POM height-map scale multiplier (from
    /// `BSShaderPPLightingProperty.parallax_scale` on FO3/FNV or
    /// Skyrim `ShaderTypeData::ParallaxOcc.scale`). Default `0.04`
    /// matches Bethesda's typical brick-wall depth. See #453.
    pub parallax_height_scale: f32, // 4 B, offset 176
    /// POM ray-march sample budget (typically 4–16). Default `4.0`
    /// matches BSShaderPPLightingProperty's default. See #453.
    pub parallax_max_passes: f32, // 4 B, offset 180
    /// Bindless texture index for the environment reflection map
    /// (`BSShaderTextureSet` slot 4). Currently treated as a 2D
    /// sphere-map sample; full cubemap support (separate
    /// `samplerCube` descriptor binding) is deferred. 0 = no env
    /// map; combined with `env_map_scale` on `material_kind == 1`
    /// (BSLightingShaderProperty EnvironmentMap). See #453.
    pub env_map_index: u32, // 4 B, offset 184
    /// Bindless texture index for the env-reflection mask
    /// (`BSShaderTextureSet` slot 5). Per-texel attenuation of
    /// the env reflection. 0 = no mask → reflection is unmasked.
    /// See #453.
    pub env_mask_index: u32, // 4 B, offset 188
    /// UV transform translation X (from `MaterialInfo.uv_offset[0]`).
    /// Applied to texture coordinates as `uv = uv * scale + offset`.
    /// FO4 BGSM authors both offset and scale; FO3/FNV/Skyrim usually
    /// default to identity (0, 0) / (1, 1). See #492 (FO4-BGSM-3).
    pub uv_offset_u: f32, // 4 B, offset 192
    pub uv_offset_v: f32,      // 4 B, offset 196
    /// UV transform scale X / Y.
    pub uv_scale_u: f32, // 4 B, offset 200
    pub uv_scale_v: f32,       // 4 B, offset 204
    /// Material alpha multiplier — the BGSM `material_alpha` field
    /// (equivalent to `MaterialInfo.alpha` on the NIF side). Fragment
    /// shader multiplies the sampled texture alpha by this for the
    /// final blend-pass alpha. Default `1.0` = no attenuation.
    pub material_alpha: f32, // 4 B, offset 208
    /// 12 bytes of std430 tail padding for the BGSM UV/alpha block so
    /// that block's vec4 is full (BGSM: `[uvOffsetU, uvOffsetV, uvScaleU,
    /// uvScaleV]` + `[materialAlpha, _uv_pad0, _uv_pad1, _uv_pad2]`).
    pub _uv_pad0: f32, // 4 B, offset 212
    pub _uv_pad1: f32,         // 4 B, offset 216
    pub _uv_pad2: f32,         // 4 B, offset 220
    // ── Skyrim+ BSLightingShaderProperty variant payloads (#562) ───
    //
    // All six vec4 slots below carry per-variant data from
    // `MaterialInfo::ShaderTypeFields`; the fragment shader's
    // `material_kind` ladder branches on `GpuInstance.material_kind`
    // (offset 156) to decide which fields to read. Default-lit meshes
    // (`material_kind == 0`) ignore every field.
    /// SkinTint (material_kind == 5): RGB skin tint + alpha. Shader
    /// multiplies sampled albedo by this RGB. Alpha carries the
    /// authored tint strength (1.0 = full tint multiply).
    pub skin_tint_r: f32, // 4 B, offset 224
    pub skin_tint_g: f32, // 4 B, offset 228
    pub skin_tint_b: f32, // 4 B, offset 232
    pub skin_tint_a: f32, // 4 B, offset 236
    /// HairTint (material_kind == 6): RGB hair tint. Same shader
    /// contract as skin — multiplies albedo. The w slot carries
    /// `multi_layer_envmap_strength` for MultiLayerParallax so the
    /// vec4 doesn't waste alignment on a lone f32; the two variants
    /// never overlap on a single mesh.
    pub hair_tint_r: f32, // 4 B, offset 240
    pub hair_tint_g: f32, // 4 B, offset 244
    pub hair_tint_b: f32, // 4 B, offset 248
    /// MultiLayerParallax (material_kind == 11) envmap strength
    /// scalar, packed into the hair_tint vec4 to save a dedicated
    /// slot. Skin- and hair-tinted meshes always have
    /// `multi_layer_envmap_strength = 0.0` (they don't set this
    /// variant), and MultiLayer meshes always have `hair_tint_* = 0`.
    pub multi_layer_envmap_strength: f32, // 4 B, offset 252
    /// EyeEnvmap (material_kind == 16) left-iris reflection center
    /// (xyz, object-space) + `eye_cubemap_scale` in w. Skyrim author
    /// ships both left/right centers so each eye's reflection tracks
    /// independently on a moving head mesh.
    pub eye_left_center_x: f32, // 4 B, offset 256
    pub eye_left_center_y: f32, // 4 B, offset 260
    pub eye_left_center_z: f32, // 4 B, offset 264
    pub eye_cubemap_scale: f32, // 4 B, offset 268
    /// EyeEnvmap right-iris reflection center (xyz) + reserved w.
    pub eye_right_center_x: f32, // 4 B, offset 272
    pub eye_right_center_y: f32, // 4 B, offset 276
    pub eye_right_center_z: f32, // 4 B, offset 280
    pub _eye_pad: f32,    // 4 B, offset 284
    /// MultiLayerParallax (material_kind == 11) inner-layer scalars —
    /// `inner_thickness`, `refraction_scale`, `inner_layer_scale_u`,
    /// `inner_layer_scale_v`. These drive a second UV sample of the
    /// base texture offset along the view direction, blended with the
    /// outer layer by Fresnel × `multi_layer_envmap_strength`. See
    /// `ShaderTypeData::MultiLayerParallax` for the NIF-side payload.
    pub multi_layer_inner_thickness: f32, // 4 B, offset 288
    pub multi_layer_refraction_scale: f32, // 4 B, offset 292
    pub multi_layer_inner_scale_u: f32, // 4 B, offset 296
    pub multi_layer_inner_scale_v: f32, // 4 B, offset 300
    /// SparkleSnow (material_kind == 14) sparkle-params vec4 from
    /// `ShaderTypeData::SparkleSnow`. Bethesda's content packs RGB
    /// sparkle color + alpha intensity; fragment shader overlays a
    /// per-pixel hash-driven glint modulated by these. Default
    /// (0, 0, 0, 0) produces no sparkle.
    pub sparkle_r: f32, // 4 B, offset 304
    pub sparkle_g: f32,   // 4 B, offset 308
    pub sparkle_b: f32,   // 4 B, offset 312
    pub sparkle_intensity: f32, // 4 B, offset 316
    // ── #221: NiMaterialProperty diffuse + ambient colors ───────────
    //
    // `NiMaterialProperty.diffuse` and `.ambient` were captured by
    // the importer pre-#221 but stopped at MaterialInfo — never
    // reached the GPU. The fragment shader now multiplies the sampled
    // albedo by `diffuseRGB` (per-material tint, no-op at white) and
    // the cell ambient term by `ambientRGB` (per-material ambient
    // response). Two padded vec4 slots so we keep std430 alignment
    // without renumbering the existing `_uv_pad*` / `_eye_pad`
    // patches above; defaults are `[1.0, 1.0, 1.0, 0.0]` so meshes
    // without an `NiMaterialProperty` (every BSShader-only Skyrim+
    // / FO4 mesh) are unaffected.
    pub diffuse_r: f32,    // 4 B, offset 320
    pub diffuse_g: f32,    // 4 B, offset 324
    pub diffuse_b: f32,    // 4 B, offset 328
    pub _diffuse_pad: f32, // 4 B, offset 332
    pub ambient_r: f32,    // 4 B, offset 336
    pub ambient_g: f32,    // 4 B, offset 340
    pub ambient_b: f32,    // 4 B, offset 344
    pub _ambient_pad: f32, // 4 B, offset 348 → total 352
                           // Struct is 352 bytes (22×16), 16-byte aligned for std430.
}

impl Default for GpuInstance {
    fn default() -> Self {
        Self {
            model: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            texture_index: 0,
            bone_offset: 0,
            normal_map_index: 0,
            roughness: 0.5,
            metalness: 0.0,
            emissive_mult: 0.0,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            specular_strength: 1.0,
            specular_r: 1.0,
            specular_g: 1.0,
            specular_b: 1.0,
            vertex_offset: 0,
            index_offset: 0,
            vertex_count: 0,
            alpha_threshold: 0.0,
            alpha_test_func: 0,
            dark_map_index: 0,
            avg_albedo_r: 0.5,
            avg_albedo_g: 0.5,
            avg_albedo_b: 0.5,
            flags: 0,
            material_kind: 0,
            glow_map_index: 0,
            detail_map_index: 0,
            gloss_map_index: 0,
            parallax_map_index: 0,
            parallax_height_scale: 0.04,
            parallax_max_passes: 4.0,
            env_map_index: 0,
            env_mask_index: 0,
            uv_offset_u: 0.0,
            uv_offset_v: 0.0,
            uv_scale_u: 1.0,
            uv_scale_v: 1.0,
            material_alpha: 1.0,
            _uv_pad0: 0.0,
            _uv_pad1: 0.0,
            _uv_pad2: 0.0,
            // Skyrim+ variant payloads (#562) — zeroed by default.
            // Default-lit meshes (material_kind == 0) never read them;
            // variant branches gate on `material_kind` so these are
            // live only when a real BSLightingShaderProperty shader
            // type is set on the instance.
            skin_tint_r: 0.0,
            skin_tint_g: 0.0,
            skin_tint_b: 0.0,
            skin_tint_a: 0.0,
            hair_tint_r: 0.0,
            hair_tint_g: 0.0,
            hair_tint_b: 0.0,
            multi_layer_envmap_strength: 0.0,
            eye_left_center_x: 0.0,
            eye_left_center_y: 0.0,
            eye_left_center_z: 0.0,
            eye_cubemap_scale: 0.0,
            eye_right_center_x: 0.0,
            eye_right_center_y: 0.0,
            eye_right_center_z: 0.0,
            _eye_pad: 0.0,
            multi_layer_inner_thickness: 0.0,
            multi_layer_refraction_scale: 0.0,
            multi_layer_inner_scale_u: 0.0,
            multi_layer_inner_scale_v: 0.0,
            sparkle_r: 0.0,
            sparkle_g: 0.0,
            sparkle_b: 0.0,
            sparkle_intensity: 0.0,
            // #221 — `[1.0; 3]` defaults so meshes without
            // `NiMaterialProperty` (every BSShader-only mesh) keep
            // identity tint + ambient response. Padding fields stay
            // at 0.0 — they're never read by the shader.
            diffuse_r: 1.0,
            diffuse_g: 1.0,
            diffuse_b: 1.0,
            _diffuse_pad: 0.0,
            ambient_r: 1.0,
            ambient_g: 1.0,
            ambient_b: 1.0,
            _ambient_pad: 0.0,
        }
    }
}

/// GPU-side light struct (48 bytes, std430 layout).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GpuLight {
    /// xyz = world position, w = radius (Bethesda units).
    pub position_radius: [f32; 4],
    /// rgb = color (0–1), w = type (0=point, 1=spot, 2=directional).
    pub color_type: [f32; 4],
    /// xyz = direction (spot/directional), w = spot outer angle cosine.
    pub direction_angle: [f32; 4],
}

/// GPU-side camera data (256 bytes, std140-compatible).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuCamera {
    /// Combined view-projection matrix (column-major).
    pub view_proj: [[f32; 4]; 4],
    /// Previous frame's view-projection matrix (column-major). Used by the
    /// vertex shader to compute screen-space motion vectors: projecting a
    /// vertex's current world position through both matrices gives the
    /// screen motion that downstream temporal filters (SVGF, TAA) need.
    /// On the very first frame, this equals `view_proj` so motion is zero.
    pub prev_view_proj: [[f32; 4]; 4],
    /// Precomputed `inverse(viewProj)` — used by cluster culling and SSAO
    /// to reconstruct world positions from depth without a per-invocation
    /// matrix inverse on the GPU.
    pub inv_view_proj: [[f32; 4]; 4],
    /// xyz = world position, w = frame counter (for temporal jitter seed).
    pub position: [f32; 4],
    /// x = RT enabled (1.0), y/z/w = ambient light color (RGB).
    pub flags: [f32; 4],
    /// x = screen width, y = screen height, z = fog near, w = fog far.
    pub screen: [f32; 4],
    /// xyz = fog color (RGB 0-1), w = fog enabled (1.0 = yes).
    pub fog: [f32; 4],
    /// xy = sub-pixel projection jitter in NDC space (Halton 2,3 sequence),
    /// applied to `gl_Position.xy` AFTER motion-vector clip positions are
    /// captured so reprojection remains jitter-free. zw = reserved.
    pub jitter: [f32; 4],
}

impl Default for GpuCamera {
    fn default() -> Self {
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        Self {
            view_proj: identity,
            prev_view_proj: identity,
            inv_view_proj: identity,
            position: [0.0; 4],
            flags: [0.0; 4],
            screen: [1280.0, 720.0, 0.0, 0.0],
            fog: [0.0, 0.0, 0.0, 0.0],
            jitter: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

/// SSBO header: lightCount + padding to 16-byte alignment (std430).
#[repr(C)]
#[derive(Clone, Copy)]
struct LightHeader {
    count: u32,
    _pad: [u32; 3],
}

/// Per-frame scene buffers and their descriptor sets.
pub struct SceneBuffers {
    /// One SSBO per frame-in-flight (header + light array).
    light_buffers: Vec<GpuBuffer>,
    /// One UBO per frame-in-flight (camera data).
    camera_buffers: Vec<GpuBuffer>,
    /// One SSBO per frame-in-flight (bone palette for skinning).
    /// Slot 0 is always the identity matrix; the vertex shader uses it
    /// as a fallback for rigid vertices that land here by accident.
    bone_buffers: Vec<GpuBuffer>,
    /// One SSBO per frame-in-flight (per-instance data for instanced drawing).
    /// Each entry contains model matrix + texture index + bone offset.
    instance_buffers: Vec<GpuBuffer>,
    /// One `INDIRECT_BUFFER`-usage buffer per frame-in-flight for
    /// `vkCmdDrawIndexedIndirect`. Holds
    /// `VkDrawIndexedIndirectCommand` entries uploaded CPU-side each
    /// frame. The draw loop collapses consecutive batches sharing
    /// `(pipeline_key, is_decal)` into one indirect draw call reading
    /// a contiguous range of this buffer. See #309.
    indirect_buffers: Vec<GpuBuffer>,
    /// Single DEVICE_LOCAL SSBO holding `MAX_TERRAIN_TILES`
    /// `GpuTerrainTile` entries. Rewritten only at cell transitions
    /// via [`SceneBuffers::upload_terrain_tiles`] — a staging copy
    /// into GPU memory. Pre-#497 this was double-buffered HOST_VISIBLE,
    /// which wasted 32 KB of scarce BAR heap for read-only data. All
    /// frame-in-flight descriptor sets point at the same buffer since
    /// there are no per-frame contents. Fragment shader reads
    /// `terrainTiles[tile_idx]` when `INSTANCE_FLAG_TERRAIN_SPLAT` is
    /// set on the instance. See #470 / #497.
    terrain_tile_buffer: GpuBuffer,
    /// One HOST_VISIBLE u32 SSBO per frame-in-flight for the RT mipmap
    /// glass ray budget counter. The CPU zeroes it before each render pass;
    /// the fragment shader atomically increments it per IOR ray pair fired
    /// and skips Phase-3 glass once the budget is exhausted. Binding 11.
    ray_budget_buffers: Vec<GpuBuffer>,
    /// Size of the terrain tile buffer in bytes — stashed so upload
    /// paths don't have to recompute it from `MAX_TERRAIN_TILES`.
    terrain_tile_buf_size: vk::DeviceSize,
    /// Descriptor pool for scene descriptor sets.
    descriptor_pool: vk::DescriptorPool,
    /// Layout for set 1: binding 0 = SSBO (lights), binding 1 = UBO (camera),
    /// binding 2 = TLAS (RT only), binding 3 = SSBO (bone palette),
    /// binding 4 = SSBO (instance data), …, binding 12 = SSBO (previous-frame
    /// bone palette — SH-3 / #641, vertex-stage only, points at the
    /// frame-in-flight ring's other slot so motion vectors on skinned
    /// vertices reflect actual joint motion rather than zero).
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    /// One descriptor set per frame-in-flight.
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    /// Tracks whether the TLAS binding has been written for each frame.
    pub tlas_written: Vec<bool>,
}

impl SceneBuffers {
    /// Create scene buffers and descriptor infrastructure.
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        rt_enabled: bool,
    ) -> Result<Self> {
        // Calculate buffer sizes.
        let light_buf_size = (std::mem::size_of::<LightHeader>()
            + std::mem::size_of::<GpuLight>() * MAX_LIGHTS)
            as vk::DeviceSize;
        let camera_buf_size = std::mem::size_of::<GpuCamera>() as vk::DeviceSize;
        // Bone palette: 4 × vec4 (mat4) per slot, std430 layout.
        let bone_buf_size =
            (std::mem::size_of::<[[f32; 4]; 4]>() * MAX_TOTAL_BONES) as vk::DeviceSize;
        // Instance SSBO: per-instance model matrix + texture index + bone offset.
        let instance_buf_size =
            (std::mem::size_of::<GpuInstance>() * MAX_INSTANCES) as vk::DeviceSize;
        // Indirect buffer: one VkDrawIndexedIndirectCommand (20 B) per batch. #309.
        let indirect_buf_size = (std::mem::size_of::<vk::DrawIndexedIndirectCommand>()
            * MAX_INDIRECT_DRAWS) as vk::DeviceSize;
        // Terrain tile SSBO: 32 B per slot × MAX_TERRAIN_TILES. #470.
        let terrain_tile_buf_size =
            (std::mem::size_of::<GpuTerrainTile>() * MAX_TERRAIN_TILES) as vk::DeviceSize;

        // Create per-frame buffers.
        let mut light_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut camera_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut bone_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut instance_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut indirect_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut ray_budget_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            light_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                light_buf_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            camera_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                camera_buf_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )?);
            bone_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bone_buf_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            instance_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                instance_buf_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            indirect_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                indirect_buf_size,
                vk::BufferUsageFlags::INDIRECT_BUFFER,
            )?);
            // Ray budget counter: 4 bytes, atomically incremented by the
            // fragment shader, zeroed by the CPU before each render pass.
            ray_budget_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                std::mem::size_of::<u32>() as vk::DeviceSize,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
        }
        // Terrain tile SSBO: single DEVICE_LOCAL buffer, uploaded via
        // staging at cell load. TRANSFER_DST needed for the staging
        // copy. Shared across all frame-in-flight descriptor sets
        // since the contents are static until the next cell
        // transition. See #497.
        let terrain_tile_buffer = GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            terrain_tile_buf_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
        )?;

        // Descriptor set layout: set 1.
        let mut bindings = vec![
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            // Camera UBO is read by both vertex (viewProj) and fragment (cameraPos, sceneFlags).
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];
        if rt_enabled {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            );
        }
        // Binding 3: bone palette SSBO (vertex shader — skinning).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX),
        );
        // Binding 4: instance data SSBO (vertex + fragment — instanced drawing + PBR materials).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 5: cluster grid SSBO (fragment shader — clustered lighting).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 6: cluster light indices SSBO (fragment shader — clustered lighting).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 7: SSAO texture (fragment shader — ambient occlusion).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(7)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 8: global vertex SSBO (fragment shader — RT reflection UV lookup).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(8)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 9: global index SSBO (fragment shader — RT reflection UV lookup).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(9)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 10: terrain tile SSBO (fragment shader — LAND splat layer
        // texture indices, one entry per terrain entity). #470.
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(10)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 11: RT mipmap ray budget counter (fragment shader — u32 atomic).
        // The CPU zeroes this before each render pass; Phase-3 glass fragments
        // atomicAdd to claim a budget slot and skip IOR refraction once exhausted.
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(11)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 12: previous-frame bone palette SSBO (vertex shader —
        // skinned-vertex motion vectors, SH-3 / #641). Bound to the OTHER
        // slot in the `bone_buffers` frame-in-flight ring, so reading it
        // yields the palette uploaded last frame. Pre-#641 the vertex
        // shader composed `fragPrevClipPos = prevViewProj * worldPos`
        // using the CURRENT-frame skinned worldPos — every actor pixel
        // had a motion vector encoding only camera + rigid motion, and
        // SVGF / TAA reprojected the wrong source pixel on intra-mesh
        // disocclusions (forearm crossing torso). Same indices/weights as
        // binding 3, so the per-mesh bone offset stamped on the
        // `GpuInstance` still resolves correctly as long as the offset
        // assignment is stable across the two frames.
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(12)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX),
        );
        // Mark bindings 5+6 (cluster data) as PARTIALLY_BOUND so they are
        // valid even when unwritten (cluster cull pipeline may fail to create).
        // The fragment shader guards access with a lightCount > 0 check.
        let binding_flags: Vec<vk::DescriptorBindingFlags> = bindings
            .iter()
            .enumerate()
            .map(|(_, b)| {
                let binding_idx = b.binding;
                if binding_idx >= 5 {
                    vk::DescriptorBindingFlags::PARTIALLY_BOUND
                } else {
                    vk::DescriptorBindingFlags::empty()
                }
            })
            .collect();
        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);
        // Validate against triangle.vert/frag SPIR-V before creating the layout (#427).
        // When rt_enabled=false the TLAS binding (2) is declared in the shader
        // but intentionally omitted from the layout — the fragment gates every
        // rayQuery behind a uniform flag.
        // Binding 2 (TLAS) is declared in the shader but omitted when rt_enabled=false.
        let optional_bindings: &[u32] = if rt_enabled { &[] } else { &[2] };
        super::reflect::validate_set_layout(
            1,
            &bindings,
            &[
                super::reflect::ReflectedShader {
                    name: "triangle.vert",
                    spirv: super::pipeline::TRIANGLE_VERT_SPV,
                },
                super::reflect::ReflectedShader {
                    name: "triangle.frag",
                    spirv: super::pipeline::TRIANGLE_FRAG_SPV,
                },
            ],
            "scene (set=1)",
            optional_bindings,
        )
        .expect("scene descriptor layout drifted against triangle.vert/frag (see #427)");
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&bindings)
            .push_next(&mut binding_flags_info);
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .context("Failed to create scene descriptor set layout")?
        };

        // Descriptor pool.
        // Two STORAGE_BUFFER descriptors per frame (lights + bones).
        let mut pool_sizes = vec![
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                // 10 SSBOs per frame: lights(0), bones(3), instances(4), cluster
                // grid(5), light indices(6), vertices(8), indices(9), terrain
                // tiles(10), ray budget counter(11), bones_prev(12 — SH-3 / #641).
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 10) as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                // 1 per frame: SSAO texture (binding 7).
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
        ];
        if rt_enabled {
            pool_sizes.push(vk::DescriptorPoolSize {
                ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            });
        }
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to create scene descriptor pool")?
        };

        // Allocate descriptor sets.
        let layouts = vec![descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to allocate scene descriptor sets")?
        };

        // Write descriptor sets to point at the buffers.
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let light_buf_info = [vk::DescriptorBufferInfo {
                buffer: light_buffers[i].buffer,
                offset: 0,
                range: light_buf_size,
            }];
            let camera_buf_info = [vk::DescriptorBufferInfo {
                buffer: camera_buffers[i].buffer,
                offset: 0,
                range: camera_buf_size,
            }];
            let bone_buf_info = [vk::DescriptorBufferInfo {
                buffer: bone_buffers[i].buffer,
                offset: 0,
                range: bone_buf_size,
            }];
            // Previous-frame bone palette: the OTHER slot in the ring.
            // Frame N writes its palette to `bone_buffers[N % MAX]` and
            // reads `bone_buffers[(N + MAX - 1) % MAX]` as last frame's.
            // SH-3 / #641. With MAX_FRAMES_IN_FLIGHT=2 the prev index is
            // `(i + 1) % 2`. The mapping is static — written once here.
            let bone_prev_idx = (i + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
            let bone_prev_buf_info = [vk::DescriptorBufferInfo {
                buffer: bone_buffers[bone_prev_idx].buffer,
                offset: 0,
                range: bone_buf_size,
            }];
            let instance_buf_info = [vk::DescriptorBufferInfo {
                buffer: instance_buffers[i].buffer,
                offset: 0,
                range: instance_buf_size,
            }];
            let terrain_tile_buf_info = [vk::DescriptorBufferInfo {
                buffer: terrain_tile_buffer.buffer,
                offset: 0,
                range: terrain_tile_buf_size,
            }];
            let ray_budget_buf_info = [vk::DescriptorBufferInfo {
                buffer: ray_budget_buffers[i].buffer,
                offset: 0,
                range: std::mem::size_of::<u32>() as vk::DeviceSize,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&light_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&camera_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&bone_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(4)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&instance_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(10)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&terrain_tile_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(11)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&ray_budget_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(12)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&bone_prev_buf_info),
            ];
            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }
        }

        // Seed slot 0 of each bone palette with the identity matrix so
        // rigid vertices that fall through to the palette (shouldn't
        // happen, but serves as a defensive fallback) produce correct
        // positions rather than collapsing to origin.
        let identity: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        for buf in &mut bone_buffers {
            buf.write_mapped(device, std::slice::from_ref(&identity))?;
        }

        log::info!(
            "Scene buffers created: {} frames, {} max lights ({} bytes/frame)",
            MAX_FRAMES_IN_FLIGHT,
            MAX_LIGHTS,
            light_buf_size,
        );

        Ok(Self {
            light_buffers,
            camera_buffers,
            bone_buffers,
            instance_buffers,
            indirect_buffers,
            terrain_tile_buffer,
            terrain_tile_buf_size,
            ray_budget_buffers,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            tlas_written: vec![false; MAX_FRAMES_IN_FLIGHT],
        })
    }

    /// Upload light data for the current frame-in-flight.
    pub fn upload_lights(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        lights: &[GpuLight],
    ) -> Result<()> {
        let count = lights.len().min(MAX_LIGHTS);
        let header = LightHeader {
            count: count as u32,
            _pad: [0; 3],
        };

        let header_size = std::mem::size_of::<LightHeader>();
        let light_size = std::mem::size_of::<GpuLight>();

        // Write directly to mapped GPU memory — no intermediate Vec allocation.
        let buf = &mut self.light_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;

        // SAFETY: LightHeader and GpuLight are #[repr(C)] with plain f32/u32 fields.
        // mapped buffer is sized for MAX_LIGHTS. No overlap between header and light regions.
        unsafe {
            std::ptr::copy_nonoverlapping(
                &header as *const LightHeader as *const u8,
                mapped.as_mut_ptr(),
                header_size,
            );
            if count > 0 {
                std::ptr::copy_nonoverlapping(
                    lights.as_ptr() as *const u8,
                    mapped.as_mut_ptr().add(header_size),
                    light_size * count,
                );
            }
        }

        buf.flush_if_needed(device)
    }

    /// Upload camera data for the current frame-in-flight.
    pub fn upload_camera(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        camera: &GpuCamera,
    ) -> Result<()> {
        self.camera_buffers[frame_index].write_mapped(device, std::slice::from_ref(camera))
    }

    /// Upload the bone palette for the current frame-in-flight.
    ///
    /// `palette` is packed contiguous mat4 entries in column-major glam
    /// layout. Slot 0 is always the identity matrix — callers that
    /// assemble multiple meshes into one palette should keep slot 0 as
    /// identity and start writing mesh bones at slot 1.
    ///
    /// Writes at most `MAX_TOTAL_BONES` entries; extra are silently
    /// clamped and logged once per session by the caller.
    pub fn upload_bones(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        palette: &[[[f32; 4]; 4]],
    ) -> Result<()> {
        let count = palette.len().min(MAX_TOTAL_BONES);
        if count == 0 {
            return Ok(());
        }

        let buf = &mut self.bone_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        // SAFETY: [[f32; 4]; 4] is #[repr(C)]-compatible with std430 mat4.
        // bone_buffers are sized for MAX_TOTAL_BONES slots; count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                palette.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                std::mem::size_of::<[[f32; 4]; 4]>() * count,
            );
        }
        buf.flush_if_needed(device)
    }

    /// Upload per-instance data for the current frame-in-flight.
    ///
    /// Called once per frame before the render pass. The vertex shader reads
    /// `instances[gl_InstanceIndex]` for model matrix, texture index, and bone offset.
    pub fn upload_instances(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        instances: &[GpuInstance],
    ) -> Result<()> {
        let count = instances.len().min(MAX_INSTANCES);
        if instances.len() > MAX_INSTANCES {
            log::warn!(
                "Instance SSBO overflow: {} instances submitted, capped at {} — excess draws silently dropped. #279 P2-12",
                instances.len(),
                MAX_INSTANCES,
            );
        }
        if count == 0 {
            return Ok(());
        }
        let buf = &mut self.instance_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let byte_size = std::mem::size_of::<GpuInstance>() * count;
        // SAFETY: GpuInstance is #[repr(C)] with plain f32/u32 fields.
        // instance_buffers are sized for MAX_INSTANCES; count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                instances.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size,
            );
        }
        buf.flush_if_needed(device)
    }

    /// Get a mutable reference to the mapped instance buffer for direct writes.
    /// Used by the UI overlay to append a single instance after the bulk upload.
    pub fn instance_buffer_mapped_mut(&mut self, frame_index: usize) -> Result<&mut [u8]> {
        self.instance_buffers[frame_index].mapped_slice_mut()
    }

    /// Upload `VkDrawIndexedIndirectCommand` entries for the current
    /// frame-in-flight. The draw loop issues one
    /// `vkCmdDrawIndexedIndirect` per pipeline group, reading a
    /// contiguous range of this buffer. See #309.
    ///
    /// Clamps at [`MAX_INDIRECT_DRAWS`] and logs a warn on overflow —
    /// real scenes with the #272 instanced batching rarely emit more
    /// than a few hundred batches per frame, so the clamp is a
    /// defense-in-depth against unbounded-growth bugs.
    pub fn upload_indirect_draws(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        draws: &[vk::DrawIndexedIndirectCommand],
    ) -> Result<()> {
        let count = draws.len().min(MAX_INDIRECT_DRAWS);
        if draws.len() > MAX_INDIRECT_DRAWS {
            log::warn!(
                "Indirect draw overflow: {} commands submitted, capped at {} — excess draws silently dropped",
                draws.len(),
                MAX_INDIRECT_DRAWS,
            );
        }
        if count == 0 {
            return Ok(());
        }
        let buf = &mut self.indirect_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let byte_size = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() * count;
        // SAFETY: VkDrawIndexedIndirectCommand is a Vulkan-defined C struct
        // with the exact layout expected by the device. `indirect_buffers`
        // are sized for MAX_INDIRECT_DRAWS; count is clamped above.
        unsafe {
            std::ptr::copy_nonoverlapping(
                draws.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size,
            );
        }
        buf.flush_if_needed(device)
    }

    /// Return the `VkBuffer` handle for the current frame's indirect
    /// buffer. The draw loop passes this to `cmd_draw_indexed_indirect`.
    pub fn indirect_buffer(&self, frame_index: usize) -> vk::Buffer {
        self.indirect_buffers[frame_index].buffer
    }

    /// Upload terrain tile data into the single DEVICE_LOCAL SSBO via
    /// a staging buffer + one-time `vkCmdCopyBuffer`. Called from the
    /// cell loader path after `spawn_terrain_mesh` packs per-tile layer
    /// texture indices. The data is static until the next cell
    /// transition, so exactly one upload per dirty transition is
    /// enough — no per-frame double-buffering. See #470 / #497.
    pub fn upload_terrain_tiles(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        tiles: &[GpuTerrainTile],
    ) -> Result<()> {
        let count = tiles.len().min(MAX_TERRAIN_TILES);
        if tiles.len() > MAX_TERRAIN_TILES {
            log::warn!(
                "Terrain tile SSBO overflow: {} tiles submitted, capped at {} — excess slots silently dropped. #470",
                tiles.len(),
                MAX_TERRAIN_TILES,
            );
        }
        if count == 0 {
            return Ok(());
        }

        let byte_size = (std::mem::size_of::<GpuTerrainTile>() * count) as vk::DeviceSize;

        // Create a transient staging buffer. Terrain tile uploads run
        // at cell-transition frequency (a few times a minute at most),
        // so skip the StagingPool reuse overhead — a one-shot 32 KB
        // CpuToGpu allocation is cheap and the buffer vanishes cleanly
        // via the guard below on any exit path.
        let staging_info = vk::BufferCreateInfo::default()
            .size(byte_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_buffer = unsafe {
            device
                .create_buffer(&staging_info, None)
                .context("Failed to create terrain tile staging buffer")?
        };
        let reqs = unsafe { device.get_buffer_memory_requirements(staging_buffer) };
        let mut staging_alloc = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "terrain_tile_staging",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate terrain tile staging memory")?;
        super::buffer::debug_assert_cpu_to_gpu_mapped(
            &staging_alloc,
            "terrain_tile_staging",
        );
        unsafe {
            device
                .bind_buffer_memory(
                    staging_buffer,
                    staging_alloc.memory(),
                    staging_alloc.offset(),
                )
                .context("Failed to bind terrain tile staging buffer")?;
        }

        // SAFETY: GpuTerrainTile is #[repr(C)] with u32-only fields
        // matching std430. Staging was sized to `byte_size` above.
        let mapped = staging_alloc
            .mapped_slice_mut()
            .context("Terrain tile staging not mapped")?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                tiles.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size as usize,
            );
        }

        let copy = vk::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: byte_size,
        };
        let dst = self.terrain_tile_buffer.buffer;
        let result = super::texture::with_one_time_commands(device, queue, command_pool, |cmd| {
            unsafe {
                device.cmd_copy_buffer(cmd, staging_buffer, dst, &[copy]);
            }
            Ok(())
        });

        // Tear down staging regardless of copy outcome.
        unsafe {
            device.destroy_buffer(staging_buffer, None);
        }
        allocator
            .lock()
            .expect("allocator lock poisoned")
            .free(staging_alloc)
            .ok();

        // Suppress "field never read" on the cached size — kept for
        // future layout changes / debugging introspection.
        let _ = self.terrain_tile_buf_size;

        result
    }

    /// Get the light buffers (for compute pipeline descriptor writes).
    pub fn light_buffers(&self) -> &[GpuBuffer] {
        &self.light_buffers
    }

    /// Get the camera buffers (for compute pipeline descriptor writes).
    pub fn camera_buffers(&self) -> &[GpuBuffer] {
        &self.camera_buffers
    }

    /// Get the instance buffers (for the caustic pipeline's descriptor writes).
    pub fn instance_buffers(&self) -> &[GpuBuffer] {
        &self.instance_buffers
    }

    /// Get the per-frame bone palette buffers (M29 — skin compute
    /// reads them as the bone-matrix source per-dispatch).
    pub fn bone_buffers(&self) -> &[GpuBuffer] {
        &self.bone_buffers
    }

    /// Bone palette buffer size in bytes (`MAX_TOTAL_BONES × mat4`).
    pub fn bone_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<[[f32; 4]; 4]>() * MAX_TOTAL_BONES) as vk::DeviceSize
    }

    /// Light buffer size in bytes.
    pub fn light_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<LightHeader>() + std::mem::size_of::<GpuLight>() * MAX_LIGHTS)
            as vk::DeviceSize
    }

    /// Camera buffer size in bytes.
    pub fn camera_buffer_size(&self) -> vk::DeviceSize {
        std::mem::size_of::<GpuCamera>() as vk::DeviceSize
    }

    /// Instance buffer size in bytes.
    pub fn instance_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<GpuInstance>() * MAX_INSTANCES) as vk::DeviceSize
    }

    /// Get the descriptor set for the current frame-in-flight.
    pub fn descriptor_set(&self, frame_index: usize) -> vk::DescriptorSet {
        self.descriptor_sets[frame_index]
    }

    /// Write the SSAO texture into the scene descriptor set for a given frame.
    pub fn write_ao_texture(
        &self,
        device: &ash::Device,
        frame_index: usize,
        ao_image_view: vk::ImageView,
        ao_sampler: vk::Sampler,
    ) {
        let image_info = [vk::DescriptorImageInfo::default()
            .sampler(ao_sampler)
            .image_view(ao_image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_sets[frame_index])
            .dst_binding(7)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info);
        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }
    }

    /// Write global geometry SSBO references for RT reflection UV lookups.
    pub fn write_geometry_buffers(
        &self,
        device: &ash::Device,
        frame_index: usize,
        vertex_buffer: vk::Buffer,
        vertex_size: vk::DeviceSize,
        index_buffer: vk::Buffer,
        index_size: vk::DeviceSize,
    ) {
        let vert_info = [vk::DescriptorBufferInfo {
            buffer: vertex_buffer,
            offset: 0,
            range: vertex_size,
        }];
        let idx_info = [vk::DescriptorBufferInfo {
            buffer: index_buffer,
            offset: 0,
            range: index_size,
        }];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[frame_index])
                .dst_binding(8)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&vert_info),
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[frame_index])
                .dst_binding(9)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&idx_info),
        ];
        unsafe { device.update_descriptor_sets(&writes, &[]) }
    }

    /// Write cluster buffer references into the scene descriptor set for a given frame.
    /// Called once during init after the cluster cull pipeline is created.
    pub fn write_cluster_buffers(
        &self,
        device: &ash::Device,
        frame_index: usize,
        grid_buffer: vk::Buffer,
        grid_size: vk::DeviceSize,
        index_buffer: vk::Buffer,
        index_size: vk::DeviceSize,
    ) {
        let grid_info = [vk::DescriptorBufferInfo {
            buffer: grid_buffer,
            offset: 0,
            range: grid_size,
        }];
        let index_info = [vk::DescriptorBufferInfo {
            buffer: index_buffer,
            offset: 0,
            range: index_size,
        }];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[frame_index])
                .dst_binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&grid_info),
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[frame_index])
                .dst_binding(6)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&index_info),
        ];
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    /// Update the TLAS acceleration structure in the descriptor set for a given frame.
    pub fn write_tlas(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        tlas: vk::AccelerationStructureKHR,
    ) {
        self.tlas_written[frame_index] = true;
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);

        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_sets[frame_index])
            .dst_binding(2)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .descriptor_count(1)
            .push_next(&mut accel_write);

        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }
    }

    /// Zero the ray budget counter for the given frame before the render pass.
    ///
    /// Called from `draw_frame` after uploading instances and before
    /// `cmd_begin_render_pass`. The fragment shader atomically increments this
    /// counter for each Phase-3 IOR glass ray pair it fires; once the count
    /// exceeds `GLASS_RAY_BUDGET` (declared in `triangle.frag`) all further
    /// glass fragments degrade to the tier-1 cheaper path for that frame.
    pub fn reset_ray_budget(&mut self, device: &ash::Device, frame: usize) -> Result<()> {
        let zero: u32 = 0;
        self.ray_budget_buffers[frame].write_mapped(device, std::slice::from_ref(&zero))
    }

    /// Destroy all resources.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buf in &mut self.light_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.camera_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.bone_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.instance_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.indirect_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.ray_budget_buffers {
            buf.destroy(device, allocator);
        }
        self.terrain_tile_buffer.destroy(device, allocator);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}

#[cfg(test)]
mod gpu_instance_layout_tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    /// Regression guard for the Shader Struct Sync invariant (#318 / #417).
    /// The `GpuInstance` struct is duplicated across **four** GLSL
    /// sources — `triangle.vert`, `triangle.frag`, `ui.vert`, and (since
    /// the caustic pass #321) `caustic_splat.comp` — and must stay
    /// byte-for-byte identical with the Rust definition. Any drift here
    /// silently corrupts per-instance data on the GPU. Verified offsets
    /// come from the explicit `// offset N` comments inside those shaders.
    /// See the `feedback_shader_struct_sync` memory note for the lockstep
    /// update protocol (grep for `struct GpuInstance` in the shaders tree
    /// before touching this struct).
    #[test]
    fn gpu_instance_is_352_bytes_std430_compatible() {
        // 176 → 192 in #453 (parallax/env slots).
        // 192 → 224 in #492 (uv_offset + uv_scale + material_alpha).
        // 224 → 320 in #562 (Skyrim+ BSLightingShaderProperty variant
        // payloads — SkinTint / HairTint / MultiLayerParallax /
        // EyeEnvmap / SparkleSnow — packed as 6 vec4s).
        // 320 → 352 in #221 (NiMaterialProperty diffuse + ambient
        // colors — 2 padded vec4 slots).
        // 22 × 16 = 352.
        assert_eq!(
            size_of::<GpuInstance>(),
            352,
            "GpuInstance must stay 352 B to match std430 shader layout"
        );
    }

    #[test]
    fn gpu_instance_field_offsets_match_shader_contract() {
        assert_eq!(offset_of!(GpuInstance, model), 0);
        assert_eq!(offset_of!(GpuInstance, texture_index), 64);
        assert_eq!(offset_of!(GpuInstance, bone_offset), 68);
        assert_eq!(offset_of!(GpuInstance, normal_map_index), 72);
        assert_eq!(offset_of!(GpuInstance, roughness), 76);
        assert_eq!(offset_of!(GpuInstance, metalness), 80);
        assert_eq!(offset_of!(GpuInstance, emissive_mult), 84);
        assert_eq!(offset_of!(GpuInstance, emissive_r), 88);
        assert_eq!(offset_of!(GpuInstance, emissive_g), 92);
        assert_eq!(offset_of!(GpuInstance, emissive_b), 96);
        assert_eq!(offset_of!(GpuInstance, specular_strength), 100);
        assert_eq!(offset_of!(GpuInstance, specular_r), 104);
        assert_eq!(offset_of!(GpuInstance, specular_g), 108);
        assert_eq!(offset_of!(GpuInstance, specular_b), 112);
        assert_eq!(offset_of!(GpuInstance, vertex_offset), 116);
        assert_eq!(offset_of!(GpuInstance, index_offset), 120);
        assert_eq!(offset_of!(GpuInstance, vertex_count), 124);
        assert_eq!(offset_of!(GpuInstance, alpha_threshold), 128);
        assert_eq!(offset_of!(GpuInstance, alpha_test_func), 132);
        assert_eq!(offset_of!(GpuInstance, dark_map_index), 136);
        assert_eq!(offset_of!(GpuInstance, avg_albedo_r), 140);
        assert_eq!(offset_of!(GpuInstance, avg_albedo_g), 144);
        assert_eq!(offset_of!(GpuInstance, avg_albedo_b), 148);
        assert_eq!(offset_of!(GpuInstance, flags), 152);
        // material_kind reuses the previous _pad1 slot — kept at the
        // same offset so every shader-side `pad1` reference renamed to
        // `materialKind` continues to alias the same 4 bytes. See #344.
        assert_eq!(offset_of!(GpuInstance, material_kind), 156);
        // #399 — three NiTexturingProperty texture-slot indices appended
        // after material_kind, each 4 bytes.
        assert_eq!(offset_of!(GpuInstance, glow_map_index), 160);
        assert_eq!(offset_of!(GpuInstance, detail_map_index), 164);
        assert_eq!(offset_of!(GpuInstance, gloss_map_index), 168);
        // #453 — BSShaderTextureSet slots 3/4/5 + POM scalars. The
        // `_pad_extra_textures` u32 at offset 172 was reclaimed as
        // `parallax_map_index`; the new fields extend to offset 192.
        assert_eq!(offset_of!(GpuInstance, parallax_map_index), 172);
        assert_eq!(offset_of!(GpuInstance, parallax_height_scale), 176);
        assert_eq!(offset_of!(GpuInstance, parallax_max_passes), 180);
        assert_eq!(offset_of!(GpuInstance, env_map_index), 184);
        assert_eq!(offset_of!(GpuInstance, env_mask_index), 188);
        // #492 — FO4 BGSM UV transform + material alpha. Packed as
        // one vec4 (offset/scale) + one half-used vec4 (alpha + 3
        // padding f32) to land on a 16-byte boundary.
        assert_eq!(offset_of!(GpuInstance, uv_offset_u), 192);
        assert_eq!(offset_of!(GpuInstance, uv_offset_v), 196);
        assert_eq!(offset_of!(GpuInstance, uv_scale_u), 200);
        assert_eq!(offset_of!(GpuInstance, uv_scale_v), 204);
        assert_eq!(offset_of!(GpuInstance, material_alpha), 208);
        assert_eq!(offset_of!(GpuInstance, _uv_pad0), 212);
        assert_eq!(offset_of!(GpuInstance, _uv_pad1), 216);
        assert_eq!(offset_of!(GpuInstance, _uv_pad2), 220);
        // #562 — Skyrim+ BSLightingShaderProperty variant payloads.
        // Each `vec4`-sized slot is 16 B, packed to land on std430
        // alignment boundaries.
        assert_eq!(offset_of!(GpuInstance, skin_tint_r), 224);
        assert_eq!(offset_of!(GpuInstance, skin_tint_g), 228);
        assert_eq!(offset_of!(GpuInstance, skin_tint_b), 232);
        assert_eq!(offset_of!(GpuInstance, skin_tint_a), 236);
        assert_eq!(offset_of!(GpuInstance, hair_tint_r), 240);
        assert_eq!(offset_of!(GpuInstance, hair_tint_g), 244);
        assert_eq!(offset_of!(GpuInstance, hair_tint_b), 248);
        assert_eq!(offset_of!(GpuInstance, multi_layer_envmap_strength), 252);
        assert_eq!(offset_of!(GpuInstance, eye_left_center_x), 256);
        assert_eq!(offset_of!(GpuInstance, eye_left_center_y), 260);
        assert_eq!(offset_of!(GpuInstance, eye_left_center_z), 264);
        assert_eq!(offset_of!(GpuInstance, eye_cubemap_scale), 268);
        assert_eq!(offset_of!(GpuInstance, eye_right_center_x), 272);
        assert_eq!(offset_of!(GpuInstance, eye_right_center_y), 276);
        assert_eq!(offset_of!(GpuInstance, eye_right_center_z), 280);
        assert_eq!(offset_of!(GpuInstance, _eye_pad), 284);
        assert_eq!(offset_of!(GpuInstance, multi_layer_inner_thickness), 288);
        assert_eq!(offset_of!(GpuInstance, multi_layer_refraction_scale), 292);
        assert_eq!(offset_of!(GpuInstance, multi_layer_inner_scale_u), 296);
        assert_eq!(offset_of!(GpuInstance, multi_layer_inner_scale_v), 300);
        assert_eq!(offset_of!(GpuInstance, sparkle_r), 304);
        assert_eq!(offset_of!(GpuInstance, sparkle_g), 308);
        assert_eq!(offset_of!(GpuInstance, sparkle_b), 312);
        assert_eq!(offset_of!(GpuInstance, sparkle_intensity), 316);
        // #221 — NiMaterialProperty diffuse + ambient. Two padded vec4
        // slots appended at the end of the struct.
        assert_eq!(offset_of!(GpuInstance, diffuse_r), 320);
        assert_eq!(offset_of!(GpuInstance, diffuse_g), 324);
        assert_eq!(offset_of!(GpuInstance, diffuse_b), 328);
        assert_eq!(offset_of!(GpuInstance, _diffuse_pad), 332);
        assert_eq!(offset_of!(GpuInstance, ambient_r), 336);
        assert_eq!(offset_of!(GpuInstance, ambient_g), 340);
        assert_eq!(offset_of!(GpuInstance, ambient_b), 344);
        assert_eq!(offset_of!(GpuInstance, _ambient_pad), 348);
    }

    /// Regression: #309 — `VkDrawIndexedIndirectCommand` is a Vulkan-
    /// specified C struct that `cmd_draw_indexed_indirect` reads
    /// directly from the device-side buffer. Its layout is part of
    /// the Vulkan contract (20 bytes, five u32 fields in a fixed
    /// order). Guard the size so a future `ash` upgrade that
    /// accidentally renames / reorders fields breaks the test
    /// instead of silently producing garbage draw params.
    #[test]
    fn draw_indexed_indirect_command_is_20_bytes() {
        assert_eq!(
            size_of::<vk::DrawIndexedIndirectCommand>(),
            20,
            "VkDrawIndexedIndirectCommand must be 20 bytes (5 × u32) per the Vulkan spec"
        );
    }

    /// Regression: #309 — `upload_indirect_draws` clamps at
    /// `MAX_INDIRECT_DRAWS` so a future bug that produces an
    /// unbounded batch list can't overflow the indirect buffer.
    /// 8192 × 20 B = 160 KB per frame; the allocation matches.
    #[test]
    fn indirect_buffer_capacity_matches_max_draw_constant() {
        let bytes_per_command = size_of::<vk::DrawIndexedIndirectCommand>();
        assert_eq!(bytes_per_command, 20);
        assert_eq!(
            bytes_per_command * MAX_INDIRECT_DRAWS,
            20 * 8192,
            "MAX_INDIRECT_DRAWS × sizeof(VkDrawIndexedIndirectCommand) \
             must match the per-frame indirect buffer allocation"
        );
    }

    /// Regression: #417 — every shader that declares its own copy of
    /// `struct GpuInstance` must name the final u32 slot
    /// `materialKind`, not `_pad1` or any other legacy placeholder.
    /// The Rust side guards offsets via
    /// `gpu_instance_field_offsets_match_shader_contract`; this test
    /// guards name-level drift across the four shader copies so a
    /// future refactor that actually reads the field (currently unused
    /// on the caustic path) doesn't silently alias it to padding.
    ///
    /// Walks the shaders tree at compile time via `include_str!` —
    /// works in `cargo test` even on machines that don't have
    /// glslangValidator installed, and catches the missed-rename
    /// failure mode from #417 (caustic_splat.comp still said
    /// `uint _pad1;` after the triangle.* / ui.vert rename).
    #[test]
    fn every_shader_struct_gpu_instance_names_material_kind_slot() {
        const SOURCES: &[(&str, &str)] = &[
            ("triangle.vert", include_str!("../../shaders/triangle.vert")),
            ("triangle.frag", include_str!("../../shaders/triangle.frag")),
            ("ui.vert", include_str!("../../shaders/ui.vert")),
            (
                "caustic_splat.comp",
                include_str!("../../shaders/caustic_splat.comp"),
            ),
        ];
        for (name, src) in SOURCES {
            assert!(
                src.contains("struct GpuInstance"),
                "{name} no longer declares `struct GpuInstance` — update \
                 the sync list at feedback_shader_struct_sync.md"
            );
            // The struct must declare the trailing slot as
            // `materialKind`, not `_pad1` / `_pad` / `pad1`.
            assert!(
                src.contains("materialKind"),
                "{name}: GpuInstance final slot must be named \
                 `materialKind` (see #417). Found no mention — did \
                 the shader revert to `_pad1`?"
            );
            assert!(
                !src.contains("uint _pad1"),
                "{name}: GpuInstance slot is still named `_pad1` — \
                 rename to `materialKind` to match the other 3 \
                 shaders (Shader Struct Sync invariant #318 / #417)."
            );
            // #453 — the BSShaderTextureSet slots 3/4/5 + POM scalars
            // must land in every copy so the Rust struct and the GLSL
            // structs stay byte-identical.
            // #492 — FO4 BGSM UV transform + material alpha extend the
            // struct to 224 B. Every copy must declare the new fields
            // even when the shader doesn't yet consume them (the
            // fragment-shader wiring lands in #494).
            for needle in [
                "parallaxMapIndex",
                "parallaxHeightScale",
                "parallaxMaxPasses",
                "envMapIndex",
                "envMaskIndex",
                "uvOffsetU",
                "uvOffsetV",
                "uvScaleU",
                "uvScaleV",
                "materialAlpha",
                // #562 — Skyrim+ BSLightingShaderProperty variant
                // payloads. All four shader copies must declare these
                // even when the shader doesn't consume them, so the
                // 320 B std430 stride stays byte-identical.
                "skinTintR",
                "hairTintR",
                "multiLayerEnvmapStrength",
                "eyeLeftCenterX",
                "eyeCubemapScale",
                "eyeRightCenterX",
                "multiLayerInnerThickness",
                "multiLayerRefractionScale",
                "multiLayerInnerScaleU",
                "sparkleR",
                "sparkleIntensity",
                // #221 — NiMaterialProperty diffuse + ambient. All
                // four shader copies must declare these even if only
                // triangle.frag consumes them today, so the 352 B
                // std430 stride stays byte-identical across the four.
                "diffuseR",
                "ambientR",
            ] {
                assert!(
                    src.contains(needle),
                    "{name}: GpuInstance must declare `{needle}` (#453). \
                     Every copy updates in lockstep — see the \
                     feedback_shader_struct_sync memory note."
                );
            }
            // The old `_pad_extra_textures` slot was reclaimed for
            // `parallaxMapIndex`. A stale `_pad_extra_textures` mention
            // means the shader still has the pre-#453 layout.
            assert!(
                !src.contains("_pad_extra_textures"),
                "{name}: GpuInstance still declares the reclaimed \
                 `_pad_extra_textures` slot — rename to \
                 `parallaxMapIndex` so the byte layout matches the \
                 320-byte Rust struct."
            );
        }
    }

    /// SH-3 / #641 regression. The vertex shader must compose
    /// `fragPrevClipPos` through the previous-frame bone palette so
    /// motion vectors on skinned vertices encode actual joint motion.
    /// Pre-#641 it composed through the current-frame palette, leaving
    /// every actor body / hand / face pixel with a wrong motion vector
    /// that SVGF + TAA reprojected as a ghost trail.
    ///
    /// Static source check (no `glslangValidator` dependency): the
    /// shader must declare a `bones_prev` SSBO at `set 1, binding 12`
    /// and feed `prevWorldPos` (composed through `bones_prev`) into
    /// `fragPrevClipPos = prevViewProj * …`.
    #[test]
    fn triangle_vert_uses_bones_prev_for_motion_vectors() {
        let src = include_str!("../../shaders/triangle.vert");
        assert!(
            src.contains("binding = 12) readonly buffer BonesPrevBuffer"),
            "triangle.vert must declare a previous-frame bone palette \
             SSBO at `set 1, binding = 12` (SH-3 / #641). Without it \
             skinned vertices produce wrong motion vectors and SVGF / \
             TAA ghost actor limbs in motion."
        );
        assert!(
            src.contains("mat4 bones_prev[]"),
            "triangle.vert: `BonesPrevBuffer` must expose a `mat4 \
             bones_prev[]` array — same layout as `bones[]` so the \
             current and previous palettes can share `inBoneIndices`."
        );
        assert!(
            src.contains("fragPrevClipPos = prevViewProj * prevWorldPos"),
            "triangle.vert: `fragPrevClipPos` must project the \
             previous-frame skinned `prevWorldPos`, not the current \
             frame's `worldPos`. SH-3 / #641 — composing through \
             `bones[]` for both frames is the bug this test guards."
        );
        assert!(
            src.contains("xformPrev"),
            "triangle.vert: a separate `xformPrev` matrix must be \
             composed from `bones_prev` so `prevWorldPos` reflects \
             last frame's joint poses (SH-3 / #641)."
        );
    }
}
