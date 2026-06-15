//! NIF material and texture property extraction.

use crate::blocks::properties::{
    NiAlphaProperty, NiFlagProperty, NiMaterialProperty, NiStencilProperty, NiTexturingProperty,
    NiVertexColorProperty, TexDesc,
};
use crate::blocks::shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderPropertyBaseOnly, BSShaderTextureSet, BSSkyShaderProperty,
    BSWaterShaderProperty, LuminanceParams, ShaderTypeData, SkyShaderProperty,
    TallGrassShaderProperty, TileShaderProperty, WaterShaderProperty,
};
use crate::blocks::texture::NiSourceTexture;
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::scene::NifScene;
use crate::types::BlockRef;
use byroredux_core::string::{FixedString, StringPool};

use super::mesh::GeomData;

/// Intern a non-empty path through the engine's `StringPool` and return
/// the resolved [`FixedString`] handle. Empty *and whitespace-only* inputs
/// collapse to `None` — matches the pre-#609 `Option<String>` semantic where an
/// empty path field meant "no texture for this slot".
///
/// Whitespace-only slot strings (`" "`, `"\t"`) on malformed / broken-exporter
/// content would otherwise intern to `Some(sym)` and bind the magenta checker
/// placeholder on the unconditionally-bound diffuse slot (see FO3-1-01 / #1541),
/// bypassing the `None`→`neutral_fallback()` early-out in the asset provider.
///
/// Centralised so every site that pulls a texture-slot name out of a
/// NIF block routes through one helper — the audit's "store FixedString
/// instead of String" recommendation reduces to swapping every
/// `Some(s.to_string())` / `Some(s.clone())` site over to this call.
/// See #609 / D6-NEW-01.
#[inline]
pub(super) fn intern_texture_path(pool: &mut StringPool, path: &str) -> Option<FixedString> {
    if path.trim().is_empty() {
        None
    } else {
        Some(pool.intern(path))
    }
}

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
const _: () =
    assert!(crate::shader_flags::fo4_slsf1::DECAL == crate::shader_flags::fo3nv_f1::DECAL);
