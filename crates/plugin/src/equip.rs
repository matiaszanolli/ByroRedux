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

/// Maximum LVLI recursion depth before [`expand_leveled_form_id`] gives
/// up. Vanilla outfit nesting tops out around 3-4 levels (a master list
/// of regional sub-lists, each of which references variant lists);
/// 8 leaves comfortable headroom and stops circular references from
/// spinning the parser. Hit-the-cap is logged once per fired site and
/// returns whatever was collected up to that point.
pub const LVLI_MAX_DEPTH: u32 = 8;

/// Expand a single form ID — which may be either a base item (ARMO /
/// WEAP / MISC) or a leveled-list reference (LVLI) — into a flat list
/// of base form IDs gated on `actor_level`. Pushes results onto `out`
/// in-place so the caller can build a mixed flat list across multiple
/// initial seeds without intermediate allocations.
///
/// **Determinism.** This picks the *highest-level entry whose level ≤
/// actor_level* (the LVLI flag-bit-0-unset Bethesda default for
/// "single-entry pick"). Vanilla outfits rarely set LVLI flag bit 0;
/// when they do, the deterministic pick still produces stable output
/// per-actor without needing a seeded RNG. The "calculate for each
/// item" flag (bit 1) is also unimplemented today — multi-pick LVLIs
/// land all eligible entries, which over-equips slightly compared to
/// Bethesda's runtime but is the safer-than-skipping default for a
/// rendering audit. Both gaps are signalled in the docstring rather
/// than producing a runtime warn — the audit-test workflow is the one
/// that benefits from the ceiling.
///
/// **`chance_none`.** Treated as 0 (always produce a result) for the
/// same render-audit reason. A future RNG-driven dispatch can opt in
/// per-actor; for now stable visible gear is the higher priority.
///
/// Recursion is capped at [`LVLI_MAX_DEPTH`]; over-cap LVLIs return
/// without expanding further and emit a one-shot debug log.
pub fn expand_leveled_form_id(
    form_id: u32,
    actor_level: i16,
    index: &EsmIndex,
    out: &mut Vec<u32>,
) {
    expand_leveled_inner(form_id, actor_level, index, out, 0);
}

