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
use crate::esm::records::{EsmIndex, ItemKind, ItemRecord};

/// Actor gender as recorded by the ACBS sub-record's flags field.
///
/// Bit 0 of `acbs_flags` is the canonical "Female" flag across every
/// targeted Bethesda game from Oblivion through Starfield (per UESP
/// ACBS documentation). The plugin crate exposes the enum so the
/// equip resolver can dispatch without depending on the binary's
/// version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Male,
    Female,
}

impl Gender {
    /// Decode the gender bit from an `NpcRecord::acbs_flags` value.
    pub fn from_acbs_flags(flags: u32) -> Self {
        if flags & 0x0000_0001 != 0 {
            Self::Female
        } else {
            Self::Male
        }
    }
}

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

/// Resolve an `ItemRecord` (assumed to be an ARMO) to the path of the
/// worn mesh that should spawn on an actor of the given gender + race.
/// Returns `None` if the item is not an armor record, has no mesh, or
/// the per-game ARMA dispatch finds no match.
///
/// Per-game shape:
///
/// * **Oblivion / FO3 / FNV** — ARMO carries the worn mesh path
///   directly in `common.model_path`. No ARMA dispatch.
/// * **Skyrim+ / FO4 / FO76 / Starfield** — ARMO's `armatures` field
///   lists ARMA FormIDs. Each ARMA has a primary race (`race_form_id`,
///   from RNAM) plus optional `additional_races`. The resolver picks
///   the first ARMA whose race set contains the actor's race, then
///   returns the gender-appropriate biped model
///   (`male_biped_model` / `female_biped_model`). When no ARMA matches
///   the actor's race, falls back to the first ARMA with a non-empty
///   gender-appropriate mesh — handles "default human" addons that
///   ship without a race link but cover most actors in practice.
///
/// The `&'a str` return borrows from `armor` or `index`, so the
/// caller must keep both alive while consuming the path. For the
/// spawn pipeline that's the cell-load scope, which already holds
/// the `EsmIndex` Arc — no lifetime acrobatics required.
pub fn resolve_armor_mesh<'a>(
    armor: &'a ItemRecord,
    gender: Gender,
    race_form_id: u32,
    index: &'a EsmIndex,
    game: GameKind,
) -> Option<&'a str> {
    let ItemKind::Armor { ref armatures, .. } = armor.kind else {
        return None;
    };

    let is_skyrim_or_later = matches!(
        game,
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield
    );

    if !is_skyrim_or_later {
        // Oblivion / FO3 / FNV: ARMO MODL is the worn mesh.
        let path = armor.common.model_path.as_str();
        return if path.is_empty() { None } else { Some(path) };
    }

    let pick_path = |arma: &'a crate::esm::records::ArmaRecord| -> Option<&'a str> {
        let path = match gender {
            Gender::Male => arma.male_biped_model.as_str(),
            Gender::Female => arma.female_biped_model.as_str(),
        };
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
    };

    // Pass 1: prefer an ARMA whose race set contains the actor's race.
    for &arma_fid in armatures {
        let Some(arma) = index.armor_addons.get(&arma_fid) else {
            continue;
        };
        let race_match = arma.race_form_id == race_form_id
            || arma.additional_races.contains(&race_form_id);
        if race_match {
            if let Some(path) = pick_path(arma) {
                return Some(path);
            }
        }
    }

    // Pass 2: no race-match — take the first ARMA with a non-empty
    // gender-appropriate mesh. Vanilla "default human" addons often
    // ship without an explicit RNAM (race_form_id == 0) but still
    // resolve correctly for most humanoid actors.
    for &arma_fid in armatures {
        let Some(arma) = index.armor_addons.get(&arma_fid) else {
            continue;
        };
        if let Some(path) = pick_path(arma) {
            return Some(path);
        }
    }

    None
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

    // ── resolve_armor_mesh ────────────────────────────────────────────

    use crate::esm::records::{
        common::CommonItemFields, ArmaRecord, EsmIndex, ItemKind, ItemRecord,
    };

    fn fnv_armor(model_path: &str) -> ItemRecord {
        ItemRecord {
            form_id: 0x0001_FFFF,
            common: CommonItemFields {
                model_path: model_path.to_string(),
                ..Default::default()
            },
            kind: ItemKind::Armor {
                biped_flags: 0x0004,
                dt: 0.0,
                dr: 0,
                health: 0,
                slot_mask: 0x0004,
                armor_rating_x100: 0,
                armor_type: None,
                armatures: Vec::new(),
            },
        }
    }

    fn skyrim_armor(armatures: Vec<u32>) -> ItemRecord {
        ItemRecord {
            form_id: 0x0001_AAAA,
            common: CommonItemFields::default(),
            kind: ItemKind::Armor {
                biped_flags: 0x0004,
                dt: 0.0,
                dr: 0,
                health: 0,
                slot_mask: 0,
                armor_rating_x100: 0,
                armor_type: Some(1),
                armatures,
            },
        }
    }

    fn arma_for_race(
        form_id: u32,
        race: u32,
        additional: Vec<u32>,
        male: &str,
        female: &str,
    ) -> ArmaRecord {
        ArmaRecord {
            form_id,
            editor_id: String::new(),
            biped_flags: 0x0004,
            general_flags: 0,
            dt: 0,
            dr: 0,
            race_form_id: race,
            male_biped_model: male.to_string(),
            female_biped_model: female.to_string(),
            additional_races: additional,
        }
    }

    fn empty_index() -> EsmIndex {
        EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        }
    }

    #[test]
    fn fnv_returns_armo_modl_directly() {
        let armor = fnv_armor(r"armor\dressclothes\dressm.nif");
        let idx = EsmIndex {
            game: GameKind::Fallout3NV,
            ..Default::default()
        };
        let path = resolve_armor_mesh(&armor, Gender::Male, 0, &idx, GameKind::Fallout3NV);
        assert_eq!(path, Some(r"armor\dressclothes\dressm.nif"));
    }

    #[test]
    fn fnv_empty_modl_returns_none() {
        let armor = fnv_armor("");
        let idx = EsmIndex {
            game: GameKind::Fallout3NV,
            ..Default::default()
        };
        assert_eq!(
            resolve_armor_mesh(&armor, Gender::Male, 0, &idx, GameKind::Fallout3NV),
            None
        );
    }

    #[test]
    fn skyrim_picks_race_matched_arma_male() {
        let nord_race = 0x0001_3746;
        let armor = skyrim_armor(vec![0xAA, 0xBB]);
        let mut idx = empty_index();
        // Beast-race ARMA — wrong race, should be skipped.
        idx.armor_addons.insert(
            0xAA,
            arma_for_race(0xAA, 0x0001_3744 /* Khajiit */, vec![], "beast_m.nif", "beast_f.nif"),
        );
        // Human ARMA — matches via primary RNAM.
        idx.armor_addons.insert(
            0xBB,
            arma_for_race(0xBB, nord_race, vec![], "human_m.nif", "human_f.nif"),
        );
        let path = resolve_armor_mesh(&armor, Gender::Male, nord_race, &idx, GameKind::Skyrim);
        assert_eq!(path, Some("human_m.nif"));
    }

    #[test]
    fn skyrim_picks_via_additional_races() {
        let imperial_race = 0x0001_3741;
        let nord_race = 0x0001_3746;
        let armor = skyrim_armor(vec![0xCC]);
        let mut idx = empty_index();
        // Primary RNAM is Nord, but Imperial is in additional_races.
        idx.armor_addons.insert(
            0xCC,
            arma_for_race(0xCC, nord_race, vec![imperial_race], "shared_m.nif", "shared_f.nif"),
        );
        let path = resolve_armor_mesh(
            &armor,
            Gender::Female,
            imperial_race,
            &idx,
            GameKind::Skyrim,
        );
        assert_eq!(
            path,
            Some("shared_f.nif"),
            "additional_races membership must count as a race match"
        );
    }

    #[test]
    fn skyrim_falls_back_to_first_arma_when_no_race_match() {
        let nord_race = 0x0001_3746;
        let unknown_race = 0xDEAD_BEEF;
        let armor = skyrim_armor(vec![0xDD, 0xEE]);
        let mut idx = empty_index();
        idx.armor_addons.insert(
            0xDD,
            arma_for_race(0xDD, nord_race, vec![], "first_m.nif", "first_f.nif"),
        );
        idx.armor_addons.insert(
            0xEE,
            arma_for_race(0xEE, nord_race, vec![], "second_m.nif", "second_f.nif"),
        );
        // Actor race doesn't match either ARMA — fallback to first.
        let path = resolve_armor_mesh(
            &armor,
            Gender::Male,
            unknown_race,
            &idx,
            GameKind::Skyrim,
        );
        assert_eq!(path, Some("first_m.nif"));
    }

    #[test]
    fn skyrim_no_armatures_returns_none() {
        let armor = skyrim_armor(Vec::new());
        let idx = empty_index();
        assert_eq!(
            resolve_armor_mesh(&armor, Gender::Male, 0, &idx, GameKind::Skyrim),
            None
        );
    }

    #[test]
    fn skyrim_dangling_arma_refs_skipped() {
        let armor = skyrim_armor(vec![0x_BAAD_F00D]);
        let idx = empty_index();
        assert_eq!(
            resolve_armor_mesh(&armor, Gender::Male, 0, &idx, GameKind::Skyrim),
            None,
            "ARMA refs that don't resolve in the index must be skipped \
             rather than panic"
        );
    }
}
