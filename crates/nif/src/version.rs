//! NIF file format version handling.
//!
//! Gamebryo encodes the version as a packed u32: major.minor.patch.build
//! where each component gets 8 bits (except major which is sometimes larger).
//! The actual encoding is: (major << 24) | (minor << 16) | (patch << 8) | build.

use std::fmt;

/// NIF file format version, packed as a u32.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NifVersion(pub u32);

impl NifVersion {
    // Well-known versions
    /// Morrowind era
    pub const V4_0_0_2: Self = Self(0x04000002);
    /// Oblivion / Fallout 3
    pub const V20_0_0_5: Self = Self(0x14000005);
    /// Oblivion (common)
    pub const V20_2_0_7: Self = Self(0x14020007);
    /// Skyrim / Fallout 4
    pub const V20_2_0_7_SSE: Self = Self(0x14020007);

    pub fn major(self) -> u8 {
        (self.0 >> 24) as u8
    }
    pub fn minor(self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub fn patch(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub fn build(self) -> u8 {
        self.0 as u8
    }
}

impl fmt::Display for NifVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.major(),
            self.minor(),
            self.patch(),
            self.build()
        )
    }
}

/// Which game generation produced this NIF.
///
/// Derived once from the header's (version, user_version, user_version_2) triplet.
/// Block parsers query semantic feature flags on this enum instead of comparing
/// raw version numbers, keeping game-specific quirks in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NifVariant {
    /// Morrowind (NIF ≤ 4.x, NetImmerse era)
    Morrowind,
    /// Oblivion (NIF 20.0.0.5, user_version < 11)
    Oblivion,
    /// Fallout 3 / Fallout New Vegas (NIF 20.2.0.7, uv=11, uv2≤34)
    Fallout3NV,
    /// Skyrim LE (NIF 20.2.0.7, uv=12, uv2=83)
    SkyrimLE,
    /// Skyrim SE (NIF 20.2.0.7, uv=12, uv2=100)
    SkyrimSE,
    /// Fallout 4 (NIF 20.2.0.7, uv=12, uv2=130)
    Fallout4,
    /// Fallout 76 (NIF 20.2.0.7, uv=12, uv2=155)
    Fallout76,
    /// Starfield (NIF 20.2.0.7, uv=12, uv2≥170)
    Starfield,
    /// Unknown version — parse with best effort.
    Unknown,
}

impl NifVariant {
    /// Determine the game variant from the NIF header version triplet.
    pub fn detect(version: NifVersion, user_version: u32, user_version_2: u32) -> Self {
        if version.0 <= 0x04000002 {
            return Self::Morrowind;
        }
        match (user_version, user_version_2) {
            (_, 0) if version == NifVersion::V20_0_0_5 => Self::Oblivion,
            (uv, _) if uv < 11 => Self::Oblivion,
            (11, uv2) if uv2 <= 34 => Self::Fallout3NV,
            (12, uv2) if uv2 <= 83 => Self::SkyrimLE,
            (12, uv2) if uv2 <= 100 => Self::SkyrimSE,
            (12, uv2) if uv2 >= 130 && uv2 < 155 => Self::Fallout4,
            (12, 155) => Self::Fallout76,
            (12, uv2) if uv2 >= 170 => Self::Starfield,
            _ => Self::Unknown,
        }
    }

    /// BSVER value for nif.xml compatibility.
    /// This is the user_version_2 that nif.xml uses for version conditionals.
    pub fn bsver(self) -> u32 {
        match self {
            Self::Morrowind | Self::Oblivion => 0,
            Self::Fallout3NV => 34,
            Self::SkyrimLE => 83,
            Self::SkyrimSE => 100,
            Self::Fallout4 => 130,
            Self::Fallout76 => 155,
            Self::Starfield => 172,
            Self::Unknown => 0,
        }
    }

    // ── Feature flags ──────────────────────────────────────────────
    // Each method documents which games have the feature and why.
    // Parsers call these instead of raw `user_version_2 >= N` checks.

    /// Bethesda compact material: ambient/diffuse omitted from NiMaterialProperty.
    /// Present in FO3/FNV+ (user_version >= 11, user_version_2 > 21).
    pub fn compact_material(self) -> bool {
        matches!(
            self,
            Self::Fallout3NV | Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4
        )
    }

    /// NiMaterialProperty has an emissive multiplier float after alpha.
    /// Present in FO3/FNV+ (user_version_2 >= 27).
    pub fn has_emissive_mult(self) -> bool {
        matches!(
            self,
            Self::Fallout3NV | Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4
        )
    }