const _: () = assert!(
    crate::shader_flags::fo4_slsf1::DYNAMIC_DECAL
        == crate::shader_flags::skyrim_slsf1::DYNAMIC_DECAL
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
/// Tests SLSF1 / F4SF1 bits 26/27 (`Decal` / `Dynamic_Decal`) AND, for
/// FO76 / Starfield content (BSVER >= 132), the union of the
/// `sf1_crcs` + `sf2_crcs` CRC32 arrays for the corresponding
/// `BSShaderCRC32::DECAL` / `DYNAMIC_DECAL` identifiers per nif.xml
/// lines 6532/6533. The legacy u32 fields are written as literal
/// zeros on BSVER >= 132 (parser branch in `shader.rs:604-608` per
/// the spec gate `bsver <= 130`), so without the CRC fallback every
/// FO76+ decal silently lost its flag — see #712 / NIF-D4-01.
///
/// `flags2` is unused but kept on the signature for callsite symmetry
/// with [`is_decal_from_legacy_shader_flags`] and to make a future
/// FO4 SLSF2 decal addition a one-line change.
///
/// The split from [`is_decal_from_legacy_shader_flags`] exists to keep
/// flags2 bit 21 (`Cloud_LOD` on Skyrim, `Anisotropic_Lighting` on
/// FO4, `Alpha_Decal` on FO3/FNV — three different semantics on the
/// same bit) out of the modern decal test. See #414 / FO4-D3-M1.
#[inline]
pub(super) fn is_decal_from_modern_shader_flags(
    flags1: u32,
    _flags2: u32,
    sf1_crcs: &[u32],
    sf2_crcs: &[u32],
) -> bool {
    use crate::shader_flags::bs_shader_crc32::{contains_any, DECAL, DYNAMIC_DECAL as DD_CRC};
    if flags1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 {
        return true;
    }
    let targets = [DECAL, DD_CRC];
    contains_any(sf1_crcs, &targets) || contains_any(sf2_crcs, &targets)
}

/// Two-sided detection for Skyrim+ / FO4 `BSLightingShaderProperty` +
/// `BSEffectShaderProperty`.
///
/// Tests SLSF2 / F4SF2 bit 4 (`Double_Sided`) AND, for FO76 / Starfield
/// content (BSVER >= 132), the union of `sf1_crcs` + `sf2_crcs` for
/// `BSShaderCRC32::TWO_SIDED` per nif.xml line 6524. Same rationale as
/// [`is_decal_from_modern_shader_flags`] — the legacy u32 fields are
/// zero on BSVER >= 132. See #712 / NIF-D4-01.
#[inline]
pub(super) fn is_two_sided_from_modern_shader_flags(
    _flags1: u32,
    flags2: u32,
    sf1_crcs: &[u32],
    sf2_crcs: &[u32],
) -> bool {
    use crate::shader_flags::bs_shader_crc32::{contains_any, TWO_SIDED};
    if flags2 & SF2_DOUBLE_SIDED != 0 {
        return true;
    }
    contains_any(sf1_crcs, &[TWO_SIDED]) || contains_any(sf2_crcs, &[TWO_SIDED])
}

/// Test a Skyrim+ / FO4 `BSEffectShaderProperty` flag bit against
/// both the typed-flag word and the FO76 / Starfield CRC32 list union
/// (#890 / SK-D4-NEW-04). Sites that need to capture one of the four
/// `BSEffect`-relevant bits route through this — the per-bit wrappers
/// [`is_soft_effect_from_modern_shader_flags`] /
/// [`is_palette_color_from_modern_shader_flags`] /
/// [`is_palette_alpha_from_modern_shader_flags`] /
/// [`is_effect_lit_from_modern_shader_flags`] keep call sites
/// self-documenting without paying a runtime cost.
///
/// `flags_word` is whichever of `shader_flags_1` / `shader_flags_2`
/// carries the typed bit position (`mask` chooses which) — the caller
/// passes the relevant word per the SLSF1/SLSF2 split. `sf1_crcs` and
/// `sf2_crcs` are the FO76+ CRC32 arrays; the typed-flag word is zero
/// on BSVER >= 132 so the CRC layer is the only signal there.
#[inline]
pub(super) fn modern_effect_shader_bit(
    flags_word: u32,
    mask: u32,
    sf1_crcs: &[u32],
    sf2_crcs: &[u32],
    crc: u32,
) -> bool {
    use crate::shader_flags::bs_shader_crc32::contains_any;
    if flags_word & mask != 0 {
        return true;
    }
    contains_any(sf1_crcs, &[crc]) || contains_any(sf2_crcs, &[crc])
}

/// `SLSF1::Soft_Effect` capture for `BSEffectShaderProperty` — bit 30
/// of `shader_flags_1` OR `BSShaderCRC32::SOFT_EFFECT` on the SF1/SF2
/// arrays. See #890.
#[inline]
pub(super) fn is_soft_effect_from_modern_shader_flags(
    flags1: u32,
    sf1_crcs: &[u32],
    sf2_crcs: &[u32],
) -> bool {
    use crate::shader_flags::bs_shader_crc32::SOFT_EFFECT;
    use crate::shader_flags::skyrim_slsf1;
    modern_effect_shader_bit(
        flags1,
        skyrim_slsf1::SOFT_EFFECT,
        sf1_crcs,
        sf2_crcs,
        SOFT_EFFECT,
    )
}

/// `SLSF1::Greyscale_To_PaletteColor` capture — bit 4 of `shader_flags_1`
/// OR `BSShaderCRC32::GRAYSCALE_TO_PALETTE_COLOR`. nif.xml uses the
/// American spelling on the CRC32 enum and the British spelling on the
/// typed flag; we honor both at the constant import sites. See #890.
#[inline]
pub(super) fn is_palette_color_from_modern_shader_flags(
    flags1: u32,
    sf1_crcs: &[u32],
    sf2_crcs: &[u32],
) -> bool {
    use crate::shader_flags::bs_shader_crc32::GRAYSCALE_TO_PALETTE_COLOR;
    use crate::shader_flags::skyrim_slsf1;
    modern_effect_shader_bit(
        flags1,
        skyrim_slsf1::GREYSCALE_TO_PALETTE_COLOR,
        sf1_crcs,
        sf2_crcs,
        GRAYSCALE_TO_PALETTE_COLOR,
    )
}

/// `SLSF1::Greyscale_To_PaletteAlpha` capture — bit 5 of `shader_flags_1`
/// OR `BSShaderCRC32::GRAYSCALE_TO_PALETTE_ALPHA`. See #890.
#[inline]
pub(super) fn is_palette_alpha_from_modern_shader_flags(
    flags1: u32,
    sf1_crcs: &[u32],
    sf2_crcs: &[u32],
) -> bool {
    use crate::shader_flags::bs_shader_crc32::GRAYSCALE_TO_PALETTE_ALPHA;
    use crate::shader_flags::skyrim_slsf1;
    modern_effect_shader_bit(
        flags1,
        skyrim_slsf1::GREYSCALE_TO_PALETTE_ALPHA,
        sf1_crcs,
        sf2_crcs,
        GRAYSCALE_TO_PALETTE_ALPHA,
    )
}

/// `SLSF2::Effect_Lighting` capture — bit 30 of `shader_flags_2` OR
/// `BSShaderCRC32::EFFECT_LIGHTING`. See #890.
#[inline]
pub(super) fn is_effect_lit_from_modern_shader_flags(
    flags2: u32,
    sf1_crcs: &[u32],
    sf2_crcs: &[u32],
) -> bool {
    use crate::shader_flags::bs_shader_crc32::EFFECT_LIGHTING;
    use crate::shader_flags::skyrim_slsf2;
    modern_effect_shader_bit(
        flags2,
        skyrim_slsf2::EFFECT_LIGHTING,
        sf1_crcs,
        sf2_crcs,
        EFFECT_LIGHTING,
    )
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
    crate::shader_flags::fo4_slsf2::DOUBLE_SIDED == crate::shader_flags::skyrim_slsf2::DOUBLE_SIDED
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
///
/// Texture-slot path fields hold [`FixedString`] handles into the
/// engine-wide [`StringPool`] (#609 / D6-NEW-01). Pre-fix every slot
/// stored an `Option<String>` and re-allocated the path on every
/// `MaterialInfo::clone` and every cell load — ~50 KB redundant heap
/// per ~200-mesh interior cell. Now a clone is a refcount + copy of
/// the symbol handle (4 bytes), and resolving back to a `&str` for the
/// texture provider is a `pool.resolve()` call away.
#[derive(Debug)]
pub(super) struct MaterialInfo {
    pub texture_path: Option<FixedString>,
    /// BGSM/BGEM material file reference (FO4+). Present when the
    /// BSLightingShaderProperty has a non-empty name.
    pub material_path: Option<FixedString>,
    pub normal_map: Option<FixedString>,
    /// Glow / self-illumination texture (NiTexturingProperty slot 4).
    /// Filled on Oblivion/FO3/FNV meshes where a dedicated emissive
    /// map supplements or replaces `NiMaterialProperty.emissive`. See #214.
    pub glow_map: Option<FixedString>,
    /// Detail overlay texture (NiTexturingProperty slot 2). Blends with
    /// the base texture at higher frequency; used for terrain detail
    /// variation and clothing micro-texture.
    pub detail_map: Option<FixedString>,
    /// Specular-mask / gloss texture (NiTexturingProperty slot 3).
    /// Per-texel specular strength; enables armor highlights masked
    /// by leather/fabric regions.
    pub gloss_map: Option<FixedString>,
    /// Dark / multiplicative lightmap texture (NiTexturingProperty slot 1).
    /// Baked shadow/grime modulation on Oblivion interior architecture.
    /// Applied as `albedo.rgb *= dark_sample.rgb`. See #264.
    pub dark_map: Option<FixedString>,
    // NOTE: `decal_maps: Vec<String>` (NiTexturingProperty decal slots
    // 0..=3) was removed in #705 / O4-07. The walker extracted them
    // but no consumer in the renderer ever bound the descriptors or
    // surfaced an overlay loop in the fragment shader, so the import
    // cost was paid for a render-side no-op. The block parser still
    // exposes the raw slots on `NiTexturingProperty.decal_textures`;
    // re-extraction is a one-line addition when descriptor bindings +
    // shader overlay land. See discussion in the issue.
    /// Parallax / height texture (`BSShaderTextureSet` slot 3). FO3/FNV
    /// architecture relies on this for brick-wall / concrete
    /// parallax-occlusion mapping on `shader_type = 3` (Parallax_Shader_Index_15)
    /// and `shader_type = 7` (Parallax_Occlusion) PPLighting materials.
    /// Pre-#452 the importer stopped reading at slot 2, so every Pitt /
    /// Point Lookout / Hoover Dam parallax wall landed flat. See #452.
    pub parallax_map: Option<FixedString>,
    /// Environment cubemap (`BSShaderTextureSet` slot 4). Drives the
    /// glass bottle / power-armor / smooth-metal reflection branch.
    /// `env_map_scale` is already captured but had no texture route
    /// until #452.
    pub env_map: Option<FixedString>,
    /// Environment-reflection mask (`BSShaderTextureSet` slot 5). Per-
    /// texel attenuation of the `env_map` reflection — used on armor
    /// edges and rim highlights so only the polished surface reflects.
    /// See #452.
    pub env_mask: Option<FixedString>,
    /// FaceTint per-NPC tint overlay (`BSShaderTextureSet` slot 7).
    /// Drives the per-face NPC color overlay (warpaint, race tint,
    /// scars). Pre-#563 the importer never read slot 7, so FaceTint
    /// materials silently dropped the tint texture and the per-NPC
    /// `material_kind == 4` dispatch had nothing to sample. See
    /// nif.xml `BSLightingShaderType::FaceTint` ("Enables Detail(TS4),
    /// Tint(TS7)") and #563.
    pub tint_map: Option<FixedString>,
    /// MultiLayerParallax inner-layer texture (`BSShaderTextureSet`
    /// slot 7). Sampled beneath the diffuse layer for ice / glass /
    /// crystal surfaces — paired with `multi_layer_inner_thickness` /
    /// `multi_layer_inner_layer_scale`. Pre-#563 the importer never
    /// read slot 7, so Dragonborn DLC ice walls and modded glass
    /// shaders silently lost their inner layer. See nif.xml
    /// `BSLightingShaderType::MultiLayerParallax` ("Enables …
    /// Layer(TS7)") and #563.
    pub inner_layer_map: Option<FixedString>,
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
    /// True once a `NiAlphaProperty` has been consumed via
    /// [`apply_alpha_flags`] for this mesh — regardless of whether
    /// it set `alpha_blend` / `alpha_test`. A NiAlphaProperty with
    /// `flags=0` is structurally "the shape explicitly authored no
    /// blending and no test"; treating it as "no NiAlphaProperty was
    /// consumed" let the parent NiNode's NiAlphaProperty silently
    /// overwrite it on inherited-property walks. The cascade gate
    /// now reads this flag instead of `!alpha_blend && !alpha_test`
    /// so a flags=0 shape property correctly closes the gate. See
    /// NIF-D4-NEW-05 (audit 2026-05-12).
    pub alpha_property_consumed: bool,
    pub two_sided: bool,
    pub is_decal: bool,
    pub emissive_color: [f32; 3],
    pub emissive_mult: f32,
    /// Provenance of `emissive_mult` — disambiguates the three NIF
    /// shader-property classes whose "emissive multiplier" fields all
    /// flow into the same slot. See
    /// [`byroredux_core::ecs::components::material::EmissiveSource`].
    /// Set by the walker at each of the 3 set-sites; defaults to
    /// `EmissiveSource::None` when no source authored a value.
    /// #1280 step 4 / canonical material convergence.
    pub emissive_source: byroredux_core::ecs::components::material::EmissiveSource,
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
    /// `BSLightingShaderProperty.refraction_strength` — refractive
    /// distortion magnitude (every BSVER 83+ BSLSP authors this).
    /// Default 0.0 = no refraction. Renderer dispatch is deferred to
    /// the Skyrim+ PBR pass paired with the BGSM v>=8 suite (#1147);
    /// captured here so the silent drop at the importer boundary is
    /// closed. See #1241 / NIF-DIM4-NEW-01.
    pub refraction_strength: f32,
    /// `BSLightingShaderProperty.lighting_effect_1` — Skyrim subsurface
    /// scattering scalar (BSVER < FO4, gated by `SLSF2_Soft_Lighting`).
    /// Drives the per-material SSS rolloff on skin / soft-cloth / wax
    /// surfaces. Default 0.0 = no SSS contribution. See #1241.
    pub lighting_effect_1: f32,
    /// `BSLightingShaderProperty.lighting_effect_2` — Skyrim backlight
    /// scalar (BSVER < FO4, gated by `SLSF2_Back_Lighting`). Drives the
    /// back-lit translucency term on hair / foliage / fabric edges.
    /// Default 0.0 = no backlight. See #1241.
    pub lighting_effect_2: f32,
    /// `BSLightingShaderProperty.subsurface_rolloff` — FO4 BSVER 130–139.
    /// Per-material SSS rolloff envelope. Default 0.0. See #1241.
    pub subsurface_rolloff: f32,
    /// `BSLightingShaderProperty.rimlight_power` — FO4 BSVER 130–139.
    /// Per-material rim-light exponent (power-armor edge highlight,
    /// NPC skin rim). Default 0.0. See #1241.
    pub rimlight_power: f32,
    /// `BSLightingShaderProperty.backlight_power` — FO4 BSVER 130–139.
    /// Per-material backlight exponent (paired with `rimlight_power`
    /// when `< FLT_MAX`). Default 0.0. See #1241.
    pub backlight_power: f32,
    /// `BSLightingShaderProperty.grayscale_to_palette_scale` — FO4+
    /// BSVER >= 130. Modulator on the greyscale→palette LUT remap
    /// (NPC face tints, gradient-driven palette swaps). Default 1.0
    /// = no scale (matches the BSLSP parser stub default at
    /// `shader.rs:748`). See #1241.
    pub grayscale_to_palette_scale: f32,
    /// `BSLightingShaderProperty.fresnel_power` — FO4+ BSVER >= 130.
    /// Per-material Schlick exponent for the Fresnel rim term.
    /// Default 5.0 (standard Schlick exponent, matches the BSLSP
    /// parser stub default at `shader.rs:749`). See #1241.
    pub fresnel_power: f32,
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
    /// Forces wireframe rendering (polygon_mode = LINE). Set when a
    /// `NiWireframeProperty { flags: 1 }` block is present. Oblivion vanilla
    /// does not ship this; common in FO3/FNV mods. Renderer-side consumption
    /// (VK_POLYGON_MODE_LINE) is tracked separately.
    pub wireframe: bool,
    /// Forces flat shading (no per-vertex normal interpolation). Set when a
    /// `NiShadeProperty { flags: 0 }` block is present (bit 0 off = flat).
    /// Used on a handful of Oblivion architectural pieces to fake a faceted
    /// look. Renderer-side consumption (GLSL `flat` qualifier) is future work.
    pub flat_shading: bool,

    // ── BSLightingShaderProperty.shader_type dispatch (SK-D3-01) ────
    // Each variant of `ShaderTypeData` exposes different trailing
    // fields. Capturing them at import time lets the renderer later
    // branch on `material_kind` without re-reading the NIF. Renderer-
    // side dispatch is tracked separately (SK-D3-02); until that lands
    // these values ride unused on `MaterialInfo`.
    /// Raw `BSLightingShaderProperty.shader_type`. Values 0–20 are the
    /// vanilla Bethesda enum (Default, EnvironmentMap, GlowShader,
    /// Parallax, FaceTint, SkinTint, HairTint, ParallaxOcc,
    /// MultiLayerParallax, ..., Dismemberment); 100+ is engine-
    /// synthesized (MATERIAL_KIND_GLASS, MATERIAL_KIND_EFFECT_SHADER).
    /// 0 when the shape has no BSLightingShaderProperty (pre-Skyrim).
    /// Widened to `u32` per #570 (SK-D3-03) — the parser keeps
    /// `shader_type` as `u32` and the GPU `GpuMaterial.material_kind`
    /// is `u32`, so narrowing through this struct silently masked
    /// values ≥ 256. Future Starfield / FO4 DLC variants are now
    /// safe to plumb without a type-width regression.
    pub material_kind: u32,
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
    /// `NiStencilProperty` test/write state captured at import time.
    /// `None` when the material has no `NiStencilProperty`. The
    /// 95% two-sided-only case lands `Some(state)` with
    /// `state.enabled = false` — `draw_mode` (consumed via
    /// `is_two_sided()`) carries the backface-culling intent
    /// separately. Renderer-side stencil pipeline variants are
    /// deferred — see [`StencilState`] docs and #337.
    pub stencil_state: Option<StencilState>,
    /// Diffuse-slot `TexClampMode` (`0..=3`, see nif.xml's enum) —
    /// captured from `NiTexturingProperty.base_texture.flags` (Oblivion
    /// / FO3 / FNV statics) or `BSEffectShaderProperty.texture_clamp_mode`
    /// (Skyrim+ effects). Default `3 = WRAP_S_WRAP_T` (REPEAT/REPEAT)
    /// matches Gamebryo's pre-clamp default. Pre-#610 the field was
    /// dropped at every authoring site; the renderer hardcoded REPEAT
    /// and CLAMP-authored decals / scope reticles / skybox seams
    /// rendered with edge bleed. The `BsEffectShaderData` struct
    /// keeps its own `texture_clamp_mode` for back-compat (effect-
    /// shader-specific consumers route through that), but
    /// `extract_material_info` mirrors the value here so the
    /// importer's per-mesh export needs only one field.
    pub texture_clamp_mode: u8,
    /// Set when a Skyrim+ `BSSkyShaderProperty` is the authoritative
    /// shader for this material — sky-dome / sun-glare / moon / star
    /// meshes from `meshes/sky/*.nif`. Pre-#977 these meshes silently
    /// imported with `texture_path = None` and rendered as the magenta
    /// placeholder because the importer had no consumer for the type.
    /// The renderer-side dispatch is follow-up work (sky materials must
    /// bypass scene lighting and treat the diffuse sample as emissive);
    /// until then the flag rides through as a structural marker.
    pub is_sky_object: bool,
    /// Per nif.xml `SkyObjectType`: `0=Texture, 1=Sunglare, 2=Sky,
    /// 3=Clouds, 5=Stars, 7=Moon/Stars_Mask`. Selects which sky function
    /// this property fulfills at render time. Zero when [`is_sky_object`]
    /// is false. See `BSSkyShaderProperty.sky_object_type` in
    /// `crates/nif/src/blocks/shader.rs`.
    pub sky_object_type: u32,
    /// Captured from `BSWaterShaderProperty.water_shader_flags` per
    /// nif.xml `WaterShaderPropertyFlags`. Bit layout: `0=Specular,
    /// 1=Reflections, 2=Refractions, 3=Vertex_UV, 6=Reflections,
    /// 7=Refractions, 8=Vertex_UV, 9=Vertex_Alpha_Depth, 10=Procedural,
    /// 11=Fog, 12=Update_Constants, 13=Cubemap`. Zero when no
    /// `BSWaterShaderProperty` was bound. Renderer-side dispatch (#977
    /// follow-up) will let the M38 `WaterPipeline` honor authored
    /// reflection / refraction / cubemap intent on legacy mesh-driven
    /// water surfaces (Oblivion `meshes/water/*.nif`, Skyrim river
    /// segments). See `BSWaterShaderProperty` in
    /// `crates/nif/src/blocks/shader.rs`.
    pub water_shader_flags: u32,
}

/// Stencil-test state captured from `NiStencilProperty`. Mirrors the
/// seven non-`draw_mode` fields the parser reads (`draw_mode` is
/// consumed separately by `MaterialInfo::two_sided` via
/// `NiStencilProperty::is_two_sided()`).
///
/// **Renderer-side gap (#337):** every pipeline-create site at
/// `crates/renderer/src/vulkan/pipeline.rs` currently hardcodes
/// `stencil_test_enable(false)`, and `find_depth_format` prefers
/// `D32_SFLOAT` (no stencil bits) over the stencil-capable formats.
/// Wiring stencil pipeline variants therefore needs both a depth-
/// format flip (gated on a real workload — D32 has better precision
/// when no consumer needs stencil) AND per-`MaterialKind` pipeline
/// variants honouring the captured ops. Until that lands, the
/// captured state rides on `MaterialInfo` so the silent drop at the
/// importer boundary is closed and a future renderer-side fix has
/// one grep target. The `>95%`-of-vanilla `NiStencilProperty` usage
/// for two-sided rendering already works through the `is_two_sided()`
/// path in the walker (`draw_mode` 0 / 3); this struct covers the
/// long-tail stencil-masked decals, portals, and shadow volumes
/// that author the other fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StencilState {
    /// `true` when the stencil test is active for this material. The
    /// 95% two-sided-only case lands `false` here — `draw_mode` is
    /// what the importer consumes for backface culling.
    pub enabled: bool,
    /// Stencil compare function. Maps to Gamebryo's `TestFunction`
    /// enum (`0=NEVER, 1=LESS, 2=EQUAL, 3=LESSEQUAL, 4=GREATER,
    /// 5=NOTEQUAL, 6=GREATEREQUAL, 7=ALWAYS`). Default `7=ALWAYS`
    /// from the parser.
    pub function: u32,
    /// Reference value compared against `(stencil_value & mask)`.
    pub reference: u32,
    /// Read mask `AND`-ed with both the reference and the stored
    /// stencil value before comparison.
    pub mask: u32,
    /// Action when the stencil test fails. Maps to
    /// Gamebryo's `Action` enum (`0=KEEP, 1=ZERO, 2=REPLACE,
    /// 3=INCREMENT, 4=DECREMENT, 5=INVERT`).
    pub fail_action: u32,
    /// Action when the stencil test passes but the depth test fails.
    pub z_fail_action: u32,
    /// Action when both stencil and depth tests pass.
    pub pass_action: u32,
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
    /// `SLSF1::Soft_Effect` (bit 30) — near-camera depth feathering for
    /// soft particles (smoke, dust, force-field haze, Dwemer steam).
    /// Captured via the modern-shader flag + CRC32 fallback path so
    /// FO76 / Starfield content surfaces it through `sf1_crcs` / `sf2_crcs`.
    /// Stage 2 (separate issue) wires this into the fragment shader's
    /// `MATERIAL_KIND_EFFECT_SHADER` branch as a depth-attachment read
    /// + alpha fade. See #890 / SK-D4-NEW-04.
    pub effect_soft: bool,
    /// `SLSF1::Greyscale_To_PaletteColor` (bit 4) — sample
    /// [`greyscale_texture`] as a colour palette LUT indexed by the
    /// source-texture luminance instead of using the luminance directly.
    /// Fire / electricity / magic gradients drive this on Skyrim+ /
    /// FO4 spell FX. Stage 2 plumbs the palette sample into the
    /// effect-shader branch. See #890.
    pub effect_palette_color: bool,
    /// `SLSF1::Greyscale_To_PaletteAlpha` (bit 5) — same as
    /// [`effect_palette_color`] but for the alpha channel: the
    /// [`greyscale_texture`] alpha LUT modulates the surface alpha.
    /// See #890.
    pub effect_palette_alpha: bool,
    /// `SLSF2::Effect_Lighting` (bit 30) — scene-lit `BSEffectShaderProperty`
    /// surface. Pre-fix the renderer treats every effect shader as
    /// purely additive; with this bit set the surface should receive
    /// the cell's directional + ambient light modulated against
    /// `base_color × base_color_scale`. Stage 2 wires the lit shading.
    /// See #890.
    pub effect_lit: bool,
    /// FO76 reflectance texture (BSVER == 155). Drives the surface's
    /// metallic / reflectance lookup on FO76 emissive materials.
    /// Empty path collapses to `None`. Renderer-side consumption is
    /// follow-up work — capture-only today. See #1205.
    pub reflectance_texture: Option<String>,
    /// FO76 lighting texture (BSVER == 155). Per-material lighting
    /// LUT (similar role to FO76's `_l.dds` channels). Empty path
    /// collapses to `None`. See #1205.
    pub lighting_texture: Option<String>,
    /// FO76 emit gradient texture (BSVER == 155). Drives the
    /// per-fragment emittance gradient on glow planes / UI overlays.
    /// Empty path collapses to `None`. See #1205.
    pub emit_gradient_texture: Option<String>,
    /// FO76 emittance color `[r, g, b]` (BSVER == 155). Multiplied
    /// with the emit-gradient lookup; `None` when the parser saw the
    /// `[0, 0, 0]` Bethesda-default sentinel. See #1205.
    pub emittance_color: Option<[f32; 3]>,
    /// FO76 luminance parameters (BSVER == 155). Drives the
    /// HDR-emittance / exposure-clamp envelope for emissive surfaces.
    /// `None` on pre-FO76 content where the parser left the slot
    /// empty. See #1205.
    pub luminance: Option<LuminanceParams>,
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
            parallax_map: None,
            env_map: None,
            env_mask: None,
            tint_map: None,
            inner_layer_map: None,
            vertex_color_mode: VertexColorMode::AmbientDiffuse,
            alpha_blend: false,
            src_blend_mode: 6, // SRC_ALPHA — Gamebryo default
            dst_blend_mode: 7, // INV_SRC_ALPHA — Gamebryo default
            alpha_test: false,
            alpha_threshold: 0.0,
            alpha_test_func: 6, // GREATEREQUAL — Gamebryo default
            alpha_property_consumed: false,
            two_sided: false,
            is_decal: false,
            emissive_color: [0.0, 0.0, 0.0],
            emissive_mult: 0.0,
            emissive_source: byroredux_core::ecs::components::material::EmissiveSource::None,
            specular_color: [1.0, 1.0, 1.0],
            specular_strength: 1.0,
            diffuse_color: [1.0, 1.0, 1.0],
            specular_enabled: true,
            glossiness: 80.0,
            // BSLightingShaderProperty PBR scalars — defaults mirror
            // the parser stub at `crates/nif/src/blocks/shader.rs:739-749`
            // so the no-author fallback is the same as the stopcond
            // fallback. See #1241.
            refraction_strength: 0.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale: 1.0,
            fresnel_power: 5.0,
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
            wireframe: false,
            flat_shading: false,

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
            stencil_state: None,
            // 3 = WRAP_S_WRAP_T per nif.xml — pre-#610 hardcoded
            // default. Walker overwrites with the diffuse-slot value
            // when the material's authoring source carries one.
            texture_clamp_mode: 3,
            is_sky_object: false,
            sky_object_type: 0,
            water_shader_flags: 0,
        }
    }
}

