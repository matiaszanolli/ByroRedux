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

use super::derived::{DerivedInput, DerivedStatFormula};

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
    let term = |av: u32| DerivedStatFormula::affine(DerivedInput::actor_value(av), 1.0, 0.0).player_only();
    [
        term(strength_av),
        term(willpower_av),
        term(agility_av),
        term(endurance_av),
    ]
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
        let mut rs = CharacterRuleset::new(LevelingModel::default());
        for f in oblivion_fatigue_formulas(STR, WIL, AGI, END) {
            rs.push_derived(FATIGUE, f);
        }

        let avs = ActorValues::from_pairs([(STR, 40.0), (WIL, 30.0), (AGI, 35.0), (END, 45.0)]);
        // 40 + 30 + 35 + 45 = 150 — the sum of all four rows.
        assert_eq!(rs.derived_value(FATIGUE, &avs, 1), Some(150.0));
        // A stat with no rows is still None.
        assert_eq!(rs.derived_value(0xDEAD, &avs, 1), None);
    }
}
