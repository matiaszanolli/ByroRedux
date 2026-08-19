//! Fallout-family [`CharacterRuleset`] builders (CHARAL).
//!
//! Encode the locked per-game derived-stat formulas — the coefficients are
//! engine-supplied constants (`docs/engine/charal-fo4-ruleset.md` +
//! `charal-fnv-fo3-ruleset.md`) — and resolve each stat's input/output AVIF
//! EditorIDs through `resolve`, which the loader backs with
//! `EsmIndex::actor_value_form_id`. A stat whose EditorID doesn't resolve is
//! **skipped** — the same resolve-or-skip contract as the population path, so
//! a game missing an AVIF degrades gracefully rather than panicking.
//!
//! The formula *shape* is code (engine knowledge); the FormIDs are AUTHORED
//! (resolved per load). Health / Action Points are flagged
//! [`player_only`](DerivedStatFormula::player_only) — NPCs ship baked values
//! or derive them differently; Carry Weight / Melee Damage / Critical Chance
//! / Unarmed Damage are actor-general. That justification is sourced for
//! Health (every game) and for FO4/FO76 Action Points (NPCs there read a
//! baked `DNAM` value) — but **not** for FO3/FNV Action Points, which is
//! `player_only` as a conservative, unsourced choice (#2937): the capture
//! document locks the FO3/FNV AP *formula* but never states its scope, and
//! FO4's "NPCs ship baked values" evidence is FO4-specific. See
//! [`fallout3_ruleset`]'s AP row for the reasoning.
//!
//! FO3/FNV attach both the [`AttributeSet::FALLOUT`] SPECIAL roster and their
//! distinct [`SkillSet::FALLOUT3`] / [`SkillSet::FALLOUT_NV`] skill roster +
//! governing-SPECIAL map — the single source the population path
//! (`crates/plugin/.../actor_value_derive.rs`) now consumes for auto-calc base
//! values. FO4/FO76 genuinely have no skills, so they keep the empty roster.

use super::attribute::AttributeSet;
use super::derived::{DerivedInput, DerivedStatFormula};
use super::leveling::LevelingModel;
use super::resistance::Affliction;
use super::ruleset::CharacterRuleset;
use super::skill::SkillSet;

#[inline]
fn av(form_id: u32) -> DerivedInput {
    DerivedInput::actor_value(form_id)
}

const LEVEL: DerivedInput = DerivedInput::LEVEL;

/// The six derived stats shared verbatim by FO3 and FNV (all actor-general).
fn add_fnv_fo3_shared<F: Fn(&str) -> Option<u32>>(rs: &mut CharacterRuleset, resolve: &F) {
    let strength = resolve("Strength");
    // Carry Weight = 150 + 10·STR.
    if let (Some(out), Some(s)) = (resolve("CarryWeight"), strength) {
        rs.push_derived(out, DerivedStatFormula::affine(av(s), 10.0, 150.0));
    }
    // Melee Damage = 0.5·STR (additive bonus).
    if let (Some(out), Some(s)) = (resolve("MeleeDamage"), strength) {
        rs.push_derived(out, DerivedStatFormula::affine(av(s), 0.5, 0.0));
    }
    // Critical Chance = Luck·1 %, capped at 10 %. #2936 — on the 0–100
    // percentage scale, not the 0.0–1.0 fraction the source's "×1%" phrasing
    // could also read as. The Affliction resistances below use the same
    // 0–100 scale (`damage_multiplier`'s `/100.0` fixes it as CHARAL's one
    // percentage convention, see `derived.rs`'s module docs) — Critical
    // Chance used to be the one row transcribed as a fraction instead
    // (`0.01·Luck` capped `0.10`), silently 100x off any percentage-scale
    // reader with no way to tell from the table which convention applied.
    if let (Some(out), Some(l)) = (resolve("CritChance"), resolve("Luck")) {
        rs.push_derived(
            out,
            DerivedStatFormula::affine(av(l), 1.0, 0.0).capped(10.0),
        );
    }
    // Unarmed Damage = ceil((10 + Unarmed)/20) = ceil(0.5 + 0.05·Unarmed).
    if let (Some(out), Some(u)) = (resolve("UnarmedDamage"), resolve("Unarmed")) {
        rs.push_derived(out, DerivedStatFormula::affine(av(u), 0.05, 0.5).ceiled());
    }
    // Affliction-family resistances — FO3/FNV derived percentages
    // `(governing − 1)·k` (Radiation k=2 cap 85 %, Poison k=5 uncapped). The
    // coefficients live in the `Affliction` descriptors (single source); armor
    // / chems / perks layer on via the AV mods, not the base formula.
    for aff in Affliction::ALL {
        if let (Some(out), Some(gov)) = (
            resolve(aff.resist_editor_id),
            resolve(aff.governing_editor_id),
        ) {
            rs.push_derived(out, aff.fo3_fnv_resistance_formula(gov));
        }
    }
}