impl MaterialInfo {
    /// Stage 2 (`feedback_format_translation.md`) — derive PBR
    /// `(metalness, roughness)` from this MaterialInfo's legacy
    /// inline-shader data at NIF-import time, so every mesh leaves
    /// the parser with explicit `metalness_override` /
    /// `roughness_override` populated. The BGSM merge layer
    /// downstream (`asset_provider::merge_bgsm_into_mesh`)
    /// overwrites both unconditionally for BGSM-resolved materials,
    /// so legacy Oblivion / FO3 / FNV / pre-Skyrim meshes keep the
    /// values this method writes; BGSM-using FO4 / Skyrim meshes get
    /// the authored spec-glossiness translation instead.
    ///
    /// Single source of truth — delegates to
    /// `byroredux_core::ecs::components::material::classify_pbr_keyword`
    /// so the per-frame draw build's
    /// `Material::classify_pbr` and this importer-side translation
    /// stay in lockstep.
    pub(super) fn classify_legacy_pbr(
        &self,
        pool: &byroredux_core::string::StringPool,
    ) -> byroredux_core::ecs::components::material::PbrMaterial {
        let texture_path_str = self.texture_path.and_then(|s| pool.resolve(s));
        byroredux_core::ecs::components::material::classify_pbr_keyword(
            byroredux_core::ecs::components::material::PbrClassifierInputs {
                texture_path: texture_path_str,
                glossiness: self.glossiness,
                env_map_scale: self.env_map_scale,
                has_normal_map: self.normal_map.is_some(),
                specular_color: self.specular_color,
                has_gloss_map: self.gloss_map.is_some(),
            },
        )
    }

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
    // Mark the property as consumed regardless of whether either bit
    // fired. A flags=0 NiAlphaProperty is still a valid statement of
    // intent ("this shape authored no blending"); the cascade gate
    // checks this flag to avoid letting a parent NiNode's
    // NiAlphaProperty silently overwrite the shape's choice. See
    // NIF-D4-NEW-05 (audit 2026-05-12).
    info.alpha_property_consumed = true;
}

