//! Elder Scrolls-family derived pools (CHARAL).
//!
//! The classic TES titles (Morrowind / Oblivion) derive the three status
//! pools — Health, Magicka, Fatigue — from the 8 attributes, mirroring the
//! Fallout derived-stat pattern in [`super::fallout`]. Formula *shapes* are
//! engine-supplied constants; the attribute AVIF FormIDs are AUTHORED and
//! passed in resolved (the same contract as the Fallout builders).
//!
//! **Oblivion, sourced (no guessing):**
//! * Health  = `2 × Endurance`     — Elder Scrolls Wiki, *Health (Oblivion)*.
//! * Magicka = `2 × Intelligence`  — Elder Scrolls Wiki, *Magicka (Oblivion)*.
//! * Fatigue = `Strength + Willpower + Agility + Endurance` — UESP,
//!   *Oblivion:Fatigue* (the vanilla sum; the `NewFormula*` game-setting
//!   variant is a non-default alternative).
//!
//! Health's per-level accrual (≈10 % of Endurance each level) is **not** part
//! of the base formula — it is the deferred TES leveling-efficiency mechanic
//! (`docs/engine/charal.md` §5), a leveling concern, not a derived pool.
//!
//! Fatigue is a four-attribute sum, which the two-input [`DerivedStatFormula`]
//! cannot hold in one row. It is expressed as **four affine rows** registered
//! under one output id and summed by
//! [`CharacterRuleset::derived_value`](super::ruleset::CharacterRuleset::derived_value)
//! — see [`oblivion_fatigue_formulas`]. The rows are uncapped/unrounded so the
//! sum is the true total.

use super::attribute::AttributeSet;
use super::derived::{DerivedInput, DerivedStatFormula};
use super::leveling::LevelingModel;
use super::ruleset::CharacterRuleset;
use super::skill::SkillSet;

/// Oblivion Health = `2 × Endurance` (starting/base pool; per-level accrual is
/// a leveling concern). Player-scoped, matching the Fallout convention where
/// NPCs ship baked pool values. `endurance_av` is the resolved Endurance AVIF
/// FormID.
#[must_use]
pub fn oblivion_health_formula(endurance_av: u32) -> DerivedStatFormula {
    DerivedStatFormula::affine(DerivedInput::actor_value(endurance_av), 2.0, 0.0).player_only()
}

/// Oblivion Magicka = `2 × Intelligence`. Player-scoped (NPCs bake it).
/// `intelligence_av` is the resolved Intelligence AVIF FormID.
#[must_use]
pub fn oblivion_magicka_formula(intelligence_av: u32) -> DerivedStatFormula {
    DerivedStatFormula::affine(DerivedInput::actor_value(intelligence_av), 2.0, 0.0).player_only()
}

/// Oblivion Fatigue = `Strength + Willpower + Agility + Endurance` (UESP).
///
/// Returned as the four affine rows (coefficient 1.0 each, uncapped,
/// player-scoped) to register under a single Fatigue output id; their sum via
/// [`CharacterRuleset::derived_value`](super::ruleset::CharacterRuleset::derived_value)
/// is the pool. Arguments are the resolved AVIF FormIDs of the four governing
/// attributes.
#[must_use]
pub fn oblivion_fatigue_formulas(
    strength_av: u32,
    willpower_av: u32,
    agility_av: u32,
    endurance_av: u32,
) -> [DerivedStatFormula; 4] {
    let term =
        |av: u32| DerivedStatFormula::affine(DerivedInput::actor_value(av), 1.0, 0.0).player_only();
    [
        term(strength_av),
        term(willpower_av),
        term(agility_av),
        term(endurance_av),
    ]
}

