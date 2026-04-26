//! NIF material and texture property extraction.

use crate::blocks::properties::{
    NiAlphaProperty, NiFlagProperty, NiMaterialProperty, NiStencilProperty, NiTexturingProperty,
    NiVertexColorProperty, TexDesc,
};
use crate::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderTextureSet, ShaderTypeData,
};
use crate::blocks::texture::NiSourceTexture;
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::scene::NifScene;
use crate::types::BlockRef;

use super::mesh::GeomData;

mod shader_data;
mod walker;

pub use shader_data::ShaderTypeFields;
pub(crate) use shader_data::{apply_shader_type_data, capture_effect_shader_data};
// Re-exported only for the per-mod test sibling
// `shader_type_data_tests.rs` — production callers go through
// `apply_shader_type_data` instead. Marked `allow(unused_imports)` so
// non-test builds don't warn about the unused alias.
#[allow(unused_imports)]
pub(crate) use shader_data::capture_shader_type_fields;
pub(crate) use walker::{
    extract_material_info, extract_material_info_from_refs, extract_vertex_colors,
};

// Import-side aliases for the named flag constants in
// `crate::shader_flags`. Kept `pub(super)` so downstream files in the
// `import` module (`mesh.rs`, `walk.rs`) can reach them without paging
// through the shared module. The shared constants are documented with
// per-game semantics so a future refactor can swap callsites onto
// `GameVariant`-aware lookups (#461 / #437).
// SLSF1 bits 26 / 27 — `Decal` and `Dynamic_Decal`. The bit positions
// align byte-exact across FO3/FNV `BSShaderFlags` (`fo3nv_f1`),
// Skyrim `SkyrimShaderPropertyFlags1` (`skyrim_slsf1`), and FO4
// `Fallout4ShaderPropertyFlags1` (`fo4_slsf1`) — every era touched in
// production. We source from `fo4_slsf1` (the most-recent registry)
// so the FO4 module is not dead-code-only and a future bit drift is
// caught at compile time below. The cross-era equivalence is proven
// by the runtime tests at `shader_flags::tests::*`. See #592.
pub(super) const DECAL_SINGLE_PASS: u32 = crate::shader_flags::fo4_slsf1::DECAL;
pub(super) const DYNAMIC_DECAL: u32 = crate::shader_flags::fo4_slsf1::DYNAMIC_DECAL;

// Compile-time proof: any future shader-flags reshuffle that breaks
// cross-era equivalence on the bits this module consumes will fail
// the build, surfacing the drift before it reaches a renderer
// regression. Pre-#592 the production path read FO4 properties
// through Skyrim/FNV-labelled aliases by accident — the bit positions
// happened to coincide. These const-eval assertions promote the
// coincidence to a load-bearing invariant.
const _: () =
    assert!(crate::shader_flags::fo4_slsf1::DECAL == crate::shader_flags::skyrim_slsf1::DECAL);
const _: () = assert!(crate::shader_flags::fo4_slsf1::DECAL == crate::shader_flags::fo3nv_f1::DECAL);
const _: () = assert!(
    crate::shader_flags::fo4_slsf1::DYNAMIC_DECAL == crate::shader_flags::skyrim_slsf1::DYNAMIC_DECAL
);
const _: () = assert!(
    crate::shader_flags::fo4_slsf1::DYNAMIC_DECAL == crate::shader_flags::fo3nv_f1::DYNAMIC_DECAL
);
// FO3/FNV-specific decal bit on flags2 — collides with Skyrim's
// `Cloud_LOD` on the same bit. Only tested on FO3/FNV `BSShader*Property`
// paths; Skyrim+ `BSLightingShaderProperty` goes through SLSF1 bits
// 26/27 only (see #176 closure).
const ALPHA_DECAL_F2: u32 = crate::shader_flags::fo3nv_f2::ALPHA_DECAL;

