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

/// Lift a `BSEffectShaderProperty` into the importer's
/// [`BsEffectShaderData`] capture struct. Empty string fields collapse
/// to `None`. Pre-FO76 inputs leave `refraction_power = None`. See
/// #345 / audit S4-01.
fn capture_effect_shader_data(shader: &BSEffectShaderProperty) -> BsEffectShaderData {
    fn opt(s: &str) -> Option<String> {
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    }
    BsEffectShaderData {
        falloff_start_angle: shader.falloff_start_angle,
        falloff_stop_angle: shader.falloff_stop_angle,
        falloff_start_opacity: shader.falloff_start_opacity,
        falloff_stop_opacity: shader.falloff_stop_opacity,
        soft_falloff_depth: shader.soft_falloff_depth,
        greyscale_texture: opt(&shader.greyscale_texture),
        env_map_texture: opt(&shader.env_map_texture),
        normal_texture: opt(&shader.normal_texture),
        env_mask_texture: opt(&shader.env_mask_texture),
        env_map_scale: shader.env_map_scale,
        // refraction_power is FO76-only; the parser fills it with 0.0
        // on pre-FO76. Surface as `None` so the shader-side dispatch
        // can branch on `Some(p)` instead of guessing whether 0.0
        // means "off" or "FO76 with literal 0".
        refraction_power: (shader.refraction_power != 0.0).then_some(shader.refraction_power),
        lighting_influence: shader.lighting_influence,
        env_map_min_lod: shader.env_map_min_lod,
        texture_clamp_mode: shader.texture_clamp_mode,
    }
}

/// Extract vertex colors using a pre-computed `MaterialInfo`.
///
/// Reads `mat.vertex_color_mode` and `mat.diffuse_color` directly instead
/// of re-walking the property list. Pre-#438 this function ignored its
/// `_mat` parameter and re-scanned the shape + inherited properties twice
/// (once for vertex-color mode, once for diffuse fallback), costing 3×
/// the property-list work per NiTriShape on top of the initial
/// `extract_material_info` scan at the caller.
pub(super) fn extract_vertex_colors(
    _scene: &NifScene,
    _shape: &NiTriShape,
    data: &GeomData,
    _inherited_props: &[BlockRef],
    mat: &MaterialInfo,
) -> Vec<[f32; 4]> {
    let num_verts = data.vertices.len();

    let use_vertex_colors =
        !data.vertex_colors.is_empty() && mat.vertex_color_mode == VertexColorMode::AmbientDiffuse;

    // Keep the alpha lane — authored per-vertex modulation on hair tip
    // cards, eyelash strips, and BSEffectShader meshes is the source of
    // truth for those surfaces. See #618.
    if use_vertex_colors {
        return data.vertex_colors.to_vec();
    }

    let d = mat.diffuse_color;
    vec![[d[0], d[1], d[2], 1.0]; num_verts]
}

/// Extract all material properties from a NiTriShape in a single pass.
///
/// `inherited_props` carries property BlockRefs accumulated from parent
/// NiNodes during the scene graph walk. Gamebryo propagates properties
/// down the hierarchy — child shapes inherit parent properties unless
/// they override them with their own. Shape-level properties take
/// priority; inherited properties fill in any gaps. See #208.
pub(super) fn extract_material_info(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited_props: &[BlockRef],
) -> MaterialInfo {
    extract_material_info_from_refs(
        scene,
        shape.shader_property_ref,
        shape.alpha_property_ref,
        &shape.av.properties,
        inherited_props,
    )
}

