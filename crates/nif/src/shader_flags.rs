//! Named `BSShader*` flag constants for the two game-era flag
//! vocabularies Gamebryo / Creation Engine uses.
//!
//! There are three flag pairs in the wild:
//!
//!   - **FO3 / FNV** — `BSShaderFlags` (F1) + `BSShaderFlags2` (F2), used by
//!     `BSShaderPPLightingProperty` and `BSShaderNoLightingProperty`.
//!   - **Skyrim LE / SE** — `SkyrimShaderPropertyFlags1` (SLSF1) +
//!     `SkyrimShaderPropertyFlags2` (SLSF2), used by
//!     `BSLightingShaderProperty` and `BSEffectShaderProperty`.
//!   - **FO4+** — `Fallout4ShaderPropertyFlags1/2` (F4SF1/F4SF2), again on
//!     `BSLightingShaderProperty` / `BSEffectShaderProperty`. Captured here
//!     only where the semantic diverges from Skyrim; most bits overlap.
//!
//! **Bits that happen to share the same numeric value do not share the
//! same semantic across games.** The comment on each constant pins which
//! game carries which meaning. Sites that test a bit need to know which
//! game's property they are holding — `BSShaderPPLightingProperty` always
//! implies FO3/FNV semantics; `BSLightingShaderProperty` implies Skyrim+
//! (with FO4 overrides for specific bits). Future #437 work should wrap
//! these into a `GameVariant`-keyed lookup so a single mesh can't pick
//! the wrong vocabulary.
//!
//! References: nif.xml `BSShaderFlags` / `BSShaderFlags2` and
//! `SkyrimShaderPropertyFlags1/2`.

/// FO3/FNV `BSShaderFlags` — the first flag word of
/// `BSShaderPPLightingProperty` / `BSShaderNoLightingProperty`.
///
/// Most bit semantics overlap with SLSF1; the noteworthy exception is
/// bit 12 which is `Unknown_3` on FO3/FNV and `Model_Space_Normals` on
/// Skyrim. Do NOT re-use FO3/FNV bit 12 expecting Skyrim behavior.
pub mod fo3nv_f1 {
    /// Bit 26 — `Decal`. Render on top of coplanar surfaces.
    pub const DECAL: u32 = 0x0400_0000;
    /// Bit 27 — `Dynamic_Decal`. Runtime-spawned decal (blood splat).
    pub const DYNAMIC_DECAL: u32 = 0x0800_0000;
    /// Bit 31 — `ZBuffer_Test`.
    pub const ZBUFFER_TEST: u32 = 0x8000_0000;
}

/// FO3/FNV `BSShaderFlags2` — second flag word.
pub mod fo3nv_f2 {
    /// Bit 21 — `Alpha_Decal`. FO3/FNV-only extension that flags a
    /// mesh as alpha-blended decal (blood splat / gore).
    /// **Warning**: bit 21 on Skyrim SLSF2 is `Cloud_LOD`, NOT decal —
    /// do NOT test this constant on a `BSLightingShaderProperty`.
    pub const ALPHA_DECAL: u32 = 0x0020_0000;
}

/// Skyrim `SkyrimShaderPropertyFlags1` — first flag word of
/// `BSLightingShaderProperty` / `BSEffectShaderProperty`.
pub mod skyrim_slsf1 {
    /// Bit 26 — `Decal`. Same bit + semantic as FO3/FNV F1.
    pub const DECAL: u32 = 0x0400_0000;
    /// Bit 27 — `Dynamic_Decal`. Same bit + semantic as FO3/FNV F1.
    pub const DYNAMIC_DECAL: u32 = 0x0800_0000;
    /// Bit 31 — `ZBuffer_Test`.
    pub const ZBUFFER_TEST: u32 = 0x8000_0000;
}