/// Shared decal detection across `BSShaderPPLightingProperty` +
/// `BSShaderNoLightingProperty` (FO3/FNV).
///
/// Tests SLSF1 bits 26/27 (`Decal` / `Dynamic_Decal` — these align
/// numerically across every game-era) AND the FO3/FNV-only
/// `Alpha_Decal` bit on flags2 (bit 21). The flags2 bit is crucial on
/// blood-splat NoLighting meshes that don't set the SLSF1 decal bits
/// (pre-#454 the NoLighting branch had no flags2 check and those
/// rendered as opaque coplanar quads).
///
/// **Must not be called on Skyrim+ or FO4 properties.** Bit 21 of the
/// second flag word on those games is `Cloud_LOD` (Skyrim) or
/// `Anisotropic_Lighting` (FO4), NOT a decal bit — using this helper
/// on `BSLightingShaderProperty` would spuriously classify those
/// meshes as decals. Modern properties route through
/// [`is_decal_from_modern_shader_flags`] instead. See #414 / FO4-D3-M1.
#[inline]
pub(super) fn is_decal_from_legacy_shader_flags(flags1: u32, flags2: u32) -> bool {
    flags1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 || flags2 & ALPHA_DECAL_F2 != 0
}

/// Decal detection for Skyrim+ / FO4 `BSLightingShaderProperty` +
/// `BSEffectShaderProperty`.
///
/// Tests SLSF1 / F4SF1 bits 26/27 (`Decal` / `Dynamic_Decal`) only.
/// These bits have identical numeric value and semantic on Skyrim +
/// FO4 — the split from [`is_decal_from_legacy_shader_flags`] exists
/// to keep flags2 bit 21 (`Cloud_LOD` on Skyrim, `Anisotropic_Lighting`
/// on FO4) out of the decal test. See #414 / FO4-D3-M1.
#[inline]
pub(super) fn is_decal_from_modern_shader_flags(flags1: u32, _flags2: u32) -> bool {
    flags1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0
}

// NOTE: there is no `SF_DOUBLE_SIDED` on the FO3/FNV
// `BSShaderPPLightingProperty` / `BSShaderNoLightingProperty` flag
// pair. Pre-#441 we tested `flags_1 & 0x1000` on both blocks as if
// that bit meant Double_Sided (the Skyrim/FO4 `SkyrimShaderPropertyFlags2`
// convention), but on the FO3/FNV `BSShaderFlags` enum that bit is
// `Unknown_3` — a debug/crash flag with no backface meaning. flags2
// bit 4 on FO3/FNV is `Refraction_Tint`, also not Double_Sided.
// Verified against nif.xml lines 6148–6218 (`Fallout3ShaderPropertyFlags1/2`)
// vs. lines 6407+ / 6479+ for Skyrim and FO4 where the bit semantics
// actually land.
//
// FO3/FNV meshes that want back-face-off rely on `NiStencilProperty`
// — handled by the fallback at `extract_material_info` below. The
// Skyrim+ `BSLightingShaderProperty` / `BSEffectShaderProperty` path
// still uses `flags2 & 0x10` because that is the documented
// Double_Sided bit on those games.
//
// Double_Sided bit on Skyrim+ / FO4 `*ShaderPropertyFlags2`. Only
// tested on blocks whose game actually carries this semantic (see
// note above). Sourced from `fo4_slsf2` so the FO4 module is not
// dead-code-only — bit position aligns with Skyrim and the
// compile-time assertion below pins the equivalence. See #592.
const SF2_DOUBLE_SIDED: u32 = crate::shader_flags::fo4_slsf2::DOUBLE_SIDED;
const _: () = assert!(
    crate::shader_flags::fo4_slsf2::DOUBLE_SIDED
        == crate::shader_flags::skyrim_slsf2::DOUBLE_SIDED
);

/// How a `NiVertexColorProperty` wants per-vertex colors to participate
/// in shading, mirroring Gamebryo's `NiVertexColorProperty::SourceMode`.
///
/// `NiTexturingProperty` / `NiMaterialProperty` meshes can opt out of
/// vertex-color contribution entirely (`Ignore`) or route it through a
/// different shader channel (`Emissive`). Pre-#214 the importer always
/// used vertex colors as diffuse regardless of the stored mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum VertexColorMode {
    /// `SRC_IGNORE` — the mesh has vertex colors in the data block but
    /// the material explicitly disables them. Treat as if absent.
    Ignore = 0,
    /// `SRC_EMISSIVE` — vertex colors drive per-vertex self-illumination
    /// rather than diffuse. Gamebryo uses this for flickering torches,
    /// signs, and glowing effects baked into the geometry.
    Emissive = 1,
    /// `SRC_AMB_DIFF` — default / pre-10.0 behavior: vertex colors act
    /// as per-vertex diffuse + ambient.
    AmbientDiffuse = 2,
}

