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
    resolve_armor_meshes(armor, gender, race_form_id, index, game)
        .into_iter()
        .next()
}

/// Every worn mesh an ARMO contributes for this gender + race.
///
/// #3357 — on Skyrim+ an ARMO links N ARMAs and its `BOD2` mask is the
/// **union** of the regions those addons cover, each supplying its own NIF.
/// The single-`Option` resolver returned the first race-matching addon and
/// stopped, which is right for a one-region item (a cuirass, a ring) and
/// wrong for anything multi-region.
///
/// The race default skin is the case that matters: `SkinNaked`
/// (`0x00000D64`, `BOD2 = Head|Body|Hands|Feet`) carries 25 ARMAs, three of
/// which match any given human race — `NakedTorso`, `NakedHands`,
/// `NakedFeet`. `NakedFeet` sorts first in the armature list, so the skin
/// layer resolved to a *feet* NIF for 2,068 of 5,118 vanilla Skyrim NPCs,
/// and the `hidden_biped_mask` logic then applied a torso/feet displacement
/// mask to the wrong mesh. 166 of 2,762 Skyrim ARMOs have more than one
/// ARMA serving the same race, so the shape is wrong beyond the skin too.
///
/// Paths are de-duplicated: several ARMAs can legitimately name one NIF,
/// and spawning it twice would double-draw the same geometry.
pub fn resolve_armor_meshes<'a>(
    armor: &'a ItemRecord,
    gender: Gender,
    race_form_id: u32,
    index: &'a EsmIndex,
    game: GameKind,
) -> Vec<&'a str> {
    let ItemKind::Armor { ref armatures, .. } = armor.kind else {
        return Vec::new();
    };

    let is_skyrim_or_later = matches!(
        game,
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield
    );

    if !is_skyrim_or_later {
        // Oblivion / FO3 / FNV: ARMO MODL is the worn mesh. One record,
        // one mesh — no ARMA dispatch, so never more than one path.
        let path = armor.common.model_path.as_str();
        return if path.is_empty() {
            Vec::new()
        } else {
            vec![path]
        };
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

    // Pass 1: EVERY ARMA whose race set contains the actor's race — the
    // ARMO's biped mask is their union, so taking only the first leaves the
    // other regions bare (#3357). Authored armature order is preserved.
    let mut out: Vec<&'a str> = Vec::new();
    for &arma_fid in armatures {
        let Some(arma) = index.armor_addons.get(&arma_fid) else {
            continue;
        };
        let race_match =
            arma.race_form_id == race_form_id || arma.additional_races.contains(&race_form_id);
        if race_match {
            if let Some(path) = pick_path(arma) {
                if !out.contains(&path) {
                    out.push(path);
                }
            }
        }
    }
    if !out.is_empty() {
        return out;
    }

    // Pass 2: no race-match — take the first ARMA with a non-empty
    // gender-appropriate mesh. Vanilla "default human" addons often
    // ship without an explicit RNAM (race_form_id == 0) but still
    // resolve correctly for most humanoid actors. Deliberately still
    // single-valued: without a race match there is no evidence that the
    // remaining addons are meant for this actor, and returning all of
    // them would stack unrelated meshes.
    for &arma_fid in armatures {
        let Some(arma) = index.armor_addons.get(&arma_fid) else {
            continue;
        };
        if let Some(path) = pick_path(arma) {
            return vec![path];
        }
    }

    Vec::new()
}

/// Maximum LVLI recursion depth before [`expand_leveled_form_id`] gives
/// up. Vanilla outfit nesting tops out around 3-4 levels (a master list
/// of regional sub-lists, each of which references variant lists);
/// 8 leaves comfortable headroom and stops circular references from
/// spinning the parser. Hit-the-cap is logged once per fired site and
/// returns whatever was collected up to that point.
pub const LVLI_MAX_DEPTH: u32 = 8;

