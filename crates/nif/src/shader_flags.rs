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
}