impl VertexColorMode {
    /// Decode the Gamebryo source-mode u32. Unknown values fall back to
    /// `AmbientDiffuse` — the value Gamebryo uses when the field is
    /// missing — so legacy content stays visually unchanged.
    pub(super) fn from_source_mode(raw: u32) -> Self {
        match raw {
            0 => Self::Ignore,
            1 => Self::Emissive,
            _ => Self::AmbientDiffuse,
        }
    }

    /// Decode the full `NiVertexColorProperty` (vertex_mode, lighting_mode)
    /// pair into our 1-D `VertexColorMode` axis. See #694 / O4-02.
    ///
    /// Gamebryo's lighting equation gates which terms contribute:
    ///
    /// * `LIGHTING_E_A_D` (1, default): Emissive + Ambient + Diffuse
    ///   terms all participate. Vertex color routes per `vertex_mode`.
    /// * `LIGHTING_E` (0): only the Emissive term contributes — Ambient
    ///   and Diffuse are dropped from the lighting integral.
    ///
    /// When `LIGHTING_E` combines with `SOURCE_AMB_DIFF`, the vertex
    /// colors feed terms the engine has just dropped — they become
    /// invisible. Collapse that to `Ignore` so the renderer's PBR
    /// pipeline skips the (`texColor.rgb * fragColor`) multiplication
    /// that the fragment shader unconditionally applies. Pre-fix this
    /// double-counted material colors on the rare LIGHTING_E meshes
    /// (Oblivion FX / a few statics).
    ///
    /// Other (vertex_mode, lighting_mode) combinations either route
    /// through Emissive (which we already special-case) or are the
    /// LIGHTING_E_A_D default which keeps the source-mode unchanged.
    pub(super) fn from_property(vertex_mode: u32, lighting_mode: u32) -> Self {
        let src = Self::from_source_mode(vertex_mode);
        // `lighting_mode == 0` is `LIGHTING_E`; any other value (including
        // missing-field default `1`) is `LIGHTING_E_A_D`.
        if lighting_mode == 0 && src == Self::AmbientDiffuse {
            Self::Ignore
        } else {
            src
        }
    }
}