/// Block-ref-parameterised core of [`extract_material_info`].
///
/// Both the `NiTriShape` path (via the thin wrapper above) and the
/// `BsTriShape` path share this implementation so parity drift
/// between them — NIF-404 / NIF-403 — can't re-emerge. BsTriShape
/// passes empty slices for `direct_properties` and `inherited_props`
/// because Skyrim+ geometry binds properties via the dedicated
/// `shader_property_ref` / `alpha_property_ref` fields rather than
/// the legacy NiProperty chain. See #129.
pub(super) fn extract_material_info_from_refs(
    scene: &NifScene,
    shader_property_ref: BlockRef,
    alpha_property_ref: BlockRef,
    direct_properties: &[BlockRef],
    inherited_props: &[BlockRef],
) -> MaterialInfo {
    let mut info = MaterialInfo::default();

    // Skyrim+: dedicated shader_property_ref
    if let Some(idx) = shader_property_ref.index() {
        if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
            if let Some(name) = shader.net.name.as_deref() {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".bgsm") || lower.ends_with(".bgem") {
                    info.material_path = Some(name.to_string());
                }
            }
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
                    // Glow / emissive map is textures[2].
                    if info.glow_map.is_none() {
                        if let Some(glow) = tex_set.textures.get(2).filter(|s| !s.is_empty()) {
                            info.glow_map = Some(glow.clone());
                        }
                    }
                    // Parallax / height (textures[3]). Used by
                    // BSLightingShaderProperty ParallaxOcc +
                    // MultiLayerParallax shader-type variants. The
                    // scale / passes scalars already arrive via
                    // `apply_shader_type_data`; pair them with the
                    // texture here. #452.
                    if info.parallax_map.is_none() {
                        if let Some(px) = tex_set.textures.get(3).filter(|s| !s.is_empty()) {
                            info.parallax_map = Some(px.clone());
                        }
                    }
                    // Env cube (textures[4]) + env mask (textures[5])
                    // — reach the renderer alongside the existing
                    // `env_map_scale`. #452.
                    if info.env_map.is_none() {
                        if let Some(env) = tex_set.textures.get(4).filter(|s| !s.is_empty()) {
                            info.env_map = Some(env.clone());
                        }
                    }
                    if info.env_mask.is_none() {
                        if let Some(mask) = tex_set.textures.get(5).filter(|s| !s.is_empty()) {
                            info.env_mask = Some(mask.clone());
                        }
                    }
                }
            }
            // Skyrim/FO4 Double_Sided lives on flags2 bit 4 on
            // `BSLightingShaderProperty` per nif.xml `SkyrimShaderPropertyFlags2`
            // / `Fallout4ShaderPropertyFlags2`. See #441 for why this
            // check is NOT shared with the FO3/FNV PPLighting path.
            if shader.shader_flags_2 & SF2_DOUBLE_SIDED != 0 {
                info.two_sided = true;
            }
            // Skyrim+/FO4 decal path — flags2 bit 21 is `Cloud_LOD` on
            // Skyrim / `Anisotropic_Lighting` on FO4, NOT a decal bit.
            // See #414.
            if is_decal_from_modern_shader_flags(shader.shader_flags_1, shader.shader_flags_2) {
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
            info.has_uv_transform = true;
            info.alpha = shader.alpha;
            info.material_kind = shader.shader_type as u8;
            apply_shader_type_data(&mut info, &shader.shader_type_data);
            info.has_material_data = true;
        }
        if let Some(shader) = scene.get_as::<BSEffectShaderProperty>(idx) {
            if info.material_path.is_none() {
                info.material_path =
                    crate::import::mesh::material_path_from_name(shader.net.name.as_deref());
            }
            if info.texture_path.is_none() && !shader.source_texture.is_empty() {
                info.texture_path = Some(shader.source_texture.clone());
            }
            if !info.has_material_data {
                // BSEffect's base_color is semantically a diffuse
                // tint, not emissive (#166 renamed from emissive_*).
                // We still route it into emissive_color/emissive_mult
                // because the effect shader's visible "glow" comes
                // from `base_color * base_color_scale` in the current
                // fragment-shader path. A proper diffuse-tint
                // remapping is downstream work once effect-shader
                // surfaces get their own render path.
                info.emissive_color = [
                    shader.base_color[0],
                    shader.base_color[1],
                    shader.base_color[2],
                ];
                info.emissive_mult = shader.base_color_scale;
                info.uv_offset = shader.uv_offset;
                info.uv_scale = shader.uv_scale;
                info.has_uv_transform = true;
                // `base_color[3]` is BGEM's alpha — the existing
                // `NiAlphaProperty` / `info.alpha_blend` path owns
                // binary transparency, but `mat_alpha` rides through
                // to the shader as a per-instance multiplier.
                // Pre-#129 the BsTriShape path captured this
                // explicitly and the NiTriShape path lost it.
                info.alpha = shader.base_color[3];
                // FO4+ effect shaders (BSVER >= 130) carry their own
                // normal + env maps alongside the greyscale palette.
                // Pre-#129 only the BsTriShape path read them.
                if !shader.normal_texture.is_empty() {
                    info.normal_map = Some(shader.normal_texture.clone());
                }
                info.env_map_scale = shader.env_map_scale;
                info.has_material_data = true;
            }
            // Double_Sided (`shader_flags_2 & 0x10`) and the decal
            // flags apply on BSEffectShaderProperty with the same
            // semantics as BSLightingShaderProperty. Pre-#129 the
            // BsTriShape path checked them explicitly via
            // `bs_tri_shape_two_sided` / `find_decal_bs`; folding those
            // checks in here keeps both paths in lockstep.
            if shader.shader_flags_2 & SF2_DOUBLE_SIDED != 0 {
                info.two_sided = true;
            }
            // Skyrim+/FO4 effect-shader decal path — same rationale as
            // the BSLightingShaderProperty branch above. See #414.
            if is_decal_from_modern_shader_flags(shader.shader_flags_1, shader.shader_flags_2) {
                info.is_decal = true;
            }
            // Capture the rich effect-shader fields (falloff cone,
            // greyscale palette, FO4+/FO76 companion textures, etc.)
            // so downstream consumers can route them when the renderer-
            // side dispatch lands. See #345 / audit S4-01.
            info.effect_shader = Some(capture_effect_shader_data(shader));
            // #706 / FX-1 — flag the material as effect-shader for the
            // renderer's `material_kind` dispatch. Routes through the
            // existing u8 ladder (same plumbing the BSLightingShaderProperty
            // shader_type uses) into `triangle.frag`'s `MATERIAL_KIND_EFFECT_SHADER`
            // branch, which short-circuits lit shading and emits only
            // `emissive_color * emissive_mult * texColor.rgba`. Without
            // this flag, fire / magic / glow planes get scene-lit by
            // every nearby point light + ambient + RT GI bounce — pure
            // emissive surfaces are then modulated against scene colors
            // and render rainbow. See #706.
            //
            // 101 fits in the `u8` field (max 255). The contract on
            // `MaterialInfo.material_kind` widens here: 0..=19 is the
            // BSLightingShaderProperty shader_type; >= 100 is an
            // engine-synthesized kind (mirrors the Glass = 100 pattern
            // already shipped in scene_buffer.rs). The variant-specific
            // packs in render.rs gate on `base_material_kind == N` for
            // N in {5, 6, 11, 14, 16}, none of which collide with 101.
            // 101 = MATERIAL_KIND_EFFECT_SHADER (defined in
            // `byroredux-renderer/src/vulkan/scene_buffer.rs`). Inlined
            // here as a literal because the nif crate is upstream of
            // renderer in the dep graph; the existing test
            // `effect_shader_sets_material_kind_to_101` pins the value.
            info.material_kind = 101;
            // Implicit alpha blend: BSEffectShaderProperty is the
            // Skyrim+ transparency source of truth. Bethesda effect
            // NIFs frequently omit NiAlphaProperty entirely because
            // BGEM/shader data owns the blend — without this flag,
            // `meshes/effects/*.nif` (glow rings, magic flares, dust
            // planes, smoke cards) render as opaque planes with hard
            // edges. Only flip when the shape hasn't already bound a
            // NiAlphaProperty (that path owns explicit src/dst blend
            // factors and must not be overwritten). See #354 / audit
            // S4-03.
            if !info.alpha_blend && !info.alpha_test {
                info.alpha_blend = true;
                // The src/dst defaults live on `MaterialInfo::default`
                // as SRC_ALPHA / INV_SRC_ALPHA — correct for the
                // falloff-cone case which is the common one. Additive
                // blend (ONE / ONE) for Own_Emit / EnvMap_Light_Fade
                // flagged effect meshes is the remaining half of this
                // issue and needs a per-flag check before the src/dst
                // rewrite; defer to the follow-up.
            }
        }
    }

    // Skyrim+: dedicated alpha_property_ref
    if let Some(idx) = alpha_property_ref.index() {
        if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
            apply_alpha_flags(&mut info, alpha);
        }
    }

    // FO3/FNV/Oblivion: single pass over shape + inherited properties.
    // Shape properties first so they take priority (#208). Empty for
    // BsTriShape (Skyrim+ binds via shader_property_ref only).
    for prop_ref in direct_properties.iter().chain(inherited_props.iter()) {
        let Some(idx) = prop_ref.index() else {
            continue;
        };

        if !info.alpha_blend && !info.alpha_test {
            if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
                apply_alpha_flags(&mut info, alpha);
            }
        }

        // NiZBufferProperty — depth test/write mode + comparison function (#398).
        if let Some(zbuf) = scene.get_as::<crate::blocks::properties::NiZBufferProperty>(idx) {
            info.z_test = zbuf.z_test_enabled;
            info.z_write = zbuf.z_write_enabled;
            // Clamp to the 8 Gamebryo TestFunction values; out-of-range
            // (file corruption / unimplemented variant) falls back to
            // LESSEQUAL via the Default.
            if zbuf.z_function < 8 {
                info.z_function = zbuf.z_function as u8;
            }
        }

        // NiMaterialProperty — capture specular/emissive/shininess/alpha.
        if !info.has_material_data {
            if let Some(mat) = scene.get_as::<NiMaterialProperty>(idx) {
                info.diffuse_color = [mat.diffuse.r, mat.diffuse.g, mat.diffuse.b];
                // #221 — `NiMaterialProperty.ambient` was previously
                // discarded; the renderer now consumes it as a
                // per-material modulator on the cell ambient term.
                info.ambient_color = [mat.ambient.r, mat.ambient.g, mat.ambient.b];
                info.specular_color = [mat.specular.r, mat.specular.g, mat.specular.b];
                info.emissive_color = [mat.emissive.r, mat.emissive.g, mat.emissive.b];
                info.glossiness = mat.shininess;
                info.alpha = mat.alpha;
                info.emissive_mult = mat.emissive_mult;
                info.has_material_data = true;
            }
        }

        if let Some(tex_prop) = scene.get_as::<NiTexturingProperty>(idx) {
            if info.texture_path.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.base_texture.as_ref()) {
                    info.texture_path = Some(path);
                }
            }
            // Oblivion stores tangent-space normal maps in the `bump_texture`
            // slot (the dedicated `normal_texture` slot landed later in FO3).
            // Skyrim+ meshes use BSShaderTextureSet handled elsewhere, so
            // this branch is specifically for pre-Skyrim static meshes.
            // See issue #131.
            if info.normal_map.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.normal_texture.as_ref())
                    .or_else(|| tex_desc_source_path(scene, tex_prop.bump_texture.as_ref()))
                {
                    info.normal_map = Some(path);
                }
            }
            // Secondary texture slots (#214). NiTexturingProperty has
            // up to 8 slots — base and normal/bump are consumed above,
            // the remaining three slots we care about feed separate
            // shader inputs:
            //   * glow_texture  → emissive map (self-illumination)
            //   * detail_texture → high-frequency overlay
            //   * gloss_texture  → per-texel specular strength mask
            // We only overwrite if a Skyrim+ BSShader path hasn't
            // already set them, matching the base/normal policy.
            if info.glow_map.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.glow_texture.as_ref()) {
                    info.glow_map = Some(path);
                }
            }
            if info.detail_map.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.detail_texture.as_ref()) {
                    info.detail_map = Some(path);
                }
            }
            if info.gloss_map.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.gloss_texture.as_ref()) {
                    info.gloss_map = Some(path);
                }
            }
            // Dark / multiplicative lightmap (slot 1). Baked shadow data
            // on Oblivion interior architecture — `albedo *= dark`. #264.
            if info.dark_map.is_none() {
                if let Some(path) = tex_desc_source_path(scene, tex_prop.dark_texture.as_ref()) {
                    info.dark_map = Some(path);
                }
            }
            // Parallax height-map (slot 7, v20.2.0.5+). Pre-#450 the
            // parser consumed + dropped this slot so FO3 meshes that
            // kept the legacy `NiTexturingProperty` chain alongside a
            // `BSShaderPPLightingProperty` lost their parallax bake.
            // Feed the same downstream field as the BSShaderTextureSet
            // slot 3 path at line 532 so the shader does not need to
            // distinguish the two sources.
            if info.parallax_map.is_none() {
                if let Some(path) =
                    tex_desc_source_path(scene, tex_prop.parallax_texture.as_ref())
                {
                    info.parallax_map = Some(path);
                }
            }
            // Decal slots (0..=3 per nif.xml). Append every slot whose
            // `source_ref` resolves to a real filename; inherited props
            // only contribute when the shape itself has no decals yet,
            // matching the precedence rule used by the other slots.
            // #400 / OBL-D4-H4.
            if info.decal_maps.is_empty() {
                for desc in &tex_prop.decal_textures {
                    if let Some(path) = tex_desc_source_path(scene, Some(desc)) {
                        info.decal_maps.push(path);
                    }
                }
            }
            // Propagate the base slot's UV transform to the shared
            // `uv_offset` / `uv_scale` fields. The renderer shader applies
            // them per-vertex to every sampled texture — fine for the
            // common case where base, detail, glow and parallax share a
            // UV set, which holds for Oblivion/FO3/FNV static meshes. See
            // issues #219 and #435. Only overwrite when no shader path
            // earlier in the pass has already supplied a UV transform —
            // gated on `has_uv_transform` rather than `has_material_data`
            // (the latter is set by `NiMaterialProperty`, which carries
            // no UV transform of its own and so was wrongly suppressing
            // this branch when it preceded `NiTexturingProperty` in
            // Oblivion / FO3 / FNV property arrays).
            if !info.has_uv_transform {
                if let Some(base) = tex_prop.base_texture.as_ref() {
                    if let Some(tx) = base.transform {
                        info.uv_offset = tx.translation;
                        info.uv_scale = tx.scale;
                        info.has_uv_transform = true;
                    }
                }
            }
        }

        if let Some(shader) = scene.get_as::<BSShaderPPLightingProperty>(idx) {
            if let Some(ts_idx) = shader.texture_set_ref.index() {
                if let Some(tex_set) = scene.get_as::<BSShaderTextureSet>(ts_idx) {
                    if info.texture_path.is_none() {
                        if let Some(path) = tex_set.textures.first() {
                            if !path.is_empty() {
                                info.texture_path = Some(path.clone());
                            }
                        }
                    }
                    // Normal map is textures[1] in BSShaderTextureSet (same layout as Skyrim).
                    if info.normal_map.is_none() {
                        if let Some(normal) = tex_set.textures.get(1) {
                            if !normal.is_empty() {
                                info.normal_map = Some(normal.clone());
                            }
                        }
                    }
                    // Glow / emissive map is textures[2].
                    if info.glow_map.is_none() {
                        if let Some(glow) = tex_set.textures.get(2).filter(|s| !s.is_empty()) {
                            info.glow_map = Some(glow.clone());
                        }
                    }
                    // Parallax / height map is textures[3] (FO3/FNV
                    // Parallax_Shader_Index_15 / Parallax_Occlusion).
                    // See #452.
                    if info.parallax_map.is_none() {
                        if let Some(px) = tex_set.textures.get(3).filter(|s| !s.is_empty()) {
                            info.parallax_map = Some(px.clone());
                        }
                    }
                    // Environment cubemap is textures[4]. Glass bottles,
                    // power armor, polished metal — pre-#452 the path was
                    // read and thrown away. env_map_scale was captured
                    // but had no texture to route to.
                    if info.env_map.is_none() {
                        if let Some(env) = tex_set.textures.get(4).filter(|s| !s.is_empty()) {
                            info.env_map = Some(env.clone());
                        }
                    }
                    // Environment-reflection mask is textures[5]. #452.
                    if info.env_mask.is_none() {
                        if let Some(mask) = tex_set.textures.get(5).filter(|s| !s.is_empty()) {
                            info.env_mask = Some(mask.clone());
                        }
                    }
                }
            }
            // `BSShaderPPLightingProperty.parallax_max_passes` /
            // `parallax_scale` (parsed since BSVER >= 24 per
            // `blocks/shader.rs:70`) flow straight through. Only
            // overwrite when the material hasn't already bound them
            // from a Skyrim+ BSLightingShaderProperty ParallaxOcc
            // variant — the shader-type capture path in
            // `apply_shader_type_data` keeps those values. #452.
            if info.parallax_max_passes.is_none() {
                info.parallax_max_passes = Some(shader.parallax_max_passes);
            }
            if info.parallax_height_scale.is_none() {
                info.parallax_height_scale = Some(shader.parallax_scale);
            }
            // FO3/FNV `BSShaderPPLightingProperty` has NO Double_Sided
            // bit on either flag pair — see the SF_DOUBLE_SIDED
            // explanatory block at the top of this file. Leave
            // `two_sided` unset here; the `NiStencilProperty` fallback
            // below handles it correctly for meshes that want
            // back-face-off.
            if is_decal_from_legacy_shader_flags(shader.shader_flags_1(), shader.shader_flags_2()) {
                info.is_decal = true;
            }
        }

        if let Some(shader) = scene.get_as::<BSShaderNoLightingProperty>(idx) {
            if info.texture_path.is_none() && !shader.file_name.is_empty() {
                info.texture_path = Some(shader.file_name.clone());
            }
            // Same rationale as the PPLighting branch above: no Double_Sided
            // bit on the FO3/FNV flag enum. #441. Pre-#454 this branch
            // was missing the `ALPHA_DECAL_F2` (flag2 bit 21) check, so
            // blood-splat NoLighting meshes that marked themselves decal
            // via only the flag2 bit fell through to the opaque-coplanar
            // path. Shared helper keeps PP + NoLighting in lockstep.
            if is_decal_from_legacy_shader_flags(shader.shader_flags_1(), shader.shader_flags_2()) {
                info.is_decal = true;
            }
            // Capture the soft-falloff cone so the HUD / VATS / scope
            // overlay pipelines can eventually consume it. Pre-#451 the
            // four scalars were silently discarded (parser extracted
            // them but the importer had no field to receive them).
            // Don't overwrite a previously-captured falloff set: if the
            // mesh somehow binds both a NoLighting and an effect block
            // the caller-most wins, matching the other shader-field
            // merging in this loop.
            info.no_lighting_falloff.get_or_insert(NoLightingFalloff {
                start_angle: shader.falloff_start_angle,
                stop_angle: shader.falloff_stop_angle,
                start_opacity: shader.falloff_start_opacity,
                stop_opacity: shader.falloff_stop_opacity,
            });
        }

        // NiStencilProperty — proper parser replaces NiUnknown heuristic.
        if !info.two_sided {
            if let Some(stencil) = scene.get_as::<NiStencilProperty>(idx) {
                if stencil.is_two_sided() {
                    info.two_sided = true;
                }
            }
        }

        // NiSpecularProperty (issue #220) — bit 0 of flags is the enable
        // toggle. Many matte surfaces in Oblivion/FNV set flags=0 here to
        // explicitly disable specular; without honoring this, every wall
        // and ceiling panel gets a bright PBR specular highlight from
        // point lights that looks like a lighting artifact.
        //
        // We use a type_name match because `NiFlagProperty` is shared by
        // NiSpecular/Wireframe/Dither/Shade and we only care about
        // specular here.
        if let Some(flag_prop) = scene.get_as::<NiFlagProperty>(idx) {
            if flag_prop.block_type_name() == "NiSpecularProperty" && !flag_prop.enabled() {
                info.specular_enabled = false;
            }
        }

        // NiVertexColorProperty (#214) — controls how per-vertex colors
        // participate in shading. The default is AmbientDiffuse; the
        // mesh may instead request Ignore (don't use vertex colors at
        // all) or Emissive (route them to self-illumination). The
        // actual behavior split on Ignore is enforced by
        // `extract_material` below when it decides whether to return
        // the vertex color vec or fall back to the material diffuse.
        //
        // #694 — the property carries a second enum, `lighting_mode`,
        // that gates which lighting terms actually consume the vertex
        // color. `from_property` collapses the 2D enum into our 1D
        // `VertexColorMode` axis: when LIGHTING_E drops the
        // ambient/diffuse contributions, a SRC_AMB_DIFF vertex_mode
        // becomes effectively invisible — demote to `Ignore` so the
        // renderer skips the `texColor * fragColor` double-count.
        if let Some(vcol) = scene.get_as::<NiVertexColorProperty>(idx) {
            info.vertex_color_mode =
                VertexColorMode::from_property(vcol.vertex_mode, vcol.lighting_mode);
        }
    }

    // Zero out specular strength **and color** when the property is
    // disabled. We do this once at the end so later code (pipeline
    // selection, draw command population) doesn't need to know about
    // the flag.
    //
    // #696 — clearing `specular_strength` alone is insufficient on
    // glass-classified meshes. The IOR glass branch in
    // `triangle.frag:1004` does `specStrength = max(specStrength,
    // 3.0)`, which silently re-promotes the spec term on every glass
    // surface even when the NIF said `NiSpecularProperty { flags: 0 }`.
    // The downstream BRDF multiplies (`specStrength * specColor` at
    // lines 1293 + 1396) then gate on the *color* — zeroing it here
    // collapses both glass-IOR and standard paths to zero spec
    // contribution as the original engine would.
    if !info.specular_enabled {
        info.specular_strength = 0.0;
        info.specular_color = [0.0, 0.0, 0.0];
    }

    info
}