/// FNV / FO3 `NpcRecord::template_flags` bits. Sourced from xEdit
/// `wbDefinitionsFNV.pas` (commit `dev-4.1.6`), the same authority the
/// surrounding biped-slot helpers use. `NpcRecord::template_flags`'s own doc
/// comment enumerates all twelve; these three are the ones this crate
/// currently resolves against real data (#2956) — the rest are parsed and
/// stored for the dispatcher but have no consumer yet.
pub const TEMPLATE_FLAG_USE_TRAITS: u16 = 0x0001;
pub const TEMPLATE_FLAG_USE_STATS: u16 = 0x0002;
pub const TEMPLATE_FLAG_USE_INVENTORY: u16 = 0x0100;

/// Maximum TPLT recursion depth for [`resolve_inherited_record`] and its
/// public wrappers ([`resolve_inherited_inventory`],
/// [`resolve_inherited_stats`], [`resolve_inherited_traits`]). Vanilla
/// template chains are flat (Lvl* template → base NPC, one hop) but mod
/// content occasionally chains a per-faction wrapper on top; 6 is
/// conservative headroom and breaks any cycle. Same justification as
/// [`LVLI_MAX_DEPTH`].
pub const TPLT_MAX_DEPTH: u32 = 6;

/// Resolve the effective inventory list for an NPC, honouring
/// FNV / FO3 `TPLT` template inheritance. When the NPC's
/// `template_flags` carries [`TEMPLATE_FLAG_USE_INVENTORY`] AND its
/// `template_form_id` resolves to a base NPC_, return THAT base's
/// inventory (with the same recursive walk so chained templates
/// resolve all the way to a leaf with authored CNTO). When TPLT
/// points at an LVLN, pick the leveled-list's highest-level
/// eligible entry that's an NPC_ and use its inventory.
///
/// Without this resolution every vanilla `Lvl*` NPC (Powder
/// Gangers, Caesar's Legion, NCR Troopers, generic settlers) spawns
/// with empty inventory → zero armor / weapon dispatch → naked
/// rendering.
///
/// Returns a borrow into the resolved record's `inventory` vector,
/// or — when no inheritance applies — a borrow of the input NPC's
/// own inventory.
pub fn resolve_inherited_inventory<'a>(
    npc: &'a crate::esm::records::actor::NpcRecord,
    actor_level: i16,
    index: &'a EsmIndex,
) -> &'a [crate::esm::records::actor::NpcInventoryEntry] {
    &resolve_inherited_record(npc, actor_level, index, TEMPLATE_FLAG_USE_INVENTORY, 0).inventory
}

/// Resolve the NPC record that should supply SPECIAL / class / level for
/// this actor, honouring FNV / FO3 `TPLT` template inheritance the same way
/// [`resolve_inherited_inventory`] does for CNTO (#2956).
///
/// `docs/engine/charal-fo4-ruleset.md`'s inheritance-chain section says that
/// `TPLT` + ACBS Template Flags inherit SPECIAL / level / etc. from the
/// template `NPC_`/`LVLN` when `Use Stats` is set. Before this existed, CHARAL
/// population read the NPC's own `class_form_id` and level unconditionally —
/// measured against vanilla FNV/FO3, 55.0% / 53.4% of templated NPCs carry
/// `Use Stats`, and among those resolving to a direct `NPC_`, 117/1510 (FNV)
/// and 105/720 (FO3) have an own class that disagrees with the template's,
/// silently mis-deriving a full SPECIAL + 15-skill set (each wrong class
/// mis-states 22 actor values, since skills derive from SPECIAL via
/// `base_skill`). The 587 FNV / 159 FO3 `LVLN`-targeted cases were never
/// resolved at all.
///
/// Returns the input NPC unchanged when `Use Stats` isn't set, `template_form_id`
/// is `0`, or the template can't be resolved — the same resolve-or-fall-back
/// contract as the inventory resolver.
pub fn resolve_inherited_stats<'a>(
    npc: &'a crate::esm::records::actor::NpcRecord,
    actor_level: i16,
    index: &'a EsmIndex,
) -> &'a crate::esm::records::actor::NpcRecord {
    resolve_inherited_record(npc, actor_level, index, TEMPLATE_FLAG_USE_STATS, 0)
}