/// Material properties extracted from a NiTriShape's property list in a single pass.
#[derive(Debug)]
pub(super) struct MaterialInfo {
    pub texture_path: Option<String>,
    /// BGSM/BGEM material file reference (FO4+). Present when the
    /// BSLightingShaderProperty has a non-empty name.
    pub material_path: Option<String>,
    pub normal_map: Option<String>,
    /// Glow / self-illumination texture (NiTexturingProperty slot 4).
    /// Filled on Oblivion/FO3/FNV meshes where a dedicated emissive
    /// map supplements or replaces `NiMaterialProperty.emissive`. See #214.
    pub glow_map: Option<String>,
    /// Detail overlay texture (NiTexturingProperty slot 2). Blends with
    /// the base texture at higher frequency; used for terrain detail
    /// variation and clothing micro-texture.
    pub detail_map: Option<String>,
    /// Specular-mask / gloss texture (NiTexturingProperty slot 3).
    /// Per-texel specular strength; enables armor highlights masked
    /// by leather/fabric regions.
    pub gloss_map: Option<String>,
    /// Dark / multiplicative lightmap texture (NiTexturingProperty slot 1).
    /// Baked shadow/grime modulation on Oblivion interior architecture.
    /// Applied as `albedo.rgb *= dark_sample.rgb`. See #264.
    pub dark_map: Option<String>,
    /// Decal overlay textures (NiTexturingProperty decal slots 0..=3).
    /// Oblivion uses these for blood splatters, wall paintings / map
    /// decals, and faction symbols — content that persists in the world
    /// but lives on top of the base material. Before #400 the parser
    /// dropped the slots silently; now populated entries ride through to
    /// the renderer for alpha-blend overlay. Empty slots are omitted so
    /// downstream consumers only see reachable textures.
    pub decal_maps: Vec<String>,
    /// Parallax / height texture (`BSShaderTextureSet` slot 3). FO3/FNV
    /// architecture relies on this for brick-wall / concrete
    /// parallax-occlusion mapping on `shader_type = 3` (Parallax_Shader_Index_15)
    /// and `shader_type = 7` (Parallax_Occlusion) PPLighting materials.
    /// Pre-#452 the importer stopped reading at slot 2, so every Pitt /
    /// Point Lookout / Hoover Dam parallax wall landed flat. See #452.
    pub parallax_map: Option<String>,
    /// Environment cubemap (`BSShaderTextureSet` slot 4). Drives the
    /// glass bottle / power-armor / smooth-metal reflection branch.
    /// `env_map_scale` is already captured but had no texture route
    /// until #452.
    pub env_map: Option<String>,
    /// Environment-reflection mask (`BSShaderTextureSet` slot 5). Per-
    /// texel attenuation of the `env_map` reflection — used on armor
    /// edges and rim highlights so only the polished surface reflects.
    /// See #452.
    pub env_mask: Option<String>,
    /// How vertex colors should participate in shading. See #214 /
    /// `VertexColorMode`. Defaults to `AmbientDiffuse` — the value
    /// Gamebryo uses when the NIF has no `NiVertexColorProperty`.
    pub vertex_color_mode: VertexColorMode,
    pub alpha_blend: bool,
    /// Source blend factor from NiAlphaProperty flags bits 1–4.
    /// Maps to Gamebryo's AlphaFunction enum:
    ///   0=ONE, 1=ZERO, 2=SRC_COLOR, 3=INV_SRC_COLOR, 4=DEST_COLOR,
    ///   5=INV_DEST_COLOR, 6=SRC_ALPHA, 7=INV_SRC_ALPHA, 8=DEST_ALPHA,
    ///   9=INV_DEST_ALPHA, 10=SRC_ALPHA_SATURATE.
    /// Default: 6 (SRC_ALPHA).
    pub src_blend_mode: u8,
    /// Destination blend factor from NiAlphaProperty flags bits 5–8.
    /// Same enum as src_blend_mode. Default: 7 (INV_SRC_ALPHA).
    pub dst_blend_mode: u8,
    /// Alpha-tested (cutout) rendering — vertices whose sampled texture
    /// alpha falls below `alpha_threshold` should be `discard`-ed in the
    /// fragment shader. Set when NiAlphaProperty.flags has bit 9 (0x200).
    /// Mutually exclusive with `alpha_blend` in the importer: when a
    /// material sets both bits (common on Gamebryo foliage and hair),
    /// alpha-test wins because the discard + depth-write path sorts
    /// cleanly, while alpha-blend produces z-sort artifacts.
    pub alpha_test: bool,
    /// Cutoff threshold for `alpha_test`, in the [0.0, 1.0] range —
    /// `NiAlphaProperty.threshold` (u8) divided by 255.
    pub alpha_threshold: f32,
    /// Alpha test comparison function from `NiAlphaProperty.flags`
    /// bits 10–12. Maps to Gamebryo's `TestFunction` enum:
    ///   0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL,
    ///   4=GREATER, 5=NOTEQUAL, 6=GREATEREQUAL, 7=NEVER.
    /// Default: 6 (GREATEREQUAL) — keep fragments where alpha >= threshold.
    pub alpha_test_func: u8,
    pub two_sided: bool,
    pub is_decal: bool,
    pub emissive_color: [f32; 3],
    pub emissive_mult: f32,
    pub specular_color: [f32; 3],
    pub specular_strength: f32,
    /// Diffuse color from `NiMaterialProperty` (or `[1.0; 3]` default).
    ///
    /// Used as the per-vertex color fallback when
    /// `vertex_color_mode == Ignore` or the mesh has no vertex_colors
    /// array. Pre-#438 `extract_vertex_colors` walked the property list
    /// a second time to re-read this value; caching here removes one
    /// full scan per NiTriShape.
    pub diffuse_color: [f32; 3],
    /// True when the mesh has no `NiSpecularProperty` or the property's
    /// enable flag (bit 0) is set. Many Oblivion/FNV matte surfaces
    /// (stone walls, plaster, unfinished wood) explicitly disable
    /// specular via a `NiSpecularProperty { flags: 0 }` block; honoring
    /// that flag prevents bright specular hotspots that look like
    /// lighting glitches in the new PBR pipeline.
    pub specular_enabled: bool,
    pub glossiness: f32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    /// Ambient color (RGB) from `NiMaterialProperty.ambient`. Modulates
    /// the cell's ambient lighting term per material — most authored
    /// values are `[1.0; 3]` so the field acts as a no-op tint by
    /// default. Audit `AUDIT_LEGACY_COMPAT_2026-04-10.md` D4-09 / #221.
    pub ambient_color: [f32; 3],
    pub alpha: f32,
    pub env_map_scale: f32,
    pub has_material_data: bool,
    /// Set by any property that contributes a UV transform — the
    /// Skyrim+ shader paths copy `uv_offset` / `uv_scale` directly off
    /// `BSLightingShaderProperty` / `BSEffectShaderProperty`, while the
    /// pre-Skyrim path picks the base-slot `TexTransform` on
    /// `NiTexturingProperty`. Pre-#435 the NiTexturingProperty branch
    /// gated on `has_material_data`, so a `NiMaterialProperty` listed
    /// before `NiTexturingProperty` (the common Oblivion / FO3 / FNV
    /// property order) silently dropped the texture-slot UV transform —
    /// even though `NiMaterialProperty` carries no UV transform of its
    /// own, the two flags are orthogonal. See audit
    /// `AUDIT_NIF_2026-04-18.md` finding N06.
    pub has_uv_transform: bool,
    /// Depth test enabled (from NiZBufferProperty). Default: true.
    pub z_test: bool,
    /// Depth write enabled (from NiZBufferProperty). Default: true.
    pub z_write: bool,
    /// Depth comparison function from `NiZBufferProperty.z_function`.
    /// Maps to the Gamebryo `TestFunction` enum:
    /// 0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL, 4=GREATER, 5=NOTEQUAL,
    /// 6=GREATEREQUAL, 7=NEVER. Default 3 (LESSEQUAL) — matches the
    /// Gamebryo runtime default and the renderer's pre-#398 hardcoded
    /// `vk::CompareOp::LESS` (close enough that everything depth-tested
    /// strictly less still passes equal-depth co-planar geometry as
    /// LESSEQUAL would). Pre-#398 the value was extracted into
    /// `MaterialInfo` but never reached the GPU; sky domes / viewmodels
    /// / glow halos that author non-default depth state z-fought
    /// against world geometry.
    pub z_function: u8,