/// Oblivion (TES IV) [`CharacterRuleset`] builder (CHARAL).
///
/// Assembles the family's canonical pieces: the 8-attribute roster
/// ([`AttributeSet::TES_CLASSIC`]), the 21-skill roster + governing map
/// ([`SkillSet::OBLIVION`]), the skill-use leveling model
/// ([`LevelingModel::OBLIVION`]), and the three derived pools (Health / Magicka
/// / Fatigue). Inputs/outputs are resolved through `resolve` (EditorID →
/// FormID) with the same resolve-or-skip contract as the Fallout builders
/// ([`super::fallout`]) — a pool whose EditorIDs don't resolve is simply
/// omitted. Pre-AVIF Oblivion supplies a resolver mapping these EditorIDs to
/// its legacy engine actor-value indices at the parser boundary.
#[must_use]
pub fn oblivion_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F) -> CharacterRuleset {
    let mut rs = CharacterRuleset::new(LevelingModel::OBLIVION)
        .with_attributes(AttributeSet::TES_CLASSIC)
        .with_skills(SkillSet::OBLIVION);

    // Health = 2·Endurance.
    if let (Some(out), Some(end)) = (resolve("Health"), resolve("Endurance")) {
        rs.push_derived(out, oblivion_health_formula(end));
    }
    // Magicka = 2·Intelligence.
    if let (Some(out), Some(int)) = (resolve("Magicka"), resolve("Intelligence")) {
        rs.push_derived(out, oblivion_magicka_formula(int));
    }
    // Fatigue = Strength + Willpower + Agility + Endurance (four summed rows).
    if let (Some(out), Some(s), Some(w), Some(a), Some(e)) = (
        resolve("Fatigue"),
        resolve("Strength"),
        resolve("Willpower"),
        resolve("Agility"),
        resolve("Endurance"),
    ) {
        for row in oblivion_fatigue_formulas(s, w, a, e) {
            rs.push_derived(out, row);
        }
    }
    rs
}