/// Skyrim `SkyrimShaderPropertyFlags2` — second flag word.
///
/// **No dedicated Decal bit** — Skyrim meshes only carry decal flags
/// on SLSF1. See issue #176 for the closed-as-stale analysis of that
/// mismatch.
pub mod skyrim_slsf2 {
    /// Bit 4 — `Double_Sided`. Back-face rendering enabled.
    /// Only meaningful on Skyrim+ properties. FO3/FNV has no
    /// equivalent F2 bit; those games route back-face via
    /// `NiStencilProperty`. See #441.
    pub const DOUBLE_SIDED: u32 = 0x0000_0010;
    /// Bit 17 — `Weapon_Blood`. Blood decals on weapons specifically.
    pub const WEAPON_BLOOD: u32 = 0x0002_0000;
    /// Bit 21 — `Cloud_LOD` on Skyrim (NOT `Alpha_Decal` — that
    /// is FO3/FNV-only).
    pub const CLOUD_LOD: u32 = 0x0020_0000;
}

/// FO4+ `Fallout4ShaderPropertyFlags1` — first flag word of
/// `BSLightingShaderProperty` / `BSEffectShaderProperty` on BSVER >= 130
/// per nif.xml `#BS_FO4#` gate.
///
/// Bits 26 (Decal), 27 (Dynamic_Decal), 31 (ZBuffer_Test) align with
/// Skyrim SLSF1 — same numeric value, same semantic. Most other bits
/// diverge from Skyrim; pin the FO4 positions here so decal /
/// two-sided / alpha-test testing doesn't silently drift when a FO4
/// mesh happens to be read through the Skyrim vocabulary (or vice
/// versa). Sourced from nif.xml `Fallout4ShaderPropertyFlags1`
/// (lines 6443-6477). See #414 / FO4-D3-M1.
pub mod fo4_slsf1 {
    pub const SPECULAR: u32 = 0x0000_0001;
    pub const SKINNED: u32 = 0x0000_0002;
    pub const TEMP_REFRACTION: u32 = 0x0000_0004;
    pub const VERTEX_ALPHA: u32 = 0x0000_0008;
    pub const GREYSCALE_TO_PALETTE_COLOR: u32 = 0x0000_0010;
    pub const GREYSCALE_TO_PALETTE_ALPHA: u32 = 0x0000_0020;
    pub const USE_FALLOFF: u32 = 0x0000_0040;
    pub const ENVIRONMENT_MAPPING: u32 = 0x0000_0080;
    pub const RGB_FALLOFF: u32 = 0x0000_0100;
    pub const CAST_SHADOWS: u32 = 0x0000_0200;
    pub const FACE: u32 = 0x0000_0400;
    pub const UI_MASK_RECTS: u32 = 0x0000_0800;
    /// Bit 12 — `Model_Space_Normals`. Matches the Skyrim bit layout;
    /// the collision with FO3/FNV `Unknown_3` (also bit 12) is why
    /// callers must know which property type they hold.
    pub const MODEL_SPACE_NORMALS: u32 = 0x0000_1000;
    pub const NON_PROJECTIVE_SHADOWS: u32 = 0x0000_2000;
    pub const LANDSCAPE: u32 = 0x0000_4000;
    pub const REFRACTION: u32 = 0x0000_8000;
    pub const FIRE_REFRACTION: u32 = 0x0001_0000;
    pub const EYE_ENVIRONMENT_MAPPING: u32 = 0x0002_0000;
    pub const HAIR: u32 = 0x0004_0000;
    pub const SCREENDOOR_ALPHA_FADE: u32 = 0x0008_0000;
    pub const LOCALMAP_HIDE_SECRET: u32 = 0x0010_0000;
    pub const SKIN_TINT: u32 = 0x0020_0000;
    pub const OWN_EMIT: u32 = 0x0040_0000;
    pub const PROJECTED_UV: u32 = 0x0080_0000;
    pub const MULTIPLE_TEXTURES: u32 = 0x0100_0000;
    pub const TESSELLATE: u32 = 0x0200_0000;
    /// Bit 26 — shared with SLSF1 / FO3-FNV F1. Decal geometry.
    pub const DECAL: u32 = 0x0400_0000;
    /// Bit 27 — shared with SLSF1 / FO3-FNV F1. Runtime-spawned decal.
    pub const DYNAMIC_DECAL: u32 = 0x0800_0000;
    pub const CHARACTER_LIGHTING: u32 = 0x1000_0000;
    pub const EXTERNAL_EMITTANCE: u32 = 0x2000_0000;
    pub const SOFT_EFFECT: u32 = 0x4000_0000;
    pub const ZBUFFER_TEST: u32 = 0x8000_0000;
}