fn expand_leveled_inner(
    form_id: u32,
    actor_level: i16,
    index: &EsmIndex,
    out: &mut Vec<u32>,
    depth: u32,
) {
    if depth >= LVLI_MAX_DEPTH {
        log::debug!(
            "expand_leveled_form_id: LVLI recursion cap ({}) hit at form_id {:08X} \
             — leaving subtree unexpanded",
            LVLI_MAX_DEPTH,
            form_id,
        );
        return;
    }
    // Direct base record — push and stop. Most outfit entries land
    // here on the first call.
    if index.items.get(&form_id).is_some() {
        out.push(form_id);
        return;
    }
    // Leveled list — recurse on the eligible entry / entries.
    let Some(lvli) = index.leveled_items.get(&form_id) else {
        // Unknown form ID — neither a base item nor a leveled list.
        // Could be a WEAP / KEYM / NOTE the dispatch hasn't categorised
        // yet, or a load-order conflict. Skip silently; the caller's
        // log already names the originating outfit / NPC.
        return;
    };

    // Filter entries by `level <= actor_level`. Pick the highest-level
    // eligible entry (the Bethesda default). Bethesda LVLI flag bit 1
    // ("calculate for each item") would multi-pick; we land all
    // eligible entries instead — over-equips slightly vs the runtime
    // but safer than skipping for the render-audit use case the
    // resolver targets today.
    let eligible: Vec<&_> = lvli
        .entries
        .iter()
        .filter(|e| e.level as i32 <= actor_level as i32)
        .collect();
    if eligible.is_empty() {
        return;
    }

    let multi_pick = lvli.flags & 0x02 != 0;
    if multi_pick {
        for entry in &eligible {
            expand_leveled_inner(entry.form_id, actor_level, index, out, depth + 1);
        }
    } else {
        // Single-pick: highest-level eligible entry. Stable across
        // reloads — no RNG.
        let pick = eligible
            .iter()
            .max_by_key(|e| e.level)
            .expect("eligible non-empty per check above");
        expand_leveled_inner(pick.form_id, actor_level, index, out, depth + 1);
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

    // ── expand_leveled_form_id (M41 Phase 2 LVLI dispatch) ──────────

    use crate::esm::records::container::{LeveledEntry, LeveledList};

    fn add_armo(idx: &mut EsmIndex, fid: u32) {
        idx.items.insert(fid, skyrim_armor(vec![]));
    }

    fn add_lvli(idx: &mut EsmIndex, fid: u32, flags: u8, entries: Vec<(u16, u32, u16)>) {
        idx.leveled_items.insert(
            fid,
            LeveledList {
                form_id: fid,
                editor_id: String::new(),
                chance_none: 0,
                flags,
                entries: entries
                    .into_iter()
                    .map(|(level, form_id, count)| LeveledEntry {
                        level,
                        form_id,
                        count,
                    })
                    .collect(),
            },
        );
    }

    /// Direct ARMO ref passes through: the resolver pushes the form ID
    /// verbatim and recursion never happens.
    #[test]
    fn expand_leveled_direct_armo_passthrough() {
        let mut idx = empty_index();
        add_armo(&mut idx, 0x0011_1111);
        let mut out = Vec::new();
        expand_leveled_form_id(0x0011_1111, 10, &idx, &mut out);
        assert_eq!(out, vec![0x0011_1111]);
    }

    /// Single-level LVLI with one ARMO entry: resolver picks the
    /// only eligible entry and returns the ARMO form ID.
    #[test]
    fn expand_leveled_single_entry_lvli() {
        let mut idx = empty_index();
        add_armo(&mut idx, 0x0022_2222);
        add_lvli(&mut idx, 0x0033_3333, 0, vec![(1, 0x0022_2222, 1)]);
        let mut out = Vec::new();
        expand_leveled_form_id(0x0033_3333, 10, &idx, &mut out);
        assert_eq!(out, vec![0x0022_2222]);
    }

    /// Level-gated pick: actor_level=5 sees only the level≤5 entries;
    /// the highest-eligible (level=5) wins over level=1.
    #[test]
    fn expand_leveled_picks_highest_eligible() {
        let mut idx = empty_index();
        add_armo(&mut idx, 0x0044_4444); // level 1
        add_armo(&mut idx, 0x0055_5555); // level 5
        add_armo(&mut idx, 0x0066_6666); // level 20 (gated out)
        add_lvli(
            &mut idx,
            0x0077_7777,
            0,
            vec![(1, 0x0044_4444, 1), (5, 0x0055_5555, 1), (20, 0x0066_6666, 1)],
        );
        let mut out = Vec::new();
        expand_leveled_form_id(0x0077_7777, 5, &idx, &mut out);
        assert_eq!(out, vec![0x0055_5555], "highest eligible (level=5) wins");
    }

    /// Below-floor actor: no entry has `level ≤ actor_level` → empty result.
    #[test]
    fn expand_leveled_actor_below_floor_returns_empty() {
        let mut idx = empty_index();
        add_armo(&mut idx, 0x0088_8888);
        add_lvli(&mut idx, 0x0099_9999, 0, vec![(10, 0x0088_8888, 1)]);
        let mut out = Vec::new();
        expand_leveled_form_id(0x0099_9999, 5, &idx, &mut out);
        assert!(
            out.is_empty(),
            "actor_level=5 with floor=10 must produce no equip"
        );
    }

    /// Multi-pick LVLI (flag bit 1 set) lands every eligible entry,
    /// not just one. Used by NPC outfit lists that bundle a torso +
    /// hands + boots LVLI under a single OTFT ref.
    #[test]
    fn expand_leveled_multi_pick_lands_all_eligible() {
        let mut idx = empty_index();
        add_armo(&mut idx, 0x00AA_AAAA);
        add_armo(&mut idx, 0x00BB_BBBB);
        add_lvli(
            &mut idx,
            0x00CC_CCCC,
            0x02, // flag bit 1 = "calculate for each item"
            vec![(1, 0x00AA_AAAA, 1), (1, 0x00BB_BBBB, 1)],
        );
        let mut out = Vec::new();
        expand_leveled_form_id(0x00CC_CCCC, 10, &idx, &mut out);
        // Order is iteration-order over `entries`; both must land.
        assert_eq!(out.len(), 2);
        assert!(out.contains(&0x00AA_AAAA));
        assert!(out.contains(&0x00BB_BBBB));
    }

    /// Nested LVLI: an outer list whose pick is itself a leveled list
    /// recurses correctly to the inner ARMO.
    #[test]
    fn expand_leveled_nested_lvli_recurses() {
        let mut idx = empty_index();
        add_armo(&mut idx, 0x00DD_DDDD);
        add_lvli(&mut idx, 0x00EE_EEEE, 0, vec![(1, 0x00DD_DDDD, 1)]);
        // Outer LVLI: single entry pointing at the inner LVLI.
        add_lvli(&mut idx, 0x00FF_FFFF, 0, vec![(1, 0x00EE_EEEE, 1)]);
        let mut out = Vec::new();
        expand_leveled_form_id(0x00FF_FFFF, 10, &idx, &mut out);
        assert_eq!(
            out,
            vec![0x00DD_DDDD],
            "nested LVLI must resolve to the innermost ARMO"
        );
    }

    /// Recursion cap at LVLI_MAX_DEPTH: a circular self-reference
    /// returns without panic instead of stack-overflowing.
    #[test]
    fn expand_leveled_circular_reference_caps_at_max_depth() {
        let mut idx = empty_index();
        // Self-referencing LVLI — entry points back at itself.
        add_lvli(&mut idx, 0x0123_4567, 0, vec![(1, 0x0123_4567, 1)]);
        let mut out = Vec::new();
        // No panic, no infinite recursion. Output is empty (the cap
        // hits before any base ARMO is reached).
        expand_leveled_form_id(0x0123_4567, 10, &idx, &mut out);
        assert!(out.is_empty());
    }

    /// Unknown form IDs (neither ARMO nor LVLI in the index) are
    /// silently skipped — handles WEAP / KEYM / NOTE references that
    /// the dispatch hasn't categorised yet, plus load-order conflicts.
    #[test]
    fn expand_leveled_unknown_form_id_silently_skipped() {
        let idx = empty_index();
        let mut out = Vec::new();
        expand_leveled_form_id(0x0DEA_DEAD, 10, &idx, &mut out);
        assert!(out.is_empty());
    }
}
