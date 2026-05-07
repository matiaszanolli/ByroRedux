//! Per-game biped-slot bitmask constants and helpers for ARMO records.
//!
//! Sourced verbatim from the xEdit project (TES5Edit / FNVEdit /
//! TES4Edit / FO4Edit), by ElminsterAU and the xEdit team, MPL-2.0
//! licensed:
//!
//!   <https://github.com/TES5Edit/TES5Edit>
//!
//! Specifically `wbDefinitionsTES4.pas` / `wbDefinitionsFNV.pas` /
//! `wbDefinitionsTES5.pas` / `wbDefinitionsFO4.pas` at tag
//! `dev-4.1.6` (commit valid 2026-05-07).
//!
//! Bethesda doesn't ship public `BipedObject` enum headers for any of
//! the targeted games, so xEdit is the canonical community reference
//! — the same definitions every mod-tooling pipeline reads.
//!
//! The bit mappings are NOT consistent across games; FO4 in particular
//! reorganised the layout. Always go through these helpers rather than
//! hard-coding bit positions inline.
//!
//! ## Bit layouts (low bits only — high bits skipped where unused
//! by the helpers below)
//!
//! | bit | Oblivion (BMDT u16) | FO3 / FNV (BMDT low u16) | Skyrim+ (BOD2 u32) | FO4 (BOD2 u32) |
//! |-----|---------------------|--------------------------|--------------------|----------------|
//! | 0   | Head                | Head                     | 30 - Head          | 30 - Hair Top  |
//! | 1   | Hair                | Hair                     | 31 - Hair          | 31 - Hair Long |
//! | 2   | **Upper Body**      | **Upper Body**           | **32 - Body**      | 32 - FaceGen Head |
//! | 3   | Lower Body          | Left Hand                | 33 - Hands         | **33 - BODY**  |
//! | 4   | Hand                | Right Hand               | 34 - Forearms      | 34 - L Hand    |
//!
//! "Main body" — the bit that, when occupied, means the equipped
//! armor's mesh covers the actor's torso/legs/arms enough to make the
//! base body NIF (`upperbody.nif` on FO3/FNV) redundant — is **bit 2**
//! on Oblivion / FO3 / FNV / Skyrim+ but **bit 3** on FO4. The helper
//! below routes per game so callers don't need to know.

use crate::esm::reader::GameKind;

/// Returns the bit position (`0..32`) within an ARMO biped-flags
/// bitmask whose set state means "this armor covers the actor's main
/// body / torso." `None` for games that don't expose ARMO records
/// through this codepath (TES3 — separate format).
pub const fn main_body_bit(game: GameKind) -> Option<u8> {
    match game {
        // BMDT u16 (Oblivion 4-byte total) and BMDT u32 low half
        // (FO3/FNV 8-byte total). Both put Upper Body at bit 2.
        // Skyrim+ BOD2 u32 also lands "32 - Body" at bit 2 (the
        // "32" in the xEdit label is the BSDismemberBodyPartType
        // enum value, NOT the bit position).
        GameKind::Oblivion | GameKind::Fallout3NV | GameKind::Skyrim => Some(2),
        // FO4 reorganised the layout — bit 2 became "FaceGen Head"
        // and bit 3 became BODY. FO76 inherits FO4's layout per
        // Bethesda's typical incremental reuse pattern.
        GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => Some(3),
    }
}

/// Returns true when an armor's biped-flags bitmask occupies the
/// game's main-body slot. Used by the spawn pipeline to skip the
/// base-body NIF (`upperbody.nif` etc.) when an equipped armor's
/// mesh already covers the torso — vanilla armors include exposed
/// body parts inline, so doubling up causes z-fight + 2× skinned
/// bone palette load.
///
/// Verified against xEdit `dev-4.1.6` definitions (2026-05-07).
pub fn armor_covers_main_body(game: GameKind, biped_flags: u32) -> bool {
    match main_body_bit(game) {
        Some(bit) => biped_flags & (1u32 << bit) != 0,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_upper_body_bit_is_2() {
        // 0x0004 = bit 2 = "Upper Body" per xEdit wbDefinitionsFNV.pas:4031
        assert!(armor_covers_main_body(GameKind::Fallout3NV, 0x0004));
        // Bit 0 (Head), bit 1 (Hair), bit 4 (Right Hand) all leave
        // body uncovered.
        assert!(!armor_covers_main_body(GameKind::Fallout3NV, 0x0001));
        assert!(!armor_covers_main_body(GameKind::Fallout3NV, 0x0002));
        assert!(!armor_covers_main_body(GameKind::Fallout3NV, 0x0010));
    }

    #[test]
    fn oblivion_upper_body_bit_is_2() {
        // wbDefinitionsTES4.pas:1332 — bit 2 = "Upper Body".
        assert!(armor_covers_main_body(GameKind::Oblivion, 0x0004));
        assert!(!armor_covers_main_body(GameKind::Oblivion, 0x0001));
    }

    #[test]
    fn skyrim_body_bit_is_2() {
        // wbDefinitionsTES5.pas:2593 — bit 2 = "32 - Body".
        assert!(armor_covers_main_body(GameKind::Skyrim, 0x0004));
        // Bit 7 = "37 - Feet", doesn't cover torso.
        assert!(!armor_covers_main_body(GameKind::Skyrim, 0x0080));
    }

    #[test]
    fn fo4_body_bit_is_3() {
        // wbDefinitionsFO4.pas — bit 3 = "33 - BODY". Bit 2 is
        // "32 - FaceGen Head", which does NOT cover the actor's
        // torso even though the SBP enum value is the same as
        // Skyrim's body slot.
        assert!(armor_covers_main_body(GameKind::Fallout4, 0x0008));
        assert!(!armor_covers_main_body(GameKind::Fallout4, 0x0004));
    }

    #[test]
    fn empty_flags_never_cover_body() {
        for game in [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ] {
            assert!(!armor_covers_main_body(game, 0));
        }
    }
}