/// FO4+ `Fallout4ShaderPropertyFlags2` — second flag word.
///
/// **The FO4 F2 layout diverges sharply from Skyrim SLSF2**:
/// - Bit 6 is `Glow_Map` on FO4 (Skyrim doesn't have an SLSF2 glow bit)
/// - Bit 15 is `Dismemberment` on FO4
/// - Bit 21 is `Anisotropic_Lighting` on FO4 (Skyrim: `Cloud_LOD`,
///   FO3/FNV F2: `Alpha_Decal` — **three different semantics on the
///   same bit across games**)
/// - Bit 24 is `Multi_Layer_Parallax` on FO4
/// - Bit 25 is `Alpha_Test` on FO4 (Skyrim has Alpha_Test on SLSF1!)
///
/// Sourced from nif.xml `Fallout4ShaderPropertyFlags2`
/// (lines 6479-6513). See #414 / FO4-D3-M1.
pub mod fo4_slsf2 {
    pub const ZBUFFER_WRITE: u32 = 0x0000_0001;
    pub const LOD_LANDSCAPE: u32 = 0x0000_0002;
    pub const LOD_OBJECTS: u32 = 0x0000_0004;
    pub const NO_FADE: u32 = 0x0000_0008;
    /// Bit 4 — `Double_Sided`. Same bit as Skyrim SLSF2.
    pub const DOUBLE_SIDED: u32 = 0x0000_0010;
    pub const VERTEX_COLORS: u32 = 0x0000_0020;
    /// Bit 6 — `Glow_Map`. FO4-specific — Skyrim's glow signal is the
    /// texture-set slot-2 presence, not a flag bit.
    pub const GLOW_MAP: u32 = 0x0000_0040;
    pub const TRANSFORM_CHANGED: u32 = 0x0000_0080;
    pub const DISMEMBERMENT_MEATCUFF: u32 = 0x0000_0100;
    pub const TINT: u32 = 0x0000_0200;
    pub const GRASS_VERTEX_LIGHTING: u32 = 0x0000_0400;
    pub const GRASS_UNIFORM_SCALE: u32 = 0x0000_0800;
    pub const GRASS_FIT_SLOPE: u32 = 0x0000_1000;
    pub const GRASS_BILLBOARD: u32 = 0x0000_2000;
    pub const NO_LOD_LAND_BLEND: u32 = 0x0000_4000;
    pub const DISMEMBERMENT: u32 = 0x0000_8000;
    pub const WIREFRAME: u32 = 0x0001_0000;
    pub const WEAPON_BLOOD: u32 = 0x0002_0000;
    pub const HIDE_ON_LOCAL_MAP: u32 = 0x0004_0000;
    pub const PREMULT_ALPHA: u32 = 0x0008_0000;
    pub const VATS_TARGET: u32 = 0x0010_0000;
    /// Bit 21 — `Anisotropic_Lighting` on FO4. Distinct from
    /// `Cloud_LOD` (Skyrim) and `Alpha_Decal` (FO3/FNV) at the same
    /// numeric value. The reason a legacy `is_decal_from_shader_flags`
    /// that tests `flags2 & 0x0020_0000` MUST NOT run on FO4 properties.
    pub const ANISOTROPIC_LIGHTING: u32 = 0x0020_0000;
    pub const SKEW_SPECULAR_ALPHA: u32 = 0x0040_0000;
    pub const MENU_SCREEN: u32 = 0x0080_0000;
    pub const MULTI_LAYER_PARALLAX: u32 = 0x0100_0000;
    /// Bit 25 — `Alpha_Test` on FO4. Skyrim routes alpha-test via
    /// `NiAlphaProperty` on a sibling block, not a shader flag bit.
    pub const ALPHA_TEST: u32 = 0x0200_0000;
    pub const GRADIENT_REMAP: u32 = 0x0400_0000;
    pub const VATS_TARGET_DRAW_ALL: u32 = 0x0800_0000;
    pub const PIPBOY_SCREEN: u32 = 0x1000_0000;
    pub const TREE_ANIM: u32 = 0x2000_0000;
    pub const EFFECT_LIGHTING: u32 = 0x4000_0000;
    pub const REFRACTION_WRITES_DEPTH: u32 = 0x8000_0000;
}

