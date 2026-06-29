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
//! / Unarmed Damage are actor-general.

use super::derived::{DerivedInput, DerivedStatFormula};
use super::leveling::LevelingModel;
use super::ruleset::CharacterRuleset;

#[inline]
fn av(form_id: u32) -> DerivedInput {
    DerivedInput::actor_value(form_id)
}

const LEVEL: DerivedInput = DerivedInput::LEVEL;

/// The four derived stats shared verbatim by FO3 and FNV (all actor-general).
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
    // Critical Chance = 0.01·Luck, capped at 0.10.
    if let (Some(out), Some(l)) = (resolve("CritChance"), resolve("Luck")) {
        rs.push_derived(out, DerivedStatFormula::affine(av(l), 0.01, 0.0).capped(0.10));
    }
    // Unarmed Damage = ceil((10 + Unarmed)/20) = ceil(0.5 + 0.05·Unarmed).
    if let (Some(out), Some(u)) = (resolve("UnarmedDamage"), resolve("Unarmed")) {
        rs.push_derived(out, DerivedStatFormula::affine(av(u), 0.05, 0.5).ceiled());
    }
}

/// FO4 — SPECIAL-only (no skills). Health/AP player-only (NPCs ship baked
/// `DNAM`); Carry Weight / Melee Damage actor-general.
pub fn fallout4_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F) -> CharacterRuleset {
    let mut rs = CharacterRuleset::new(LevelingModel::FO4);
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
    // Melee Damage = ×(1 + 0.1·STR).
    if let (Some(out), Some(s)) = (resolve("MeleeDamage"), strength) {
        rs.push_derived(
            out,
            DerivedStatFormula::affine(av(s), 0.1, 1.0).as_multiplier(),
        );
    }
    rs
}

/// FO3 — Health `90 + 20·END + 10·L` (player), AP `65 + 2·AGI` cap 85
/// (player), + the shared skill-based stats.
pub fn fallout3_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F) -> CharacterRuleset {
    let mut rs = CharacterRuleset::new(LevelingModel::FO3);
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
                .player_only(),
        );
    }
    add_fnv_fo3_shared(&mut rs, &resolve);
    rs
}

/// FNV — Health `95 + 20·END + 5·L` (player), AP `65 + 3·AGI` cap 95
/// (player), + the shared skill-based stats.
pub fn falloutnv_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F) -> CharacterRuleset {
    let mut rs = CharacterRuleset::new(LevelingModel::FNV);
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
            _ => return None,
        })
    }

    #[test]
    fn fo4_ruleset_evaluates_and_scopes() {
        let rs = fallout4_ruleset(full);
        assert_eq!(rs.derived_len(), 4, "Health, AP, CarryWeight, MeleeDamage");
        let avs = ActorValues::from_pairs([(0x05, 7.0), (0x07, 5.0), (0x0A, 6.0)]);
        assert_eq!(rs.derived_value(0x2C9, &avs, 1), Some(105.0)); // Health floor(105)
        assert_eq!(rs.derived_value(0x2D0, &avs, 1), Some(120.0)); // AP 60 + 10·6
        assert_eq!(rs.derived_value(0x2D1, &avs, 1), Some(270.0)); // CW 200 + 10·7
        // Scopes: Health player-only; Carry Weight actor-general.
        assert_eq!(rs.derived_formula(0x2C9).unwrap().scope, DerivedScope::PlayerOnly);
        assert_eq!(rs.derived_formula(0x2D1).unwrap().scope, DerivedScope::ActorGeneral);
    }

    #[test]
    fn fnv_and_fo3_share_skill_stats_but_differ_on_health_ap() {
        let fnv = falloutnv_ruleset(full);
        let fo3 = fallout3_ruleset(full);
        assert_eq!(fnv.derived_len(), 6);
        assert_eq!(fo3.derived_len(), 6);
        let avs = ActorValues::from_pairs([
            (0x07, 5.0), // END
            (0x0A, 5.0), // AGI
            (0x05, 5.0), // STR
            (0x0B, 5.0), // Luck
            (0x2C, 90.0), // Unarmed skill
        ]);
        // Health: FO3 90+100+10 = 200; FNV 95+100+5 = 200 (different formulas, same here).
        assert_eq!(fo3.derived_value(0x2C9, &avs, 1), Some(200.0));
        assert_eq!(fnv.derived_value(0x2C9, &avs, 1), Some(200.0));
        // AP differs: FO3 65+2·5 = 75; FNV 65+3·5 = 80.
        assert_eq!(fo3.derived_value(0x2D0, &avs, 1), Some(75.0));
        assert_eq!(fnv.derived_value(0x2D0, &avs, 1), Some(80.0));
        // Shared stats identical: Unarmed Damage ceil(0.5+4.5)=5; Crit 0.05.
        assert_eq!(fnv.derived_value(0x2D4, &avs, 1), Some(5.0));
        assert_eq!(fo3.derived_value(0x2D4, &avs, 1), Some(5.0));
        assert!((fo3.derived_value(0x2D3, &avs, 1).unwrap() - 0.05).abs() < 1e-6);
    }

    #[test]
    fn unresolved_editor_ids_are_skipped() {
        // A resolver missing Strength → Carry Weight + Melee Damage skipped.
        let partial = |id: &str| match id {
            "Strength" => None,
            other => full(other),
        };
        let rs = fallout4_ruleset(partial);
        assert_eq!(rs.derived_len(), 2, "only Health + AP resolved");
    }
}