/// Resolve a `TexDesc` slot on an `NiTexturingProperty` to a texture
/// filename by following its `source_ref` through the scene's block
/// table and pulling the filename from the referenced
/// `NiSourceTexture`. Returns `None` if the slot is empty, the ref
/// is null, or the source texture has no external filename (embedded
/// NiPixelData is not supported here — the downstream texture
/// provider can't resolve those anyway). See issue #131.
///
/// The path is interned through the engine's [`StringPool`] (#609 /
/// D6-NEW-01). Pre-fix this returned `Option<String>` via `to_string`
/// on an `Arc<str>`; same path mentioned by N meshes in a cell paid
/// N allocations even though the source was already shared.
pub(super) fn tex_desc_source_path(
    scene: &NifScene,
    desc: Option<&TexDesc>,
    pool: &mut StringPool,
) -> Option<FixedString> {
    let desc = desc?;
    let src_idx = desc.source_ref.index()?;
    let src_tex = scene.get_as::<NiSourceTexture>(src_idx)?;
    let name = src_tex.filename.as_ref()?;
    intern_texture_path(pool, name)
}

#[cfg(test)]
mod alpha_flag_tests;

/// Stage 2 (`feedback_format_translation.md`) regression — every
/// legacy NIF mesh must leave the parser with explicit
/// `metalness_override` / `roughness_override` derived from the
/// shared `classify_pbr_keyword`. The classifier itself is tested in
/// `byroredux-core::ecs::components::material::tests`; this sibling
/// pins the MaterialInfo → classifier adapter.
#[cfg(test)]
mod legacy_pbr_translation_tests;