/// FO76 / Starfield CRC32-hashed shader flag identifiers.
///
/// For `BSVER >= 132` the wire format replaces the `(u32, u32)` flag pair
/// with two CRC32-tagged arrays (`SF1`/`SF2`). nif.xml defines
/// `BSShaderCRC32` (lines 6520–6553) with the canonical u32 values for
/// every recognised flag — sourcing the constants here directly from
/// the spec means we don't need to compute CRC32 at runtime, and the
/// values are pinned against any future drift.
///
/// The split between `SF1` and `SF2` arrays is purely a wire detail —
/// the same `BSShaderCRC32` enum populates both, so consumers can search
/// the union of both arrays for a target flag and stay correct.
///
/// Used by `BSLightingShaderProperty` / `BSEffectShaderProperty` /
/// `BSWaterShaderProperty` / `BSSkyShaderProperty` per nif.xml lines
/// 6590, 6647, 6701, 6714. Pre-#712 these values were parsed but no
/// importer call site read them, so every FO76+ decal / two-sided /
/// own-emit flag was silently dropped.
pub mod bs_shader_crc32 {
    /// `Decal` — bit-26 single-pass decal on legacy SLSF1.
    pub const DECAL: u32 = 3849131744;
    /// `Dynamic_Decal` — bit-27 runtime-spawned decal on legacy SLSF1.
    pub const DYNAMIC_DECAL: u32 = 1576614759;
    /// `Two_Sided` — replaces SLSF2 bit-4 `Double_Sided` at the CRC
    /// layer. nif.xml uses `TWO_SIDED` (the underscored spelling) here;
    /// the legacy bit was named `Double_Sided`. Same render semantic.
    pub const TWO_SIDED: u32 = 759557230;
    /// `Cast_Shadows` — Cast Shadows.
    pub const CAST_SHADOWS: u32 = 1563274220;
    /// `ZBuffer_Test` — Depth test enable.
    pub const ZBUFFER_TEST: u32 = 1740048692;
    /// `ZBuffer_Write` — Depth write enable.
    pub const ZBUFFER_WRITE: u32 = 3166356979;
    /// `Vertex_Colors` — Vertex-color modulation.
    pub const VERTEX_COLORS: u32 = 348504749;
    /// `PBR` — PBR pipeline path.
    pub const PBR: u32 = 731263983;
    /// `Skinned` — Skinned mesh.
    pub const SKINNED: u32 = 3744563888;
    /// `EnvMap` — Environment-map enable. (nif.xml: `ENVMAP`.)
    pub const ENVMAP: u32 = 2893749418;
    /// `Vertex_Alpha` — Vertex-alpha modulation.
    pub const VERTEX_ALPHA: u32 = 2333069810;
    /// `Face` — Face / FaceGen path.
    pub const FACE: u32 = 314919375;
    /// `Greyscale_To_Palette_Color` — palette-mapped colour.
    pub const GRAYSCALE_TO_PALETTE_COLOR: u32 = 442246519;
    /// `Hairtint` — hair-tint path.
    pub const HAIRTINT: u32 = 1264105798;
    /// `Skin_Tint` — skin-tint path.
    pub const SKIN_TINT: u32 = 1483897208;
    /// `Emit_Enabled` — Bethesda's CRC-era replacement for
    /// `Own_Emit` (legacy SLSF1 bit 22).
    pub const EMIT_ENABLED: u32 = 2262553490;
    /// `Glowmap` — glow-map slot routing.
    pub const GLOWMAP: u32 = 2399422528;
    /// `Refraction`.
    pub const REFRACTION: u32 = 1957349758;
    /// `Refraction_Falloff`.
    pub const REFRACTION_FALLOFF: u32 = 902349195;
    /// `NoFade`.
    pub const NOFADE: u32 = 2994043788;
    /// `Inverted_Fade_Pattern`.
    pub const INVERTED_FADE_PATTERN: u32 = 3030867718;
    /// `RGB_Falloff`.
    pub const RGB_FALLOFF: u32 = 3448946507;
    /// `External_Emittance`.
    pub const EXTERNAL_EMITTANCE: u32 = 2150459555;
    /// `ModelSpaceNormals`.
    pub const MODELSPACENORMALS: u32 = 2548465567;
    /// `Transform_Changed`.
    pub const TRANSFORM_CHANGED: u32 = 3196772338;
    /// `Effect_Lighting`.
    pub const EFFECT_LIGHTING: u32 = 3473438218;
    /// `Falloff`.
    pub const FALLOFF: u32 = 3980660124;
    /// `Soft_Effect`.
    pub const SOFT_EFFECT: u32 = 3503164976;
    /// `Greyscale_To_Palette_Alpha` — palette-mapped alpha.
    pub const GRAYSCALE_TO_PALETTE_ALPHA: u32 = 2901038324;
    /// `Weapon_Blood` — weapon blood decals.
    pub const WEAPON_BLOOD: u32 = 2078326675;
    /// `LOD_Objects` — LOD object render path.
    pub const LOD_OBJECTS: u32 = 2896726515;
    /// `No_Exposure` — opt-out of auto-exposure (Starfield).
    pub const NO_EXPOSURE: u32 = 3707406987;