/// Decode an NiAlphaProperty onto a `MaterialInfo`. `NiAlphaProperty.flags`
/// packs both alpha-blend (bit 0) and alpha-test (bit 9, mask 0x200) per
/// nif.xml; `threshold` is a u8 in [0, 255]. See issue #152.
///
/// When a material sets both bits (common on Gamebryo foliage, hair,
/// chain-link fences) we prefer alpha-test over alpha-blend — the
/// discard + opaque-depth path gives clean cutouts without the z-sort
/// artifacts that plague back-to-front blend on statics. `alpha_blend`
/// is intentionally set to `false` in that case so the renderer binds
/// the opaque pipeline.
/// Write every `ShaderTypeData` variant's trailing fields onto
/// `MaterialInfo`. Previously only `EnvironmentMap` was consumed; the
/// remaining 8 variants (SkinTint, Fo76SkinTint, HairTint, ParallaxOcc,
/// MultiLayerParallax, SparkleSnow, EyeEnvmap, and the `None`
/// pass-through for types that carry no trailing data) were pattern-
/// matched out and dropped. Issue #343 / SK-D3-01.
///
/// Renderer-side dispatch on `MaterialInfo.material_kind` is tracked
/// separately (SK-D3-02). Until that lands these values ride unused on
/// the `Material` component; the purpose here is to ensure no variant
/// is silently discarded at the import boundary.
pub(super) fn apply_shader_type_data(info: &mut MaterialInfo, data: &ShaderTypeData) {
    // Env-map scale lives on its own field for backwards compatibility with
    // pre-#343 readers; the other variants copy through `ShaderTypeFields`.
    if let ShaderTypeData::EnvironmentMap { env_map_scale } = *data {
        info.env_map_scale = env_map_scale;
    }
    // FO76 BSShaderType155 numbers SkinTint as 4 (Color4), but the
    // legacy BSLightingShaderType + the renderer's `materialKind == 5u`
    // branch use 5. The Color4 alpha and Color3 RGB are both written
    // into the same `skin_tint_color` + `skin_tint_alpha` slots
    // upstream (Skyrim's Color3 path leaves alpha defaulted to 1.0,
    // FO76's Color4 path supplies a real value), and the shader's
    // `mix(albedo, albedo*tint, alpha)` formula handles both. Remap
    // here so every NPC/creature reaches the same shader branch.
    // See #612 / SK-D3-04.
    if matches!(data, ShaderTypeData::Fo76SkinTint { .. }) {
        info.material_kind = 5;
    }
    let fields = capture_shader_type_fields(data);
    // #623 / SK-D3-07: GpuInstance packs `multi_layer_envmap_strength`
    // into the `w` slot of the `hair_tint_{r,g,b,_}` vec4 to save
    // alignment padding (scene_buffer.rs:240-263). The packing is
    // safe only because `ShaderTypeData` is a single-tag enum, so
    // `capture_shader_type_fields` populates one or the other but
    // never both. If a future variant or refactor breaks that, the
    // assert fires before we silently render hair-tinted meshes
    // with a stray multi-layer envmap strength (or vice versa).
    debug_assert!(
        fields.hair_tint_color.is_none() || fields.multi_layer_envmap_strength.is_none(),
        "GpuInstance vec4 share is broken: hair_tint and multi_layer_envmap_strength \
         must never appear together in a single ShaderTypeData capture"
    );
    info.skin_tint_color = fields.skin_tint_color.or(info.skin_tint_color);
    info.skin_tint_alpha = fields.skin_tint_alpha.or(info.skin_tint_alpha);
    info.hair_tint_color = fields.hair_tint_color.or(info.hair_tint_color);
    info.eye_cubemap_scale = fields.eye_cubemap_scale.or(info.eye_cubemap_scale);
    info.eye_left_reflection_center = fields
        .eye_left_reflection_center
        .or(info.eye_left_reflection_center);
    info.eye_right_reflection_center = fields
        .eye_right_reflection_center
        .or(info.eye_right_reflection_center);
    info.parallax_max_passes = fields.parallax_max_passes.or(info.parallax_max_passes);
    info.parallax_height_scale = fields
        .parallax_height_scale
        .or(info.parallax_height_scale);
    info.multi_layer_inner_thickness = fields
        .multi_layer_inner_thickness
        .or(info.multi_layer_inner_thickness);
    info.multi_layer_refraction_scale = fields
        .multi_layer_refraction_scale
        .or(info.multi_layer_refraction_scale);
    info.multi_layer_inner_layer_scale = fields
        .multi_layer_inner_layer_scale
        .or(info.multi_layer_inner_layer_scale);
    info.multi_layer_envmap_strength = fields
        .multi_layer_envmap_strength
        .or(info.multi_layer_envmap_strength);
    info.sparkle_parameters = fields.sparkle_parameters.or(info.sparkle_parameters);
}