    // ── BSLightingShaderProperty.shader_type dispatch (SK-D3-01) ────
    // Each variant of `ShaderTypeData` exposes different trailing
    // fields. Capturing them at import time lets the renderer later
    // branch on `material_kind` without re-reading the NIF. Renderer-
    // side dispatch is tracked separately (SK-D3-02); until that lands
    // these values ride unused on `MaterialInfo`.
    /// Raw `BSLightingShaderProperty.shader_type` (0–19). 0 when the
    /// shape has no BSLightingShaderProperty (pre-Skyrim).
    pub material_kind: u8,
    /// SkinTint (type 5) — race/character skin color. FO76 variant
    /// stores alpha in `skin_tint_alpha`.
    pub skin_tint_color: Option<[f32; 3]>,
    /// FO76 SkinTint (type 4, BSShaderType155) — alpha channel that
    /// the Color4 variant of SkinTint carries in addition to RGB.
    pub skin_tint_alpha: Option<f32>,
    /// HairTint (type 6) — per-NPC hair color multiplier.
    pub hair_tint_color: Option<[f32; 3]>,
    /// EyeEnvmap (type 16) — cubemap reflection strength on eye shapes.
    pub eye_cubemap_scale: Option<f32>,
    /// EyeEnvmap left-eye reflection center, world-space.
    pub eye_left_reflection_center: Option<[f32; 3]>,
    /// EyeEnvmap right-eye reflection center, world-space.
    pub eye_right_reflection_center: Option<[f32; 3]>,
    /// ParallaxOcc (type 7) — height-sample passes (stepping quality).
    pub parallax_max_passes: Option<f32>,
    /// ParallaxOcc (type 7) — height-map scale.
    pub parallax_height_scale: Option<f32>,
    /// MultiLayerParallax (type 11) — inner layer thickness.
    pub multi_layer_inner_thickness: Option<f32>,
    /// MultiLayerParallax (type 11) — refraction scale.
    pub multi_layer_refraction_scale: Option<f32>,
    /// MultiLayerParallax (type 11) — inner texture scale u/v.
    pub multi_layer_inner_layer_scale: Option<[f32; 2]>,
    /// MultiLayerParallax (type 11) — envmap strength.
    pub multi_layer_envmap_strength: Option<f32>,
    /// SparkleSnow (type 14) — packed rgba parameters (rgb color +
    /// alpha strength).
    pub sparkle_parameters: Option<[f32; 4]>,