/// Resolve the NPC record that should supply race (and other "traits"
/// fields) for this actor, honouring the same `TPLT` inheritance as
/// [`resolve_inherited_stats`] but gated on [`TEMPLATE_FLAG_USE_TRAITS`]
/// ("Use Traits") instead of "Use Stats" — a separate, independently-set
/// bit (#2956). Measured against vanilla FNV/FO3: of NPCs with `Use
/// Traits` set and a resolvable direct-`NPC_` template, 2/744 (FNV) and
/// 19/337 (FO3) have an own race that disagrees with the template's.
pub fn resolve_inherited_traits<'a>(
    npc: &'a crate::esm::records::actor::NpcRecord,
    actor_level: i16,
    index: &'a EsmIndex,
) -> &'a crate::esm::records::actor::NpcRecord {
    resolve_inherited_record(npc, actor_level, index, TEMPLATE_FLAG_USE_TRAITS, 0)
}

/// Shared `TPLT` walker behind [`resolve_inherited_inventory`],
/// [`resolve_inherited_stats`] and [`resolve_inherited_traits`]: follow the
/// template chain only while `npc.template_flags & flag` is set, the same
/// depth cap and `LVLN` highest-eligible-tier pick regardless of which
/// category is being resolved (`flag` only gates *whether* to keep
/// following the chain, not how — a chain can legitimately have `Use
/// Stats` set at one level and not the next).
fn resolve_inherited_record<'a>(
    npc: &'a crate::esm::records::actor::NpcRecord,
    actor_level: i16,
    index: &'a EsmIndex,
    flag: u16,
    depth: u32,
) -> &'a crate::esm::records::actor::NpcRecord {
    if depth >= TPLT_MAX_DEPTH {
        log::debug!(
            "resolve_inherited_record: TPLT recursion cap ({}) hit at NPC \
             {:08X} ({}) resolving flag {:#06x} — leaving subtree unresolved",
            TPLT_MAX_DEPTH,
            npc.form_id,
            npc.editor_id,
            flag,
        );
        return npc;
    }
    if npc.template_flags & flag == 0 || npc.template_form_id == 0 {
        return npc;
    }
    // Direct NPC_ template — recurse so a Lvl* → Lvl* → leaf chain
    // resolves at the bottom.
    if let Some(base) = index.npcs.get(&npc.template_form_id) {
        return resolve_inherited_record(base, actor_level, index, flag, depth + 1);
    }
    // LVLN template — pick the highest-level eligible variant whose
    // form ID resolves to an NPC_, then recurse into IT. Vanilla
    // LVLN entries point at NPC_ records directly (no LVLI-style
    // multi-pick on the leveled-NPC path), but the same level-gate
    // applies.
    if let Some(lvln) = index.leveled_npcs.get(&npc.template_form_id) {
        let mut eligible: Vec<&_> = lvln
            .entries
            .iter()
            .filter(|e| e.level as i32 <= actor_level as i32)
            .collect();
        // Determinism — same "highest level ≤ actor_level" rule
        // expand_leveled_form_id uses for LVLI. Sort then take last
        // so ties break on insertion order (stable).
        eligible.sort_by_key(|e| e.level);
        if let Some(pick) = eligible.last() {
            if let Some(base) = index.npcs.get(&pick.form_id) {
                return resolve_inherited_record(base, actor_level, index, flag, depth + 1);
            }
        }
    }
    // TPLT pointed at something neither indexed nor an LVLN —
    // ambiguous mod content or missing master. Fall back to the
    // NPC's own record rather than crashing.
    npc
}