/// FO4 — SPECIAL-only (no skills). Health/AP player-only (NPCs ship baked
/// `DNAM`); Carry Weight actor-general. The Strength melee multiplier is a
/// combat-use formula rather than an AVIF-backed derived row: vanilla FO4
/// authors no `MeleeDamage` AVIF to key such a row.
pub fn fallout4_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F) -> CharacterRuleset {
    let mut rs = CharacterRuleset::new(LevelingModel::FO4).with_attributes(AttributeSet::FALLOUT);
    let strength = resolve("Strength");
    // Health = floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END).
    if let (Some(out), Some(e)) = (resolve("Health"), resolve("Endurance")) {
        rs.push_derived(
            out,
            DerivedStatFormula::bilinear(av(e), 4.5, LEVEL, 2.5, 0.5, 77.5)
                .floored()
                .player_only(),
        );
    }
    // Action Points = 60 + 10·AGI.
    if let (Some(out), Some(a)) = (resolve("ActionPoints"), resolve("Agility")) {
        rs.push_derived(
            out,
            DerivedStatFormula::affine(av(a), 10.0, 60.0).player_only(),
        );
    }
    // Carry Weight = 200 + 10·STR.
    if let (Some(out), Some(s)) = (resolve("CarryWeight"), strength) {
        rs.push_derived(out, DerivedStatFormula::affine(av(s), 10.0, 200.0));
    }
    rs
}

/// FO3 — Health `90 + 20·END + 10·L` (player, sourced), AP `65 + 2·AGI` cap
/// 85 (player — **scope unsourced**, see the `player_only()` call site),
/// + the shared skill-based stats.
pub fn fallout3_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F) -> CharacterRuleset {
    let mut rs = CharacterRuleset::new(LevelingModel::FO3)
        .with_attributes(AttributeSet::FALLOUT)
        .with_skills(SkillSet::FALLOUT3);
    if let (Some(out), Some(e)) = (resolve("Health"), resolve("Endurance")) {
        rs.push_derived(
            out,
            DerivedStatFormula::bilinear(av(e), 20.0, LEVEL, 10.0, 0.0, 90.0).player_only(),
        );
    }
    if let (Some(out), Some(a)) = (resolve("ActionPoints"), resolve("Agility")) {
        rs.push_derived(
            out,
            DerivedStatFormula::affine(av(a), 2.0, 65.0)
                .capped(85.0)
                // #2937 — `player_only()` here is UNSOURCED, unlike Health's
                // above. `charal-fnv-fo3-ruleset.md`'s derived-stat table
                // annotates scope on every other locked row (Health
                // "(player)", Carry Weight "(actor-general)") but gives
                // Action Points none; its prose only ties the formula to the
                // `fAVDActionPoints{Base,Mult}` GMST family FO3/FNV share
                // with FO4/FO76, never states NPC scope. FO4's own
                // `player_only()` above IS sourced (NPCs there read a baked
                // `DNAM` AP value) — that evidence is FO4-specific and does
                // not carry over. Kept player_only as the conservative
                // choice (an absent AV reads 0.0, so this never
                // over-computes an NPC's AP) rather than guessing
                // ActorGeneral off the `fAVD`-prefix heuristic with no
                // citation backing it for these two games specifically.
                .player_only(),
        );
    }
    add_fnv_fo3_shared(&mut rs, &resolve);
    rs
}