/// The 13 shader-type-specific fields pulled off `BSLightingShaderProperty`'s
/// `shader_type_data` variant. Mirrors the flat fields on `MaterialInfo` so
/// both the NiTriShape path (via `MaterialInfo`) and the BsTriShape path
/// (direct) can populate the same `ImportedMesh` fields without duplication.
/// See #430 / NIF-D4-N01.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShaderTypeFields {
    pub skin_tint_color: Option<[f32; 3]>,
    pub skin_tint_alpha: Option<f32>,
    pub hair_tint_color: Option<[f32; 3]>,
    pub eye_cubemap_scale: Option<f32>,
    pub eye_left_reflection_center: Option<[f32; 3]>,
    pub eye_right_reflection_center: Option<[f32; 3]>,
    pub parallax_max_passes: Option<f32>,
    pub parallax_height_scale: Option<f32>,
    pub multi_layer_inner_thickness: Option<f32>,
    pub multi_layer_refraction_scale: Option<f32>,
    pub multi_layer_inner_layer_scale: Option<[f32; 2]>,
    pub multi_layer_envmap_strength: Option<f32>,
    pub sparkle_parameters: Option<[f32; 4]>,
}

/// Pull the shader-type-specific trailing fields out of a `ShaderTypeData`
/// into a flat `ShaderTypeFields` bundle. Complements
/// [`apply_shader_type_data`] — both are exhaustive on the 9 variants so
/// any future addition fails compilation here.
pub(crate) fn capture_shader_type_fields(data: &ShaderTypeData) -> ShaderTypeFields {
    let mut f = ShaderTypeFields::default();
    match *data {
        ShaderTypeData::None | ShaderTypeData::EnvironmentMap { .. } => {}
        ShaderTypeData::SkinTint { skin_tint_color } => {
            f.skin_tint_color = Some(skin_tint_color);
        }
        ShaderTypeData::Fo76SkinTint { skin_tint_color } => {
            f.skin_tint_color = Some([skin_tint_color[0], skin_tint_color[1], skin_tint_color[2]]);
            f.skin_tint_alpha = Some(skin_tint_color[3]);
        }
        ShaderTypeData::HairTint { hair_tint_color } => {
            f.hair_tint_color = Some(hair_tint_color);
        }
        ShaderTypeData::ParallaxOcc { max_passes, scale } => {
            f.parallax_max_passes = Some(max_passes);
            f.parallax_height_scale = Some(scale);
        }
        ShaderTypeData::MultiLayerParallax {
            inner_layer_thickness,
            refraction_scale,
            inner_layer_texture_scale,
            envmap_strength,
        } => {
            f.multi_layer_inner_thickness = Some(inner_layer_thickness);
            f.multi_layer_refraction_scale = Some(refraction_scale);
            f.multi_layer_inner_layer_scale = Some(inner_layer_texture_scale);
            f.multi_layer_envmap_strength = Some(envmap_strength);
        }
        ShaderTypeData::SparkleSnow { sparkle_parameters } => {
            f.sparkle_parameters = Some(sparkle_parameters);
        }
        ShaderTypeData::EyeEnvmap {
            eye_cubemap_scale,
            left_eye_reflection_center,
            right_eye_reflection_center,
        } => {
            f.eye_cubemap_scale = Some(eye_cubemap_scale);
            f.eye_left_reflection_center = Some(left_eye_reflection_center);
            f.eye_right_reflection_center = Some(right_eye_reflection_center);
        }
    }
    f
}