    /// BSShaderPPLightingProperty has emissive color (4×f32) after texture set ref.
    /// Present in FNV+ (user_version_2 >= 34).
    pub fn has_shader_emissive_color(self) -> bool {
        matches!(
            self,
            Self::Fallout3NV | Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4
        )
    }

    /// NiTriShape has dedicated shader_property_ref and alpha_property_ref fields.
    /// Present in FO4+ (user_version_2 >= 130).
    pub fn has_dedicated_shader_refs(self) -> bool {
        matches!(self, Self::Fallout4)
    }

    /// NiGeometryData has a material CRC field after data_flags.
    /// Present in Skyrim+ (user_version >= 12).
    pub fn has_material_crc(self) -> bool {
        matches!(self, Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4)
    }

    /// Uses BSLightingShaderProperty instead of BSShaderPPLightingProperty.
    /// Present in Skyrim+ (BSVER >= 83). nif.xml: #SKY_AND_LATER#.
    pub fn uses_bs_lighting_shader(self) -> bool {
        matches!(
            self,
            Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4 | Self::Fallout76 | Self::Starfield
        )
    }

    // ── NiAVObject feature flags (from nif.xml) ───────────────────

    /// NiAVObject has Num Properties + Properties list.
    /// nif.xml: `#NI_BS_LTE_FO3#` (BSVER ≤ 34). Removed in Skyrim+.
    pub fn has_properties_list(self) -> bool {
        matches!(self, Self::Morrowind | Self::Oblivion | Self::Fallout3NV)
    }

    /// NiAVObject flags field is u32 (BSVER > 26). Older versions use u16.
    /// nif.xml: flags is uint for BSVER > 26, ushort otherwise.
    pub fn avobject_flags_u32(self) -> bool {
        matches!(
            self,
            Self::Fallout3NV
                | Self::SkyrimLE
                | Self::SkyrimSE
                | Self::Fallout4
                | Self::Fallout76
                | Self::Starfield
        )
    }

    // ── NiGeometry feature flags ──────────────────────────────────

    /// NiGeometry has Shader Property + Alpha Property refs.
    /// nif.xml: `#BS_GT_FO3#` (BSVER > 34). Present in Skyrim+, NOT in FNV.
    pub fn has_shader_alpha_refs(self) -> bool {
        matches!(
            self,
            Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4 | Self::Fallout76 | Self::Starfield
        )
    }

    // ── NiNode feature flags ──────────────────────────────────────

    /// NiNode has Num Effects + Effects list.
    /// nif.xml: `#NI_BS_LT_FO4#` (BSVER < 130). Present in everything pre-FO4.
    pub fn has_effects_list(self) -> bool {
        matches!(
            self,
            Self::Morrowind | Self::Oblivion | Self::Fallout3NV | Self::SkyrimLE | Self::SkyrimSE
        )
    }

    // ── BSShaderProperty feature flags ────────────────────────────

    /// BSShaderProperty has ShaderType, ShaderFlags, ShaderFlags2, EnvMapScale.
    /// nif.xml: `#NI_BS_LTE_FO3#` (BSVER ≤ 34). Only FO3/FNV.
    pub fn has_shader_property_fo3_fields(self) -> bool {
        matches!(self, Self::Fallout3NV)
    }

    /// BSLightingShaderProperty uses FO4 shader flag format.
    /// nif.xml: `#BS_FO4#` (BSVER == 130) or `#BS_FO4_2#` (130-139).
    pub fn uses_fo4_shader_flags(self) -> bool {
        matches!(self, Self::Fallout4)
    }

    /// BSLightingShaderProperty uses FO76/Starfield shader flag format.
    /// nif.xml: `#BS_GTE_132#`.
    pub fn uses_fo76_shader_flags(self) -> bool {
        matches!(self, Self::Fallout76 | Self::Starfield)
    }

    // ── BSTriShape (SSE+ specific geometry) ───────────────────────