/// FNV — Health `95 + 20·END + 5·L` (player, sourced), AP `65 + 3·AGI` cap
/// 95 (player — **scope unsourced**, see [`fallout3_ruleset`]'s AP row for
/// why), + the shared skill-based stats.
pub fn falloutnv_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F) -> CharacterRuleset {
    let mut rs = CharacterRuleset::new(LevelingModel::FNV)
        .with_attributes(AttributeSet::FALLOUT)
        .with_skills(SkillSet::FALLOUT_NV);
    if let (Some(out), Some(e)) = (resolve("Health"), resolve("Endurance")) {
        rs.push_derived(
            out,
            DerivedStatFormula::bilinear(av(e), 20.0, LEVEL, 5.0, 0.0, 95.0).player_only(),
        );
    }
    if let (Some(out), Some(a)) = (resolve("ActionPoints"), resolve("Agility")) {
        rs.push_derived(
            out,
            DerivedStatFormula::affine(av(a), 3.0, 65.0)
                .capped(95.0)
                // #2937 — see fallout3_ruleset's AP row: scope unsourced,
                // chosen conservatively.
                .player_only(),
        );
    }
    add_fnv_fo3_shared(&mut rs, &resolve);
    rs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::DerivedScope;
    use crate::ecs::components::ActorValues;

    /// Stand-in EditorID → FormID resolver covering every AV the builders ask
    /// for (what the loader gets from the parsed AVIF set).
    fn full(id: &str) -> Option<u32> {
        Some(match id {
            "Strength" => 0x05,
            "Endurance" => 0x07,
            "Agility" => 0x0A,
            "Luck" => 0x0B,
            "Unarmed" => 0x2C,
            "Health" => 0x2C9,
            "ActionPoints" => 0x2D0,
            "CarryWeight" => 0x2D1,
            "MeleeDamage" => 0x2D2,
            "CritChance" => 0x2D3,
            "UnarmedDamage" => 0x2D4,
            "RadResist" => 0x2D5,
            "PoisonResist" => 0x2D6,
            _ => return None,
        })
    }

    /// FO4's authored AVIF set is the shared fixture minus `MeleeDamage`.
    fn fo4_full(id: &str) -> Option<u32> {
        (id != "MeleeDamage").then(|| full(id)).flatten()
    }

    #[test]
    fn fo4_ruleset_evaluates_and_scopes() {
        let rs = fallout4_ruleset(fo4_full);
        assert_eq!(
            rs.derived_row_len(),
            3,
            "Health, AP, CarryWeight; FO4 authors no MeleeDamage AVIF"
        );
        let avs = ActorValues::from_pairs([(0x05, 7.0), (0x07, 5.0), (0x0A, 6.0)]);
        assert_eq!(rs.derived_value(0x2C9, &avs, 1), Some(105.0)); // Health floor(105)
        assert_eq!(rs.derived_value(0x2D0, &avs, 1), Some(120.0)); // AP 60 + 10·6
        assert_eq!(rs.derived_value(0x2D1, &avs, 1), Some(270.0)); // CW 200 + 10·7
                                                                   // Scopes: Health player-only; Carry Weight actor-general.
        assert_eq!(
            rs.derived_formula(0x2C9).unwrap().scope,
            DerivedScope::PlayerOnly
        );
        assert_eq!(
            rs.derived_formula(0x2D1).unwrap().scope,
            DerivedScope::ActorGeneral
        );
    }

    /// #2937 — FO3/FNV Action Points is `player_only` as a deliberate but
    /// UNsourced conservative choice (see `fallout3_ruleset`'s AP row and
    /// the module docs), unlike Health's player-only scope, which the
    /// capture document does state. Pin the current, chosen behavior so a
    /// future edit that silently flips it to `ActorGeneral` (or the reverse
    /// on FO4/FO76) is a deliberate, reviewed change, not an accident.
    #[test]
    fn fo3_fnv_action_points_scope_is_player_only_pending_a_source() {
        let fo3 = fallout3_ruleset(full);
        let fnv = falloutnv_ruleset(full);
        assert_eq!(
            fo3.derived_formula(0x2D0).unwrap().scope,
            DerivedScope::PlayerOnly
        );
        assert_eq!(
            fnv.derived_formula(0x2D0).unwrap().scope,
            DerivedScope::PlayerOnly
        );
    }

    #[test]
    fn fnv_and_fo3_share_skill_stats_but_differ_on_health_ap() {
        let fnv = falloutnv_ruleset(full);
        let fo3 = fallout3_ruleset(full);
        assert_eq!(fnv.derived_row_len(), 8);
        assert_eq!(fo3.derived_row_len(), 8);
        let avs = ActorValues::from_pairs([
            (0x07, 5.0),  // END
            (0x0A, 5.0),  // AGI
            (0x05, 5.0),  // STR
            (0x0B, 5.0),  // Luck
            (0x2C, 90.0), // Unarmed skill
        ]);
        // Radiation Resistance = (END−1)·2 → (5−1)·2 = 8 %, identical FO3==FNV.
        assert_eq!(fo3.derived_value(0x2D5, &avs, 1), Some(8.0));
        assert_eq!(fnv.derived_value(0x2D5, &avs, 1), Some(8.0));
        // The 85 % cap clamps at high Endurance: (50−1)·2 = 98 → 85.
        let high_end = ActorValues::from_pairs([(0x07, 50.0)]);
        assert_eq!(fnv.derived_value(0x2D5, &high_end, 1), Some(85.0));
        // Poison Resistance = (END−1)·5 → (5−1)·5 = 20 %, the RadResist twin
        // (uncapped — no documented FO3/FNV cap, so it keeps scaling).
        assert_eq!(fo3.derived_value(0x2D6, &avs, 1), Some(20.0));
        assert_eq!(fnv.derived_value(0x2D6, &high_end, 1), Some(245.0)); // (50−1)·5
                                                                         // Health: FO3 90+100+10 = 200; FNV 95+100+5 = 200 (different formulas, same here).
        assert_eq!(fo3.derived_value(0x2C9, &avs, 1), Some(200.0));
        assert_eq!(fnv.derived_value(0x2C9, &avs, 1), Some(200.0));
        // AP differs: FO3 65+2·5 = 75; FNV 65+3·5 = 80.
        assert_eq!(fo3.derived_value(0x2D0, &avs, 1), Some(75.0));
        assert_eq!(fnv.derived_value(0x2D0, &avs, 1), Some(80.0));
        // Shared stats identical: Unarmed Damage ceil(0.5+4.5)=5; Crit 5%
        // (#2936 — 0–100 scale, matching RadResist/PoisonResist above, not
        // the 0.0–1.0 fraction this used to evaluate to).
        assert_eq!(fnv.derived_value(0x2D4, &avs, 1), Some(5.0));
        assert_eq!(fo3.derived_value(0x2D4, &avs, 1), Some(5.0));
        assert!((fo3.derived_value(0x2D3, &avs, 1).unwrap() - 5.0).abs() < 1e-6);
        assert!((fnv.derived_value(0x2D3, &avs, 1).unwrap() - 5.0).abs() < 1e-6);
        // The 10% cap, on the same scale as RadResist's 85% cap above.
        let high_luck = ActorValues::from_pairs([(0x0B, 50.0)]);
        assert!((fo3.derived_value(0x2D3, &high_luck, 1).unwrap() - 10.0).abs() < 1e-6);
    }

    /// #2936 — Critical Chance and the Affliction resistances (both
    /// percentage-valued stats in the same ruleset) must land on the same
    /// 0–100 scale, not silently disagree by a factor of 100. Regression:
    /// before this fix, Critical Chance evaluated to `0.05` (a fraction)
    /// at Luck 5, a hundredfold off Radiation Resistance's `8.0` (a 0–100
    /// percentage) at a comparable END 5 — with nothing in the table able
    /// to tell a reader which convention applied to which output id.
    #[test]
    fn critical_chance_and_resistances_share_the_same_percentage_scale() {
        let fo3 = fallout3_ruleset(full);
        let avs = ActorValues::from_pairs([(0x0B, 5.0), (0x07, 5.0)]); // Luck 5, END 5
        let crit = fo3.derived_value(0x2D3, &avs, 1).unwrap(); // CritChance
        let rad = fo3.derived_value(0x2D5, &avs, 1).unwrap(); // RadResist

        // Both single-digit percentages on the same 0–100 scale, not
        // `0.05` vs `8.0` two orders of magnitude apart.
        assert_eq!(crit, 5.0);
        assert_eq!(rad, 8.0);
    }

    #[test]
    fn skill_and_attribute_rosters_travel_with_the_ruleset() {
        use crate::character::{AttributeSet, SkillSet};
        let fnv = falloutnv_ruleset(full);
        assert_eq!(fnv.attributes, AttributeSet::FALLOUT);
        assert_eq!(fnv.skills, SkillSet::FALLOUT_NV);
        let fo3 = fallout3_ruleset(full);
        assert_eq!(fo3.attributes, AttributeSet::FALLOUT);
        assert_eq!(fo3.skills, SkillSet::FALLOUT3);
        // FO4 has SPECIAL but no skills (perks replace them).
        let fo4 = fallout4_ruleset(fo4_full);
        assert_eq!(fo4.attributes, AttributeSet::FALLOUT);
        assert!(fo4.skills.is_empty());
    }

    #[test]
    fn unresolved_editor_ids_are_skipped() {
        // A resolver missing Strength → Carry Weight skipped.
        let partial = |id: &str| match id {
            "Strength" => None,
            other => fo4_full(other),
        };
        let rs = fallout4_ruleset(partial);
        assert_eq!(rs.derived_row_len(), 2, "only Health + AP resolved");
    }
}
