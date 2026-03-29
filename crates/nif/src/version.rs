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
}