    /// Rich Skyrim+ effect-shader (`BSEffectShaderProperty`) data —
    /// soft falloff cone, greyscale palette, FO4+/FO76 companion
    /// textures, lighting influence, etc. `None` for non-effect
    /// materials. The parser already extracted every field; before
    /// #345 the importer dropped all but `texture_path`, `emissive_*`
    /// and `uv_*`. Now they ride through to the renderer (separate
    /// dispatch hookup tracked at SK-D3-02).
    pub effect_shader: Option<BsEffectShaderData>,

    /// FO3/FNV `BSShaderNoLightingProperty` soft-falloff cone. Four
    /// floats pulled from the parsed block when bsver > 26; on older
    /// Oblivion content they sit at the parser-side defaults. `None`
    /// when the material has no NoLighting backing.
    /// Pre-#451 these four fields were silently discarded by the
    /// importer even though the parser had captured them — FO3 UI
    /// overlays, VATS crosshair, scope reticles, Pip-Boy glow, heat-
    /// shimmer planes lost their angular falloff. Renderer dispatch
    /// is follow-up work (tracked separately alongside the BSEffect
    /// soft-falloff hookup under SK-D3-02).
    pub no_lighting_falloff: Option<NoLightingFalloff>,
}

/// Soft-falloff cone captured from `BSShaderNoLightingProperty` (FO3/FNV
/// HUD overlays + UI tiles + scope reticles). Sibling of the richer
/// [`BsEffectShaderData`] that covers `BSEffectShaderProperty`; the
/// NoLighting block only emits the four cone scalars plus its file
/// name (already routed to `MaterialInfo::texture_path`). See #451.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoLightingFalloff {
    /// Cos-of-angle where alpha = `start_opacity`.
    pub start_angle: f32,
    /// Cos-of-angle where alpha = `stop_opacity`.
    pub stop_angle: f32,
    /// Alpha at the start angle.
    pub start_opacity: f32,
    /// Alpha at the stop angle.
    pub stop_opacity: f32,
}

/// Fields imported from a `BSEffectShaderProperty` block. Only present
/// on materials backed by an effect shader (VFX surfaces, force fields,
/// glow-edged shields, Dwemer steam, BGEM-keyed surfaces). See #345 /
/// audit S4-01.
#[derive(Debug, Clone, PartialEq)]
pub struct BsEffectShaderData {
    /// Soft falloff cone — start angle (cos) where alpha = `start_opacity`.
    pub falloff_start_angle: f32,
    /// Soft falloff cone — stop angle (cos) where alpha = `stop_opacity`.
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,
    /// Soft-particles depth — fades the surface as it approaches the
    /// scene depth behind it. 0.0 = no soft-particle effect.
    pub soft_falloff_depth: f32,
    /// Greyscale palette / gradient lookup texture (fire / electricity
    /// gradients reference this). `None` when the effect shader
    /// supplies an empty path.
    pub greyscale_texture: Option<String>,
    /// Environment map texture (FO4+ — BSVER >= 130).
    pub env_map_texture: Option<String>,
    /// Normal texture (FO4+ — BSVER >= 130).
    pub normal_texture: Option<String>,
    /// Environment mask texture (FO4+ — BSVER >= 130).
    pub env_mask_texture: Option<String>,
    /// Environment-map scale (FO4+ — BSVER >= 130).
    pub env_map_scale: f32,
    /// FO76 refraction power (BSVER == 155). `None` on pre-FO76.
    pub refraction_power: Option<f32>,
    /// Lighting influence 0–255 — how much the scene's directional
    /// light tints the effect. Carried as raw u8 to avoid lossy
    /// normalisation; the shader path can divide by 255 when sampling.
    pub lighting_influence: u8,
    /// Environment-map minimum mip-level clamp (raw u8).
    pub env_map_min_lod: u8,
    /// Texture clamp mode: `0=Clamp_S_Clamp_T`, `1=Clamp_S_Wrap_T`,
    /// `2=Wrap_S_Clamp_T`, `3=Wrap_S_Wrap_T` (the Skyrim default).
    /// Raw u8 — renderer maps to `vk::SamplerAddressMode` per axis.
    pub texture_clamp_mode: u8,
}