/// Regression tests for issue #345 — `BSEffectShaderProperty` rich
/// material fields used to be dropped on import. The capture path is
/// covered by direct `capture_effect_shader_data` tests; full
/// `extract_material_info` coverage requires a synthetic NIF and is
/// blocked on test infrastructure (`NifScene` doesn't expose enough
/// mutators to wire one up cheaply). The capture helper is the entire
/// transform under test — `extract_material_info` just calls it.
#[cfg(test)]
mod effect_shader_capture_tests;

/// #1280 step 4 — pin the EmissiveSource discriminator at each of the
/// three NIF shader-property set-sites (BSLightingShaderProperty →
/// Lighting, BSEffectShaderProperty → Effect, NiMaterialProperty →
/// Material). Critical for keeping BSEffect's diffuse-tint-as-emissive
/// conflation type-visible.
#[cfg(test)]
mod emissive_source_tests;

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

/// Regression tests for #337 (D4-NEW-01) — `NiStencilProperty` test
/// state must round-trip into `MaterialInfo.stencil_state` so the
/// silent drop at the importer boundary is closed. Pre-fix the walker
/// only consumed `is_two_sided()`; the seven non-`draw_mode` fields
/// were dropped on the floor. Renderer-side stencil pipeline variants
/// stay deferred (depth-format dependency + per-`MaterialKind` variant
/// rollout); see `pipeline.rs`'s cross-reference comments.
#[cfg(test)]
mod stencil_state_capture_tests;