    /// Uses BSTriShape instead of NiTriShape for geometry.
    /// nif.xml: `#SSE# #FO4# #F76#`. SSE and later.
    pub fn uses_bs_tri_shape(self) -> bool {
        matches!(
            self,
            Self::SkyrimSE | Self::Fallout4 | Self::Fallout76 | Self::Starfield
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_components() {
        let v = NifVersion(0x14020007);
        assert_eq!(v.major(), 0x14); // 20
        assert_eq!(v.minor(), 0x02);
        assert_eq!(v.patch(), 0x00);
        assert_eq!(v.build(), 0x07);
    }

    #[test]
    fn version_display() {
        assert_eq!(NifVersion(0x14020007).to_string(), "20.2.0.7");
        assert_eq!(NifVersion(0x04000002).to_string(), "4.0.0.2");
        assert_eq!(NifVersion(0x14000005).to_string(), "20.0.0.5");
    }

    #[test]
    fn version_ordering() {
        assert!(NifVersion::V4_0_0_2 < NifVersion::V20_0_0_5);
        assert!(NifVersion::V20_0_0_5 < NifVersion::V20_2_0_7);
        assert_eq!(NifVersion::V20_2_0_7, NifVersion::V20_2_0_7_SSE);
    }

    #[test]
    fn version_constants_match_packed() {
        assert_eq!(NifVersion::V4_0_0_2.0, 0x04000002);
        assert_eq!(NifVersion::V20_0_0_5.0, 0x14000005);
        assert_eq!(NifVersion::V20_2_0_7.0, 0x14020007);
    }

    #[test]
    fn detect_morrowind() {
        assert_eq!(
            NifVariant::detect(NifVersion::V4_0_0_2, 0, 0),
            NifVariant::Morrowind,
        );
    }

    #[test]
    fn detect_oblivion() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_5, 0, 0),
            NifVariant::Oblivion,
        );
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 10, 0),
            NifVariant::Oblivion,
        );
    }

    #[test]
    fn detect_fallout3_nv() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 11, 34),
            NifVariant::Fallout3NV,
        );
    }

    #[test]
    fn detect_skyrim_le() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 83),
            NifVariant::SkyrimLE,
        );
    }

    #[test]
    fn detect_skyrim_se() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 100),
            NifVariant::SkyrimSE,
        );
    }

    #[test]
    fn detect_fallout4() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 130),
            NifVariant::Fallout4,
        );
    }

    #[test]
    fn detect_fallout76() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 155),
            NifVariant::Fallout76,
        );
    }

    #[test]
    fn detect_starfield() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 172),
            NifVariant::Starfield,
        );
    }

    #[test]
    fn bsver_values() {
        assert_eq!(NifVariant::Fallout3NV.bsver(), 34);
        assert_eq!(NifVariant::SkyrimLE.bsver(), 83);
        assert_eq!(NifVariant::SkyrimSE.bsver(), 100);
        assert_eq!(NifVariant::Fallout4.bsver(), 130);
        assert_eq!(NifVariant::Fallout76.bsver(), 155);
    }

    #[test]
    fn feature_properties_list() {
        // FNV and earlier have properties list on NiAVObject
        assert!(NifVariant::Morrowind.has_properties_list());
        assert!(NifVariant::Oblivion.has_properties_list());
        assert!(NifVariant::Fallout3NV.has_properties_list());
        // Skyrim+ removed it
        assert!(!NifVariant::SkyrimLE.has_properties_list());
        assert!(!NifVariant::SkyrimSE.has_properties_list());
        assert!(!NifVariant::Fallout4.has_properties_list());
    }

    #[test]
    fn feature_shader_alpha_refs() {
        // Skyrim+ has dedicated shader/alpha property refs on NiGeometry
        assert!(!NifVariant::Fallout3NV.has_shader_alpha_refs());
        assert!(NifVariant::SkyrimLE.has_shader_alpha_refs());
        assert!(NifVariant::SkyrimSE.has_shader_alpha_refs());
        assert!(NifVariant::Fallout4.has_shader_alpha_refs());
    }

    #[test]
    fn feature_effects_list() {
        // Everything before FO4 has effects list on NiNode
        assert!(NifVariant::Fallout3NV.has_effects_list());
        assert!(NifVariant::SkyrimSE.has_effects_list());
        assert!(!NifVariant::Fallout4.has_effects_list());
    }

    #[test]
    fn feature_compact_material() {
        assert!(!NifVariant::Oblivion.compact_material());
        assert!(NifVariant::Fallout3NV.compact_material());
        assert!(NifVariant::SkyrimSE.compact_material());
    }

    #[test]
    fn feature_dedicated_shader_refs() {
        assert!(!NifVariant::Fallout3NV.has_dedicated_shader_refs());
        assert!(!NifVariant::SkyrimSE.has_dedicated_shader_refs());
        assert!(NifVariant::Fallout4.has_dedicated_shader_refs());
    }

    #[test]
    fn feature_material_crc() {
        assert!(!NifVariant::Fallout3NV.has_material_crc());
        assert!(NifVariant::SkyrimLE.has_material_crc());
        assert!(NifVariant::Fallout4.has_material_crc());
    }
}