/// Expand a single form ID — which may be either a base item (ARMO /
/// WEAP / MISC) or a leveled-list reference (LVLI) — into a flat list
/// of base form IDs gated on `actor_level`. Pushes results onto `out`
/// in-place so the caller can build a mixed flat list across multiple
/// initial seeds without intermediate allocations.
///
/// **Determinism.** TES5's `Use All` flag (bit 2 / `0x04`) expands every
/// eligible entry, preserving authored armour bundles. Otherwise the resolver
/// single-picks the *highest-level entry whose level ≤ actor_level*.
/// `Calculate for each item` (bit 1 / `0x02`) repeats that one roll for the
/// list entry's count; it does not turn a level-tier ladder into a bundle.
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
    if index.items.contains_key(&form_id) {
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

    // Filter entries by `level <= actor_level`, then branch on TES5 `Use All`.
    // `Calculate for each item` (bit 1 / 0x02) changes roll cardinality, not
    // entry selection; treating it as Use All over-equipped 1,491 vanilla
    // Skyrim NPCs (#3217).
    let eligible: Vec<&_> = lvli
        .entries
        .iter()
        .filter(|e| e.level as i32 <= actor_level as i32)
        .collect();
    if eligible.is_empty() {
        return;
    }

    let multi_pick = lvli.flags & 0x04 != 0;
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

    /// #3357 — the race default skin is the case the single-`Option`
    /// resolver got wrong. `SkinNaked` (`0x00000D64`) carries 25 ARMAs and
    /// its `BOD2` is the union `Head|Body|Hands|Feet`; three addons match
    /// any given human race. `NakedFeet` sorts first in the armature list,
    /// so the old resolver returned a *feet* NIF for the whole skin layer —
    /// 2,068 of 5,118 vanilla Skyrim NPCs rendered with no torso and no
    /// hands. FormIDs and mesh paths below are the real vanilla values.
    #[test]
    fn skyrim_skin_resolves_every_race_matching_arma() {
        let nord_race = 0x0001_3746;
        let generic_human = 0x0000_0019;
        let armor = skyrim_armor(vec![0x0000_0D6E, 0x0000_0D6C, 0x0000_0D67]);
        let mut idx = empty_index();
        // Authored order: Feet, Hands, Torso — all three reach Nord via
        // `additional_races`, exactly as SkinNaked does.
        for (fid, male, female) in [
            (
                0x0000_0D6E_u32,
                r"Actors\Character\Character Assets\MaleFeet_1.nif",
                r"Actors\Character\Character Assets\FemaleFeet_1.nif",
            ),
            (
                0x0000_0D6C,
                r"Actors\Character\Character Assets\MaleHands_1.nif",
                r"Actors\Character\Character Assets\FemaleHands_1.nif",
            ),
            (
                0x0000_0D67,
                r"Actors\Character\Character Assets\MaleBody_1.NIF",
                r"Actors\Character\Character Assets\FemaleBody_1.nif",
            ),
        ] {
            idx.armor_addons.insert(
                fid,
                arma_for_race(fid, generic_human, vec![nord_race], male, female),
            );
        }

        let paths = resolve_armor_meshes(&armor, Gender::Male, nord_race, &idx, GameKind::Skyrim);
        assert_eq!(
            paths,
            vec![
                r"Actors\Character\Character Assets\MaleFeet_1.nif",
                r"Actors\Character\Character Assets\MaleHands_1.nif",
                r"Actors\Character\Character Assets\MaleBody_1.NIF",
            ],
            "all three race-matching addons must contribute a mesh, in authored \
             order — returning only the first leaves torso and hands bare (#3357)"
        );

        // The thin wrapper still yields one path for callers that want one.
        assert_eq!(
            resolve_armor_mesh(&armor, Gender::Male, nord_race, &idx, GameKind::Skyrim),
            Some(r"Actors\Character\Character Assets\MaleFeet_1.nif")
        );
    }

    /// Several ARMAs can legitimately name one NIF; spawning it twice would
    /// double-draw the same geometry.
    #[test]
    fn skyrim_duplicate_arma_paths_are_deduped() {
        let nord_race = 0x0001_3746;
        let armor = skyrim_armor(vec![0xA1, 0xA2]);
        let mut idx = empty_index();
        idx.armor_addons.insert(
            0xA1,
            arma_for_race(0xA1, nord_race, vec![], "shared.nif", "shared_f.nif"),
        );
        idx.armor_addons.insert(
            0xA2,
            arma_for_race(0xA2, nord_race, vec![], "shared.nif", "shared_f.nif"),
        );
        assert_eq!(
            resolve_armor_meshes(&armor, Gender::Male, nord_race, &idx, GameKind::Skyrim),
            vec!["shared.nif"]
        );
    }

    /// A single-region item still yields exactly one mesh — the common case
    /// must not change shape.
    #[test]
    fn skyrim_single_region_armor_still_yields_one_mesh() {
        let nord_race = 0x0001_3746;
        let armor = skyrim_armor(vec![0xB1]);
        let mut idx = empty_index();
        idx.armor_addons.insert(
            0xB1,
            arma_for_race(0xB1, nord_race, vec![], "cuirass_m.nif", "cuirass_f.nif"),
        );
        assert_eq!(
            resolve_armor_meshes(&armor, Gender::Male, nord_race, &idx, GameKind::Skyrim),
            vec!["cuirass_m.nif"]
        );
    }

    /// The no-race-match fallback stays single-valued: without a race match
    /// there is no evidence the remaining addons are meant for this actor,
    /// and returning all of them would stack unrelated meshes.
    #[test]
    fn skyrim_fallback_without_race_match_stays_single() {
        let armor = skyrim_armor(vec![0xC1, 0xC2]);
        let mut idx = empty_index();
        idx.armor_addons.insert(
            0xC1,
            arma_for_race(0xC1, 0x999, vec![], "first.nif", "f.nif"),
        );
        idx.armor_addons.insert(
            0xC2,
            arma_for_race(0xC2, 0x999, vec![], "second.nif", "s.nif"),
        );
        assert_eq!(
            resolve_armor_meshes(&armor, Gender::Male, 0x0001_3746, &idx, GameKind::Skyrim),
            vec!["first.nif"]
        );
    }

    /// Legacy games have no ARMA dispatch — one record, one mesh.
    #[test]
    fn fnv_yields_at_most_one_mesh() {
        let armor = fnv_armor(r"armor\dressclothes\dressm.nif");
        let idx = EsmIndex {
            game: GameKind::Fallout3NV,
            ..Default::default()
        };
        assert_eq!(
            resolve_armor_meshes(&armor, Gender::Male, 0, &idx, GameKind::Fallout3NV),
            vec![r"armor\dressclothes\dressm.nif"]
        );
        assert!(
            resolve_armor_meshes(&fnv_armor(""), Gender::Male, 0, &idx, GameKind::Fallout3NV)
                .is_empty()
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
            arma_for_race(
                0xAA,
                0x0001_3744, /* Khajiit */
                vec![],
                "beast_m.nif",
                "beast_f.nif",
            ),
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
            arma_for_race(
                0xCC,
                nord_race,
                vec![imperial_race],
                "shared_m.nif",
                "shared_f.nif",
            ),
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
        let path = resolve_armor_mesh(&armor, Gender::Male, unknown_race, &idx, GameKind::Skyrim);
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
            vec![
                (1, 0x0044_4444, 1),
                (5, 0x0055_5555, 1),
                (20, 0x0066_6666, 1),
            ],
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

    /// #3217 — TES5 LVLF bit 1 means "calculate for each item in count",
    /// not "Use All". A level-tier ladder with that flag still selects one
    /// highest eligible tier instead of equipping the whole ladder.
    #[test]
    fn expand_leveled_calculate_each_item_still_picks_one_tier() {
        let mut idx = empty_index();
        add_armo(&mut idx, 0x00AA_AAAA);
        add_armo(&mut idx, 0x00BB_BBBB);
        add_armo(&mut idx, 0x00DD_DDDD);
        add_lvli(
            &mut idx,
            0x00CC_CCCC,
            0x03, // calculate-from-all-levels + calculate-for-each-item
            vec![
                (1, 0x00AA_AAAA, 1),
                (4, 0x00BB_BBBB, 1),
                (7, 0x00DD_DDDD, 1),
            ],
        );
        let mut out = Vec::new();
        expand_leveled_form_id(0x00CC_CCCC, 5, &idx, &mut out);
        assert_eq!(out, vec![0x00BB_BBBB]);
    }

    /// #3217 — the real-world shape (`dunIronbindBeemJa`'s outfit) that
    /// motivated this fix: a `flags = 0x03` tier ladder whose entries are
    /// themselves `flags = 0x03` enchant-variant sublists. Treating `0x02`
    /// as multi-pick multiplied the two levels of expansion together
    /// (18 tiers × 5 variants → hundreds of items on one actor); with only
    /// `0x04` triggering multi-pick, both levels single-pick and the whole
    /// outfit slot resolves to exactly one item.
    #[test]
    fn expand_leveled_nested_tier_ladders_do_not_combinatorially_explode() {
        let mut idx = empty_index();
        // Five enchant variants for the tier the actor's level actually
        // reaches (level 4) — a `0x03` sublist, same as the tier itself.
        for variant in 0..5u32 {
            add_armo(&mut idx, 0x00A0_0000 + variant);
        }
        add_lvli(
            &mut idx,
            0x00B0_0000, // tier-4 enchant sublist
            0x03,
            (0..5u32)
                .map(|variant| (1, 0x00A0_0000 + variant, 1))
                .collect(),
        );
        // A second tier the actor's level does NOT reach, to prove the
        // outer ladder still single-picks instead of unioning tiers.
        add_lvli(&mut idx, 0x00C0_0000, 0x03, vec![(1, 0x00A0_0000, 1)]);
        add_lvli(
            &mut idx,
            0x00D0_0000, // outer tier ladder
            0x03,
            vec![(1, 0x00C0_0000, 1), (4, 0x00B0_0000, 1)],
        );
        let mut out = Vec::new();
        expand_leveled_form_id(0x00D0_0000, 5, &idx, &mut out);
        assert_eq!(
            out.len(),
            1,
            "nested 0x03 ladders must single-pick at every level, not multiply: got {out:?}"
        );
    }

    #[test]
    fn expand_leveled_use_all_flag_lands_all_eligible() {
        let mut idx = empty_index();
        add_armo(&mut idx, 0x00CC_CCCC);
        add_armo(&mut idx, 0x00DD_DDDD);
        add_lvli(
            &mut idx,
            0x00EE_EEEE,
            0x04, // TES5 LVLF Use All
            vec![(1, 0x00CC_CCCC, 1), (1, 0x00DD_DDDD, 1)],
        );
        let mut out = Vec::new();
        expand_leveled_form_id(0x00EE_EEEE, 10, &idx, &mut out);
        assert_eq!(out, vec![0x00CC_CCCC, 0x00DD_DDDD]);
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

    // ── TPLT inventory inheritance (FNV PowderGangers / NCRTroopers)

    use crate::esm::records::actor::{NpcInventoryEntry, NpcRecord};

    fn npc_with(form_id: u32, edid: &str) -> NpcRecord {
        NpcRecord {
            form_id,
            editor_id: edid.to_string(),
            full_name: String::new(),
            model_path: String::new(),
            race_form_id: 0,
            class_form_id: 0,
            voice_form_id: 0,
            factions: Vec::new(),
            inventory: Vec::new(),
            default_outfit: None,
            ai_packages: Vec::new(),
            death_item_form_id: 0,
            level: 1,
            disposition_base: 50,
            acbs_flags: 0,
            has_script: false,
            script_form_id: 0,
            script_instance: None,
            face_morphs: None,
            runtime_facegen: None,
            template_form_id: 0,
            template_flags: 0,
            ..Default::default()
        }
    }

    #[test]
    fn no_template_passes_through_own_inventory() {
        let mut npc = npc_with(0x0010_0001, "BaseNPC");
        npc.inventory.push(NpcInventoryEntry {
            item_form_id: 0xAAAA,
            count: 1,
        });
        let idx = empty_index();
        let inv = resolve_inherited_inventory(&npc, 1, &idx);
        assert_eq!(inv.len(), 1);
        assert_eq!(inv[0].item_form_id, 0xAAAA);
    }

    #[test]
    fn template_flag_clear_passes_through_own_inventory() {
        let mut npc = npc_with(0x0010_0002, "Lvl");
        npc.template_form_id = 0x0010_0003;
        npc.template_flags = 0; // Use Inventory bit NOT set
        let mut base = npc_with(0x0010_0003, "Base");
        base.inventory.push(NpcInventoryEntry {
            item_form_id: 0xBBBB,
            count: 1,
        });
        let mut idx = empty_index();
        idx.npcs.insert(base.form_id, base);

        let inv = resolve_inherited_inventory(&npc, 1, &idx);
        assert!(inv.is_empty(), "no Use-Inventory bit → keep own (empty)");
    }

    #[test]
    fn template_npc_inherits_inventory_through_use_inventory_bit() {
        // The canonical FNV Lvl* → base NPC case. PowderGangers
        // author no CNTO and rely on TPLT + 0x0100.
        let mut npc = npc_with(0x0010_0010, "LvlGoodspringsPowderGanger");
        npc.template_form_id = 0x0010_0011;
        npc.template_flags = TEMPLATE_FLAG_USE_INVENTORY;
        let mut base = npc_with(0x0010_0011, "BasePowderGanger");
        base.inventory.push(NpcInventoryEntry {
            item_form_id: 0x000A_4730, // PowderGang armor 03
            count: 1,
        });
        let mut idx = empty_index();
        idx.npcs.insert(base.form_id, base);

        let inv = resolve_inherited_inventory(&npc, 1, &idx);
        assert_eq!(inv.len(), 1);
        assert_eq!(inv[0].item_form_id, 0x000A_4730);
    }

    #[test]
    fn template_lvln_picks_highest_eligible_variant() {
        // TPLT → LVLN → NPC_ variant. Pick rule mirrors LVLI
        // (highest level ≤ actor_level).
        let mut npc = npc_with(0x0010_0020, "LvlSomething");
        npc.template_form_id = 0x0010_0021;
        npc.template_flags = TEMPLATE_FLAG_USE_INVENTORY;
        let mut low = npc_with(0x0010_0022, "LowVariant");
        low.inventory.push(NpcInventoryEntry {
            item_form_id: 0xC0FFEE,
            count: 1,
        });
        let mut high = npc_with(0x0010_0023, "HighVariant");
        high.inventory.push(NpcInventoryEntry {
            item_form_id: 0xBADF00D,
            count: 1,
        });
        let mut idx = empty_index();
        idx.npcs.insert(low.form_id, low);
        idx.npcs.insert(high.form_id, high);
        // LVLN with two variants — actor_level=10 sees both, picks
        // the higher-level one (level=8).
        idx.leveled_npcs.insert(
            0x0010_0021,
            crate::esm::records::container::LeveledList {
                form_id: 0x0010_0021,
                editor_id: String::new(),
                chance_none: 0,
                flags: 0,
                entries: vec![
                    crate::esm::records::container::LeveledEntry {
                        level: 1,
                        form_id: 0x0010_0022,
                        count: 1,
                    },
                    crate::esm::records::container::LeveledEntry {
                        level: 8,
                        form_id: 0x0010_0023,
                        count: 1,
                    },
                ],
            },
        );

        let inv = resolve_inherited_inventory(&npc, 10, &idx);
        assert_eq!(inv.len(), 1);
        assert_eq!(
            inv[0].item_form_id, 0xBADF00D,
            "highest-eligible LVLN variant wins"
        );
    }

    #[test]
    fn template_chain_breaks_on_cycle_via_depth_cap() {
        // A → B → A self-cycle via TPLT. Must terminate at the cap
        // and fall back to the leaf's own inventory rather than
        // stack-overflowing.
        let mut a = npc_with(0x0010_0030, "A");
        a.template_form_id = 0x0010_0031;
        a.template_flags = TEMPLATE_FLAG_USE_INVENTORY;
        let mut b = npc_with(0x0010_0031, "B");
        b.template_form_id = 0x0010_0030;
        b.template_flags = TEMPLATE_FLAG_USE_INVENTORY;
        b.inventory.push(NpcInventoryEntry {
            item_form_id: 0xDEAD,
            count: 1,
        });
        let mut idx = empty_index();
        idx.npcs.insert(b.form_id, b);

        let _inv = resolve_inherited_inventory(&a, 1, &idx);
        // No panic, no overflow. The exact returned slice depends on
        // the cap-depth parity (odd → A's empty, even → B's one
        // item); both are acceptable cycle-broken outcomes. Success
        // here is "returned without recursion overrun".
    }

    // ── TPLT stats/traits inheritance (#2956) ──────────────────────────
    //
    // `resolve_inherited_stats`/`resolve_inherited_traits` share
    // `resolve_inherited_record` with `resolve_inherited_inventory` above,
    // so the LVLN-tier-pick and cycle/depth-cap behavior already covered
    // for inventory applies identically here — these tests focus on what's
    // actually new: per-flag gating (`Use Stats` and `Use Traits` are
    // independent bits from `Use Inventory` and from each other).

    #[test]
    fn use_stats_bit_pulls_class_and_level_from_the_template() {
        // The canonical case the issue measures: a Lvl* shell whose own
        // class/level the engine ignores once `Use Stats` is set.
        let mut npc = npc_with(0x0010_0040, "LvlNCRTrooper");
        npc.class_form_id = 0xBAD_C1A55; // shell's own — must be ignored
        npc.level = 1;
        npc.template_form_id = 0x0010_0041;
        npc.template_flags = TEMPLATE_FLAG_USE_STATS;
        let mut base = npc_with(0x0010_0041, "BaseNCRTrooper");
        base.class_form_id = 0x000C_1A55;
        base.level = 12;
        let mut idx = empty_index();
        idx.npcs.insert(base.form_id, base);

        let resolved = resolve_inherited_stats(&npc, 1, &idx);
        assert_eq!(resolved.class_form_id, 0x000C_1A55);
        assert_eq!(resolved.level, 12);
    }

    #[test]
    fn use_stats_bit_clear_keeps_the_npcs_own_class() {
        let mut npc = npc_with(0x0010_0050, "Unique");
        npc.class_form_id = 0x0000_C1A5;
        npc.template_form_id = 0x0010_0051;
        npc.template_flags = 0; // Use Stats NOT set
        let mut base = npc_with(0x0010_0051, "Base");
        base.class_form_id = 0xBAD_C1A55;
        let mut idx = empty_index();
        idx.npcs.insert(base.form_id, base);

        let resolved = resolve_inherited_stats(&npc, 1, &idx);
        assert_eq!(
            resolved.class_form_id, 0x0000_C1A5,
            "no Use-Stats bit → keep own class"
        );
    }

    #[test]
    fn use_traits_bit_pulls_race_from_the_template_independent_of_stats() {
        // #2956's other named flag: Use Traits (race) is set/cleared
        // independently of Use Stats (class/level) on the same NPC.
        let mut npc = npc_with(0x0010_0060, "LvlRaider");
        npc.race_form_id = 0xBAD_2ACE;
        npc.class_form_id = 0x0000_C1A5; // own class, kept: Use Stats not set
        npc.template_form_id = 0x0010_0061;
        npc.template_flags = TEMPLATE_FLAG_USE_TRAITS; // Use Stats NOT set
        let mut base = npc_with(0x0010_0061, "BaseRaider");
        base.race_form_id = 0x0000_2ACE;
        base.class_form_id = 0xBAD_C1A55;
        let mut idx = empty_index();
        idx.npcs.insert(base.form_id, base);

        let traits = resolve_inherited_traits(&npc, 1, &idx);
        assert_eq!(
            traits.race_form_id, 0x0000_2ACE,
            "Use Traits → template race"
        );

        let stats = resolve_inherited_stats(&npc, 1, &idx);
        assert_eq!(
            stats.class_form_id, 0x0000_C1A5,
            "Use Stats not set on this NPC → own class, unaffected by Use Traits"
        );
    }
}