impl ShaderTypeFields {
    /// `true` when every slot is empty — equivalent to
    /// `*self == ShaderTypeFields::default()`. Used to skip attaching
    /// a heap-allocated `Box<ShaderTypeFields>` on the ECS `Material`
    /// component for the 99% of meshes that use no Skyrim+ variant.
    pub fn is_empty(&self) -> bool {
        *self == ShaderTypeFields::default()
    }

    /// Convert into the ECS-side [`byroredux_core::ecs::components::material::ShaderTypeFields`]
    /// mirror type. Field-by-field copy — both shapes are intentionally
    /// identical. See #562.
    pub fn to_core(&self) -> byroredux_core::ecs::components::material::ShaderTypeFields {
        byroredux_core::ecs::components::material::ShaderTypeFields {
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
#[path = "material_alpha_flag_tests.rs"]
mod alpha_flag_tests;

/// Regression tests for issue #345 — `BSEffectShaderProperty` rich
/// material fields used to be dropped on import. The capture path is
/// covered by direct `capture_effect_shader_data` tests; full
/// `extract_material_info` coverage requires a synthetic NIF and is
/// blocked on test infrastructure (`NifScene` doesn't expose enough
/// mutators to wire one up cheaply). The capture helper is the entire
/// transform under test — `extract_material_info` just calls it.
#[cfg(test)]
#[path = "material_effect_shader_capture_tests.rs"]
mod effect_shader_capture_tests;

/// Regression tests for issue #343 — exhaustive ShaderTypeData dispatch.
/// Previously only `EnvironmentMap` reached MaterialInfo; the remaining
/// 8 variants (SkinTint, Fo76SkinTint, HairTint, ParallaxOcc,
/// MultiLayerParallax, SparkleSnow, EyeEnvmap, None) were dropped. Each
/// test exercises one arm of `apply_shader_type_data`.
#[cfg(test)]
#[path = "material_shader_type_data_tests.rs"]
mod shader_type_data_tests;

/// Regression tests for issue #214 — NiTexturingProperty secondary slots
/// and NiVertexColorProperty mode extraction.
#[cfg(test)]
#[path = "material_secondary_slot_tests.rs"]
mod secondary_slot_tests;

/// Regression tests for #452 — `BSShaderTextureSet` slots 3/4/5 must
/// reach the importer via both the FO3/FNV `BSShaderPPLightingProperty`
/// path and the Skyrim+ `BSLightingShaderProperty` path. Previously
/// the importer stopped at slot 2 so parallax walls rendered flat and
/// glass/power-armor env reflections never bound.
#[cfg(test)]
#[path = "material_texture_slot_3_4_5_tests.rs"]
mod texture_slot_3_4_5_tests;

/// Regression tests for #441 — `SF_DOUBLE_SIDED = 0x1000` is NOT
/// Double_Sided on the FO3/FNV `BSShaderFlags` pair. Pre-fix the
/// importer marked every PPLighting / NoLighting mesh that happened
/// to set flags1 bit 12 (`Unknown_3`) as two-sided, rendering
/// foliage / hair / banner cloth with wrong backface culling. The
/// Skyrim+ `BSLightingShaderProperty` path (flags2 bit 4) is
/// unaffected.
#[cfg(test)]
#[path = "material_double_sided_tests.rs"]
mod double_sided_tests;