/// Regression tests for #977 — Skyrim+ `BSSkyShaderProperty` and
/// `BSWaterShaderProperty` were parsed cleanly but had no
/// `scene.get_as::<>` consumer in the walker, so every Skyrim sky NIF
/// imported with `texture_path = None` and rendered as the magenta
/// placeholder. The FO3/FNV `SkyShaderProperty` counterpart was wired
/// by #940; this closes the missing Skyrim-era sibling.
#[cfg(test)]
mod sky_water_shader_tests;

/// Regression tests for #976 (NIF-D4-NEW-02) — `BSLightingShaderProperty`
/// with a `.mat` (Starfield JSON material) or trailing-whitespace name
/// must populate `material_path` via the shared `material_path_from_name`
/// helper. Pre-fix the walker used an inline `.bgsm`/`.bgem` suffix check
/// that dropped all `.mat` references.
#[cfg(test)]
mod lighting_shader_mat_tests;

/// Regression tests for #1208 (NIF-D4-NEW-08) — inherited
/// `NiVertexColorProperty` must not overwrite the Skyrim+ shader-driven
/// `vertex_color_mode` when `BSLightingShaderProperty` has already
/// populated `has_material_data`.
#[cfg(test)]
mod vertex_color_precedence_tests;

/// Regression tests for #1243 (NIF-DIM4-NEW-02) + #1244 (NIF-DIM4-NEW-03)
/// — FO3/FNV legacy non-BS shader subclasses must reach `MaterialInfo`.
/// Covers `WaterShaderProperty` (#1243) and the four
/// `BSShaderPropertyBaseOnly` subclasses (#1244: Hair / VolumetricFog /
/// DistantLOD / DistantTree). Sibling to the Skyrim+ `BSWaterShaderProperty`
/// consumer covered by `sky_water_shader_tests` (#977 closure).
#[cfg(test)]
mod water_shader_legacy_tests;