impl Default for MaterialInfo {
    fn default() -> Self {
        Self {
            texture_path: None,
            material_path: None,
            normal_map: None,
            glow_map: None,
            detail_map: None,
            gloss_map: None,
            dark_map: None,
            decal_maps: Vec::new(),
            parallax_map: None,
            env_map: None,
            env_mask: None,
            vertex_color_mode: VertexColorMode::AmbientDiffuse,
            alpha_blend: false,
            src_blend_mode: 6, // SRC_ALPHA — Gamebryo default
            dst_blend_mode: 7, // INV_SRC_ALPHA — Gamebryo default
            alpha_test: false,
            alpha_threshold: 0.0,
            alpha_test_func: 6, // GREATEREQUAL — Gamebryo default
            two_sided: false,
            is_decal: false,
            emissive_color: [0.0, 0.0, 0.0],
            emissive_mult: 0.0,
            specular_color: [1.0, 1.0, 1.0],
            specular_strength: 1.0,
            diffuse_color: [1.0, 1.0, 1.0],
            specular_enabled: true,
            glossiness: 80.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            // Default to white so the per-material ambient term acts as
            // a no-op tint when the mesh has no `NiMaterialProperty`
            // (every BSShader path on Skyrim+/FO4 — those inherit the
            // cell ambient unmodulated).
            ambient_color: [1.0, 1.0, 1.0],
            alpha: 1.0,
            env_map_scale: 0.0,
            has_material_data: false,
            has_uv_transform: false,
            z_test: true,
            z_write: true,
            z_function: 3, // LESSEQUAL — Gamebryo default

            material_kind: 0,
            skin_tint_color: None,
            skin_tint_alpha: None,
            hair_tint_color: None,
            eye_cubemap_scale: None,
            eye_left_reflection_center: None,
            eye_right_reflection_center: None,
            parallax_max_passes: None,
            parallax_height_scale: None,
            multi_layer_inner_thickness: None,
            multi_layer_refraction_scale: None,
            multi_layer_inner_layer_scale: None,
            multi_layer_envmap_strength: None,
            sparkle_parameters: None,
            effect_shader: None,
            no_lighting_falloff: None,
        }
    }
}









impl MaterialInfo {
    /// Project this `MaterialInfo`'s shader-type fields into a
    /// `ShaderTypeFields` bundle for `ImportedMesh`. See #430.
    pub(super) fn shader_type_fields(&self) -> ShaderTypeFields {
        ShaderTypeFields {
            skin_tint_color: self.skin_tint_color,
            skin_tint_alpha: self.skin_tint_alpha,
            hair_tint_color: self.hair_tint_color,
            eye_cubemap_scale: self.eye_cubemap_scale,
            eye_left_reflection_center: self.eye_left_reflection_center,
            eye_right_reflection_center: self.eye_right_reflection_center,
            parallax_max_passes: self.parallax_max_passes,
            parallax_height_scale: self.parallax_height_scale,
            multi_layer_inner_thickness: self.multi_layer_inner_thickness,
            multi_layer_refraction_scale: self.multi_layer_refraction_scale,
            multi_layer_inner_layer_scale: self.multi_layer_inner_layer_scale,
            multi_layer_envmap_strength: self.multi_layer_envmap_strength,
            sparkle_parameters: self.sparkle_parameters,
        }
    }
}