    /// `true` when any of the supplied CRC32 flag identifiers is in
    /// `crcs`. Used by the importer to test SF1+SF2 union.
    #[inline]
    pub fn contains_any(crcs: &[u32], targets: &[u32]) -> bool {
        crcs.iter().any(|c| targets.contains(c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fo3nv_and_skyrim_decal_bits_agree() {
        // Both games route decals through SLSF1 bits 26/27 with
        // identical numeric values. Pinning the shared layout here
        // ensures a refactor can't silently drift one side.
        assert_eq!(fo3nv_f1::DECAL, skyrim_slsf1::DECAL);
        assert_eq!(fo3nv_f1::DYNAMIC_DECAL, skyrim_slsf1::DYNAMIC_DECAL);
    }

    #[test]
    fn fo3nv_f2_alpha_decal_and_skyrim_f2_cloud_lod_collide() {
        // Two different semantics on the same bit across games — the
        // collision is exactly why callers must know which property
        // type they're holding. `is_decal_from_shader_flags` in the
        // import pipeline guards this by only being called on FO3/FNV
        // `BSShader*Property` paths.
        assert_eq!(fo3nv_f2::ALPHA_DECAL, skyrim_slsf2::CLOUD_LOD);
    }

    #[test]
    fn slsf2_double_sided_matches_nif_xml() {
        // nif.xml SkyrimShaderPropertyFlags2 bit 4 = Double_Sided.
        assert_eq!(skyrim_slsf2::DOUBLE_SIDED, 1u32 << 4);
    }

    /// #414 — FO4 shares SLSF1 bits 26/27 with Skyrim + FO3/FNV for
    /// Decal / Dynamic_Decal. Same numeric value, same semantic —
    /// pin the cross-game agreement so the shared decal helper can
    /// keep testing those bits on every game-era modern property.
    #[test]
    fn fo4_shares_slsf1_decal_bits_with_skyrim_and_legacy() {
        assert_eq!(fo4_slsf1::DECAL, skyrim_slsf1::DECAL);
        assert_eq!(fo4_slsf1::DECAL, fo3nv_f1::DECAL);
        assert_eq!(fo4_slsf1::DYNAMIC_DECAL, skyrim_slsf1::DYNAMIC_DECAL);
        assert_eq!(fo4_slsf1::DYNAMIC_DECAL, fo3nv_f1::DYNAMIC_DECAL);
    }

    /// #414 — THREE different semantics on bit 21 of the second flag
    /// word across games. A legacy decal helper that tests this bit on
    /// a Skyrim+ / FO4 property reads an unrelated render-path flag and
    /// misclassifies the mesh as a decal.
    #[test]
    fn f2_bit_21_has_three_distinct_semantics_across_games() {
        assert_eq!(fo3nv_f2::ALPHA_DECAL, 0x0020_0000);
        assert_eq!(skyrim_slsf2::CLOUD_LOD, 0x0020_0000);
        assert_eq!(fo4_slsf2::ANISOTROPIC_LIGHTING, 0x0020_0000);
    }

    /// #414 — Double_Sided lives at the same bit on Skyrim and FO4 F2.
    #[test]
    fn fo4_shares_double_sided_bit_with_skyrim() {
        assert_eq!(fo4_slsf2::DOUBLE_SIDED, skyrim_slsf2::DOUBLE_SIDED);
    }

    /// #712 — pin the `BSShaderCRC32` constants against the literal
    /// `value="..."` integers in nif.xml lines 6520-6553. The exact
    /// algorithm Bethesda uses to derive these from flag-name strings
    /// is opaque (probing with standard CRC-32/IEEE 802.3 over various
    /// case-/prefix-/separator-permutations of the names produces no
    /// match — it's not the documented IEEE polynomial). What matters
    /// for correctness is that the values match the wire literals
    /// Bethesda's tools emit, and nif.xml is the authority.
    ///
    /// This test exists so a future edit copy-pasting a wrong digit
    /// fails immediately with a clear message instead of silently
    /// dropping decal/two-sided detection on a subset of Starfield
    /// content.
    #[test]
    fn bs_shader_crc32_matches_nif_xml_literals() {
        // (constant, nif.xml line, literal value).
        for (name, line, expected) in [
            ("DECAL", 6532, 3849131744),
            ("DYNAMIC_DECAL", 6533, 1576614759),
            ("TWO_SIDED", 6524, 759557230),
            ("CAST_SHADOWS", 6521, 1563274220),
            ("ZBUFFER_TEST", 6522, 1740048692),
            ("ZBUFFER_WRITE", 6523, 3166356979),
            ("VERTEX_COLORS", 6525, 348504749),
            ("SKINNED", 6527, 3744563888),
            ("EMIT_ENABLED", 6536, 2262553490),
            ("EXTERNAL_EMITTANCE", 6543, 2150459555),
        ] {
            let actual = match name {
                "DECAL" => bs_shader_crc32::DECAL,
                "DYNAMIC_DECAL" => bs_shader_crc32::DYNAMIC_DECAL,
                "TWO_SIDED" => bs_shader_crc32::TWO_SIDED,
                "CAST_SHADOWS" => bs_shader_crc32::CAST_SHADOWS,
                "ZBUFFER_TEST" => bs_shader_crc32::ZBUFFER_TEST,
                "ZBUFFER_WRITE" => bs_shader_crc32::ZBUFFER_WRITE,
                "VERTEX_COLORS" => bs_shader_crc32::VERTEX_COLORS,
                "SKINNED" => bs_shader_crc32::SKINNED,
                "EMIT_ENABLED" => bs_shader_crc32::EMIT_ENABLED,
                "EXTERNAL_EMITTANCE" => bs_shader_crc32::EXTERNAL_EMITTANCE,
                _ => unreachable!(),
            };
            assert_eq!(
                actual, expected,
                "{name} constant must match nif.xml line {line} literal {expected}",
            );
        }
    }

    #[test]
    fn bs_shader_crc32_contains_any_finds_target() {
        let crcs = [
            bs_shader_crc32::SKINNED,
            bs_shader_crc32::DECAL,
            bs_shader_crc32::CAST_SHADOWS,
        ];
        assert!(bs_shader_crc32::contains_any(
            &crcs,
            &[bs_shader_crc32::DECAL]
        ));
        assert!(bs_shader_crc32::contains_any(
            &crcs,
            &[bs_shader_crc32::DYNAMIC_DECAL, bs_shader_crc32::DECAL]
        ));
        assert!(!bs_shader_crc32::contains_any(
            &crcs,
            &[bs_shader_crc32::TWO_SIDED]
        ));
        assert!(!bs_shader_crc32::contains_any(&[], &[bs_shader_crc32::DECAL]));
    }
}