/// Regression tests for #1241 (NIF-DIM4-NEW-01) —
/// `BSLightingShaderProperty` PBR scalars (`refraction_strength`,
/// `lighting_effect_1/2`, `subsurface_rolloff`, `rimlight_power`,
/// `backlight_power`, `grayscale_to_palette_scale`, `fresnel_power`)
/// must land on `MaterialInfo` and propagate through every mesh
/// extractor into `ImportedMesh`. Pre-fix the walker captured the
/// `emissive_*` / `glossiness` / `uv_*` / `alpha` neighbours but
/// dropped these 7, so the parser's per-BSVER capture investment
/// (#1175, #115, #403) never surfaced.
#[cfg(test)]
mod lighting_shader_pbr_tests;

#[cfg(test)]
mod intern_texture_path_tests {
    use super::intern_texture_path;
    use byroredux_core::string::StringPool;

    /// Regression test for FO3-1-01 (#1541): empty *and whitespace-only*
    /// slot strings must collapse to `None` so the diffuse slot falls back
    /// to the neutral texture instead of binding the magenta checker.
    #[test]
    fn whitespace_only_paths_collapse_to_none() {
        let mut pool = StringPool::new();
        assert_eq!(intern_texture_path(&mut pool, ""), None);
        assert_eq!(intern_texture_path(&mut pool, " "), None);
        assert_eq!(intern_texture_path(&mut pool, "\t"), None);
        assert_eq!(intern_texture_path(&mut pool, "  \t \n "), None);
        // A real path is still interned.
        assert!(intern_texture_path(&mut pool, "textures/foo.dds").is_some());
    }
}
