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

    pub fn major(self) -> u8 { (self.0 >> 24) as u8 }
    pub fn minor(self) -> u8 { (self.0 >> 16) as u8 }
    pub fn patch(self) -> u8 { (self.0 >> 8) as u8 }
    pub fn build(self) -> u8 { self.0 as u8 }
}

impl fmt::Display for NifVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}.{}", self.major(), self.minor(), self.patch(), self.build())
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
    /// Fallout 3 / Fallout New Vegas (NIF 20.2.0.7, uv=11, uv2=34)
    Fallout3NV,
    /// Skyrim LE (NIF 20.2.0.7, uv=12, uv2=83)
    SkyrimLE,
    /// Skyrim SE (NIF 20.2.0.7, uv=12, uv2=100)
    SkyrimSE,
    /// Fallout 4 (NIF 20.2.0.7, uv=12, uv2=130)
    Fallout4,
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
            (12, uv2) if uv2 >= 130 => Self::Fallout4,
            _ => Self::Unknown,
        }
    }

    // ── Feature flags ──────────────────────────────────────────────
    // Each method documents which games have the feature and why.
    // Parsers call these instead of raw `user_version_2 >= N` checks.

    /// Bethesda compact material: ambient/diffuse omitted from NiMaterialProperty.
    /// Present in FO3/FNV+ (user_version >= 11, user_version_2 > 21).
    pub fn compact_material(self) -> bool {
        matches!(self, Self::Fallout3NV | Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4)
    }

    /// NiMaterialProperty has an emissive multiplier float after alpha.
    /// Present in FO3/FNV+ (user_version_2 >= 27).
    pub fn has_emissive_mult(self) -> bool {
        matches!(self, Self::Fallout3NV | Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4)
    }

    /// BSShaderPPLightingProperty has emissive color (4×f32) after texture set ref.
    /// Present in FNV+ (user_version_2 >= 34).
    pub fn has_shader_emissive_color(self) -> bool {
        matches!(self, Self::Fallout3NV | Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4)
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
    /// Present in Skyrim+ (user_version >= 12).
    pub fn uses_bs_lighting_shader(self) -> bool {
        matches!(self, Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4)
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