pub(super) fn apply_alpha_flags(info: &mut MaterialInfo, alpha: &NiAlphaProperty) {
    let blend = alpha.flags & 0x001 != 0;
    let test = alpha.flags & 0x200 != 0;
    // Extract blend factors regardless of which mode wins — they're
    // needed if the mesh later ends up blended (e.g., animated alpha).
    info.src_blend_mode = ((alpha.flags >> 1) & 0xF) as u8; // bits 1–4
    info.dst_blend_mode = ((alpha.flags >> 5) & 0xF) as u8; // bits 5–8
    if test {
        info.alpha_test = true;
        info.alpha_threshold = alpha.threshold as f32 / 255.0;
        // Bits 10-12: alpha test comparison function (3 bits, 0–7).
        info.alpha_test_func = ((alpha.flags & 0x1C00) >> 10) as u8;
        // Prefer cutout to blending when both are set.
        info.alpha_blend = false;
    } else if blend {
        info.alpha_blend = true;
    }
}

/// Resolve a `TexDesc` slot on an `NiTexturingProperty` to a texture
/// filename by following its `source_ref` through the scene's block
/// table and pulling the filename from the referenced
/// `NiSourceTexture`. Returns `None` if the slot is empty, the ref
/// is null, or the source texture has no external filename (embedded
/// NiPixelData is not supported here — the downstream texture
/// provider can't resolve those anyway). See issue #131.
fn tex_desc_source_path(scene: &NifScene, desc: Option<&TexDesc>) -> Option<String> {
    let desc = desc?;
    let src_idx = desc.source_ref.index()?;
    let src_tex = scene.get_as::<NiSourceTexture>(src_idx)?;
    src_tex.filename.as_ref().map(|f| f.to_string())
}

#[cfg(test)]

mod alpha_flag_tests;

/// Regression tests for issue #345 — `BSEffectShaderProperty` rich
/// material fields used to be dropped on import. The capture path is
/// covered by direct `capture_effect_shader_data` tests; full
/// `extract_material_info` coverage requires a synthetic NIF and is
/// blocked on test infrastructure (`NifScene` doesn't expose enough
/// mutators to wire one up cheaply). The capture helper is the entire
/// transform under test — `extract_material_info` just calls it.
#[cfg(test)]

mod effect_shader_capture_tests;

/// Regression tests for issue #343 — exhaustive ShaderTypeData dispatch.
/// Previously only `EnvironmentMap` reached MaterialInfo; the remaining
/// 8 variants (SkinTint, Fo76SkinTint, HairTint, ParallaxOcc,
/// MultiLayerParallax, SparkleSnow, EyeEnvmap, None) were dropped. Each
/// test exercises one arm of `apply_shader_type_data`.
#[cfg(test)]

mod shader_type_data_tests;

/// Regression tests for issue #214 — NiTexturingProperty secondary slots
/// and NiVertexColorProperty mode extraction.
#[cfg(test)]

mod secondary_slot_tests;

/// Regression tests for #452 — `BSShaderTextureSet` slots 3/4/5 must
/// reach the importer via both the FO3/FNV `BSShaderPPLightingProperty`
/// path and the Skyrim+ `BSLightingShaderProperty` path. Previously
/// the importer stopped at slot 2 so parallax walls rendered flat and
/// glass/power-armor env reflections never bound.
#[cfg(test)]

mod texture_slot_3_4_5_tests;

/// Regression tests for #441 — `SF_DOUBLE_SIDED = 0x1000` is NOT
/// Double_Sided on the FO3/FNV `BSShaderFlags` pair. Pre-fix the
/// importer marked every PPLighting / NoLighting mesh that happened
/// to set flags1 bit 12 (`Unknown_3`) as two-sided, rendering
/// foliage / hair / banner cloth with wrong backface culling. The
/// Skyrim+ `BSLightingShaderProperty` path (flags2 bit 4) is
/// unaffected.
#[cfg(test)]

mod double_sided_tests;