/// Oblivion level-up **attribute bonus** — the classic-TES leveling-efficiency
/// mechanic (`docs/engine/charal.md` §5).
///
/// When the character gains a level they may raise up to three attributes; the
/// bonus each raised attribute receives is tiered by how many increases landed
/// in that attribute's *governed* skills since the previous level (the
/// skill→attribute map is [`SkillSet::OBLIVION`]). Tiers, UESP
/// *Oblivion:Leveling*:
///
/// | governed skill-ups | bonus |
/// |--------------------|-------|
/// | 0                  | +1    |
/// | 1–4                | +2    |
/// | 5–7                | +3    |
/// | 8–9                | +4    |
/// | 10+                | +5    |
///
/// Capped at +5; surplus skill-ups past 10 do **not** roll over to the next
/// level. `governed_skill_ups` is the count for one attribute; the caller
/// tallies increases per governing attribute (via [`SkillSet::governing`]).
#[must_use]
pub fn oblivion_attribute_bonus(governed_skill_ups: u16) -> u8 {
    match governed_skill_ups {
        0 => 1,
        1..=4 => 2,
        5..=7 => 3,
        8..=9 => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::DerivedScope;
    use crate::ecs::components::ActorValues;

    // Resolved stand-in AVIF FormIDs.
    const END: u32 = 0x07;
    const INT: u32 = 0x09;

    #[test]
    fn health_is_twice_endurance() {
        let f = oblivion_health_formula(END);
        let avs = ActorValues::from_pairs([(END, 40.0)]);
        // 2 × 40 = 80, level-independent.
        assert_eq!(f.eval(&avs, 1), 80.0);
        assert_eq!(f.eval(&avs, 20), 80.0, "base pool has no level term");
        assert_eq!(f.scope, DerivedScope::PlayerOnly);
    }

    #[test]
    fn magicka_is_twice_intelligence() {
        let f = oblivion_magicka_formula(INT);
        let avs = ActorValues::from_pairs([(INT, 50.0)]);
        assert_eq!(f.eval(&avs, 1), 100.0);
        assert_eq!(f.scope, DerivedScope::PlayerOnly);
    }

    #[test]
    fn fatigue_sums_four_attributes_via_ruleset() {
        use crate::character::{CharacterRuleset, LevelingModel};
        const STR: u32 = 0x05;
        const WIL: u32 = 0x08; // stand-in Willpower AVIF id
        const AGI: u32 = 0x0A;
        const FATIGUE: u32 = 0x2E0; // stand-in output id

        // Register the four Fatigue rows under one output id.
        let mut rs = CharacterRuleset::new(LevelingModel::OBLIVION);
        for f in oblivion_fatigue_formulas(STR, WIL, AGI, END) {
            rs.push_derived(FATIGUE, f);
        }

        let avs = ActorValues::from_pairs([(STR, 40.0), (WIL, 30.0), (AGI, 35.0), (END, 45.0)]);
        // 40 + 30 + 35 + 45 = 150 — the sum of all four rows.
        assert_eq!(rs.derived_value(FATIGUE, &avs, 1), Some(150.0));
        // A stat with no rows is still None.
        assert_eq!(rs.derived_value(0xDEAD, &avs, 1), None);
    }

    #[test]
    fn oblivion_ruleset_assembles_and_evaluates_end_to_end() {
        use crate::character::{AttributeSet, LevelingModel, SkillSet};
        use super::oblivion_ruleset;

        // Resolver: the pool outputs + the attribute inputs the builder asks
        // for (what the Oblivion loader supplies from legacy AV indices).
        // Non-zero ids throughout: FormID 0 is the null form and also
        // `DerivedInput::UNUSED`, so a real AV never resolves to it.
        let resolve = |id: &str| -> Option<u32> {
            Some(match id {
                "Strength" => 0x1F,
                "Intelligence" => 0x20,
                "Willpower" => 0x21,
                "Agility" => 0x22,
                "Endurance" => 0x23,
                "Health" => 0x90,
                "Magicka" => 0x92,
                "Fatigue" => 0x93,
                _ => return None,
            })
        };
        let rs = oblivion_ruleset(resolve);

        // Canonical rosters travel with the ruleset.
        assert_eq!(rs.attributes, AttributeSet::TES_CLASSIC);
        assert_eq!(rs.skills, SkillSet::OBLIVION);
        assert_eq!(rs.leveling, LevelingModel::OBLIVION);

        let avs = ActorValues::from_pairs([
            (0x1F, 40.0), // STR
            (0x20, 50.0), // INT
            (0x21, 30.0), // WIL
            (0x22, 35.0), // AGI
            (0x23, 45.0), // END
        ]);
        assert_eq!(rs.derived_value(0x90, &avs, 1), Some(90.0)); // Health 2·END
        assert_eq!(rs.derived_value(0x92, &avs, 1), Some(100.0)); // Magicka 2·INT
        assert_eq!(rs.derived_value(0x93, &avs, 1), Some(150.0)); // Fatigue STR+WIL+AGI+END
    }

    #[test]
    fn oblivion_ruleset_skips_unresolved_pools() {
        use super::oblivion_ruleset;
        // Resolver knows the attributes but NOT the Magicka output id.
        let resolve = |id: &str| -> Option<u32> {
            Some(match id {
                "Strength" => 0x00,
                "Intelligence" => 0x01,
                "Willpower" => 0x02,
                "Agility" => 0x03,
                "Endurance" => 0x05,
                "Health" => 0x90,
                // "Magicka" and "Fatigue" intentionally absent.
                _ => return None,
            })
        };
        let rs = oblivion_ruleset(resolve);
        let avs = ActorValues::from_pairs([(0x01, 50.0), (0x05, 45.0)]);
        assert_eq!(rs.derived_value(0x90, &avs, 1), Some(90.0)); // Health present
        assert_eq!(rs.derived_value(0x92, &avs, 1), None); // Magicka skipped
    }

    #[test]
    fn attribute_bonus_tiers_match_uesp() {
        use super::oblivion_attribute_bonus;
        assert_eq!(oblivion_attribute_bonus(0), 1);
        // 1–4 → +2
        assert_eq!(oblivion_attribute_bonus(1), 2);
        assert_eq!(oblivion_attribute_bonus(4), 2);
        // 5–7 → +3
        assert_eq!(oblivion_attribute_bonus(5), 3);
        assert_eq!(oblivion_attribute_bonus(7), 3);
        // 8–9 → +4
        assert_eq!(oblivion_attribute_bonus(8), 4);
        assert_eq!(oblivion_attribute_bonus(9), 4);
        // 10+ → +5, capped (no roll-over)
        assert_eq!(oblivion_attribute_bonus(10), 5);
        assert_eq!(oblivion_attribute_bonus(30), 5);
    }

    #[test]
    fn attribute_bonus_composes_with_the_governing_map() {
        use super::oblivion_attribute_bonus;
        use crate::character::SkillSet;
        // Tally skill-ups per governing attribute, then derive each bonus.
        // Say the character raised Blade ×3 and Blunt ×2 (both Strength) and
        // Sneak ×6 (Agility) this level.
        let mut str_ups = 0u16;
        let mut agi_ups = 0u16;
        for (skill, count) in [("Blade", 3u16), ("Blunt", 2), ("Sneak", 6)] {
            match SkillSet::OBLIVION.governing(skill) {
                Some(crate::character::Attribute::Strength) => str_ups += count,
                Some(crate::character::Attribute::Agility) => agi_ups += count,
                _ => {}
            }
        }
        assert_eq!(str_ups, 5);
        assert_eq!(agi_ups, 6);
        assert_eq!(oblivion_attribute_bonus(str_ups), 3); // 5 → +3
        assert_eq!(oblivion_attribute_bonus(agi_ups), 3); // 6 → +3
    }
}
