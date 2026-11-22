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
//! of the base formula — it is a leveling concern, not a derived pool, and is
//! implemented separately below as [`oblivion_health_gain_per_level`]
//! (`docs/engine/charal.md` §5).
//!
//! Fatigue is a four-attribute sum, which the two-input [`DerivedStatFormula`]
//! cannot hold in one row. It is expressed as **four affine rows** registered
//! under one output id and summed by
//! [`CharacterRuleset::derived_value`](super::ruleset::CharacterRuleset::derived_value)
//! — see [`oblivion_fatigue_formulas`]. The rows are uncapped/unrounded so the
//! sum is the true total.

use super::attribute::AttributeSet;
use crate::ecs::components::ActorValues;
use super::derived::{DerivedInput, DerivedStatFormula};
use super::leveling::LevelingModel;
use super::ruleset::CharacterRuleset;
use super::skill::SkillSet;

/// Oblivion Health = `2 × Endurance` (starting/base pool; per-level accrual is
/// a leveling concern). `endurance_av` is the resolved Endurance AVIF FormID.
///
/// **Scope is UNsourced (#6431).** `player_only` is an analogy to the Fallout
/// convention where NPCs ship baked pool values — and that convention is itself
/// FO4-evidence-specific. `charal-oblivion-ruleset.md` states no player/NPC
/// scope for any pool formula. Pinned by
/// `oblivion_pool_rows_are_player_only_pending_a_source`; the choice is
/// observable since #7010 wired the ruleset, so revisit it when a source
/// states the scope.
///
/// #7198 — the coefficient is `fPCBaseHealthMult` (`base_health_mult`; CS wiki
/// *FPCBaseHealthMult*, "multiplied by the player's endurance to calculate
/// starting health", default [`OBLIVION_PC_BASE_HEALTH_MULT`]), and the result
/// is truncated (`trunc(End x fPCBaseHealthMult)`,
/// `charal-oblivion-ruleset.md`, the player's pools); truncation is `floor` on a
/// non-negative attribute.
#[must_use]
pub fn oblivion_health_formula(endurance_av: u32, base_health_mult: f32) -> DerivedStatFormula {
    DerivedStatFormula::affine(DerivedInput::actor_value(endurance_av), base_health_mult, 0.0).floored().player_only()
}

/// Oblivion Magicka = `trunc(Intelligence × (1 + fPCBaseMagickaMult))`, `2 ×
/// Intelligence` at `Oblivion.esm`'s authored 1.0 (#7198;
/// `base_magicka_mult` is the setting, CS wiki *FPCBaseMagickaMult*: "Used to
/// calculate the player's magicka from the player's intelligence"). Scope is
/// `player_only` but UNsourced — see [`oblivion_health_formula`] (#6431). The
/// capture's Magicka Multiplier factor (actor value 40, `/ 10`) is a declared
/// gap: no derived row can carry it (`charal-oblivion-ruleset.md`, Known gaps).
#[must_use]
pub fn oblivion_magicka_formula(intelligence_av: u32, base_magicka_mult: f32) -> DerivedStatFormula {
    DerivedStatFormula::affine(DerivedInput::actor_value(intelligence_av), 1.0 + base_magicka_mult, 0.0)
        .floored()
        .player_only()
}

/// `fPCBaseHealthMult`'s default (CS wiki *FPCBaseHealthMult*, 2.0; `Oblivion.esm` does not author it).
pub const OBLIVION_PC_BASE_HEALTH_MULT: f32 = 2.0;
/// `fPCBaseMagickaMult`'s default (`Oblivion.esm` authors 1.0).
pub const OBLIVION_PC_BASE_MAGICKA_MULT: f32 = 1.0;

/// Oblivion Fatigue = `Strength + Willpower + Agility + Endurance` (UESP).
///
/// Returned as the four affine rows (coefficient 1.0 each, uncapped,
/// `player_only` — scope UNsourced, see [`oblivion_health_formula`], #6431) to register under a single Fatigue output id; their sum via
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
/// omitted. Oblivion authors no `AVIF` records, so the loader installs the
/// engine-supplied 72-value roster (`EsmIndex::install_oblivion_actor_values`,
/// #7010) and these EditorIDs resolve against it like any other game's.
#[must_use]
pub fn oblivion_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F) -> CharacterRuleset {
    oblivion_ruleset_with_gmst(resolve, |_| None)
}

/// [`oblivion_ruleset`] with the load order's `fPCBaseHealthMult` and
/// `fPCBaseMagickaMult` overlaid on the player's pool rows (#7198, the #6756
/// overlay the Fallout builders carry). A non-finite or negative authored value
/// keeps its default.
pub fn oblivion_ruleset_with_gmst<F: Fn(&str) -> Option<u32>, G: Fn(&str) -> Option<f32>>(
    resolve: F,
    gmst: G,
) -> CharacterRuleset {
    let setting = |name: &str, default: f32| gmst(name).filter(|value| value.is_finite() && *value >= 0.0).unwrap_or(default);
    let health_mult = setting("fPCBaseHealthMult", OBLIVION_PC_BASE_HEALTH_MULT);
    let magicka_mult = setting("fPCBaseMagickaMult", OBLIVION_PC_BASE_MAGICKA_MULT);
    let mut rs = CharacterRuleset::new(LevelingModel::OBLIVION)
        .with_attributes(AttributeSet::TES_CLASSIC)
        .with_skills(SkillSet::OBLIVION);

    // Health = 2·Endurance.
    if let (Some(out), Some(end)) = (resolve("Health"), resolve("Endurance")) {
        rs.push_derived(out, oblivion_health_formula(end, health_mult));
    }
    // Magicka = 2·Intelligence.
    if let (Some(out), Some(int)) = (resolve("Magicka"), resolve("Intelligence")) {
        rs.push_derived(out, oblivion_magicka_formula(int, magicka_mult));
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
    // #7201 — no armor-rating rows: the engine-supplied roster has no `*ArmorRating` value for them to key on,
    // and Oblivion's armor rating is the combat-time `ArmorRatingModel` (`armor_rating.rs`, the profile's
    // `armor_rating` row), the one home of UESP's `0.35 + 0.0065 x ArmorSkill`.
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
/// Capped at the tenth band; surplus skill-ups past 10 do **not** roll over to
/// the next level. `governed_skill_ups` is the count for one attribute; the
/// caller tallies increases per governing attribute (via
/// [`SkillSet::governing`]). `bonuses` is the band for a tally of 1 to 10, the
/// `iLevelUp01Mult` … `iLevelUp10Mult` settings (#7196, [`OBLIVION_ATTRIBUTE_BONUSES`]
/// by default; a load order retunes them through
/// [`LevelingModel::with_gmst`](super::LevelingModel::with_gmst)); a tally of 0
/// is the engine's +1, which no setting covers.
#[must_use]
pub fn oblivion_attribute_bonus(governed_skill_ups: u16, bonuses: &[u8; 10]) -> u8 {
    match governed_skill_ups {
        0 => 1,
        n => bonuses[usize::from(n.min(10)) - 1],
    }
}

/// `iLevelUp01Mult` … `iLevelUp10Mult`: the attribute bonus at a governed tally of 1 … 10 (CS wiki
/// *ILevelUpXXMult*, defaults 2, 2, 2, 2, 3, 3, 3, 4, 4, 5; `Oblivion.esm` authors none of them).
pub const OBLIVION_ATTRIBUTE_BONUSES: [u8; 10] = [2, 2, 2, 2, 3, 3, 3, 4, 4, 5];

/// `fStatsHealthLevelMult`: the share of Endurance a level-up adds to Health (CS wiki *FStatsHealthLevelMult*,
/// 0.1, "rounded down"; `Oblivion.esm` does not author it).
pub const OBLIVION_HEALTH_PER_ENDURANCE: f32 = 0.1;

/// Oblivion **per-level Health gain** — the stateful half of the Health pool
/// (leveling-efficiency mechanic, `docs/engine/charal.md` §5).
///
/// Health starts at `2 × Endurance` ([`oblivion_health_formula`]) and then, at
/// **each level-up**, permanently gains **10 % of the current Endurance**
/// (UESP *Oblivion:Leveling* — "10 HP if your Endurance is at 100"). Because it
/// accrues on the Endurance *at each level* and is stored, it is
/// path-dependent — a per-level event, not a stateless derived formula. This is
/// why maximising Endurance early matters: every level compounds it.
///
/// Returned as an exact `f32` (`per_endurance × endurance`, `fStatsHealthLevelMult`,
/// [`OBLIVION_HEALTH_PER_ENDURANCE`] by default, #7196); integer HP rounding is
/// the leveling system's concern, as with [`LevelingModel::skill_points`].
///
/// **Scope:** this is *classic* Oblivion (2006, Gamebryo — ByroRedux's
/// target). The live UESP *Oblivion:Health* page now documents the 2024
/// *Oblivion Remastered* (UE5) formula (Strength + `Endurance × 0.6666 × 2`);
/// that engine is out of scope — do not adopt it here.
#[must_use]
pub fn oblivion_health_gain_per_level(endurance: u16, per_endurance: f32) -> f32 {
    per_endurance * f32::from(endurance)
}

/// Apply one Oblivion level-up's Health gain to the player's [`ActorValues`] (#7044): the whole-number part of
/// [`oblivion_health_gain_per_level`] goes into the Health actor value's [`ActorValue::stored`](crate::ecs::components::ActorValue::stored) layer, the term
/// the original keeps beside the formula (`(stored + derived) x multiplier`, `Oblivion.exe` `0x5ead00`; the
/// level-up adds to it at `0x66c3e0`). It is the layer the per-frame derived-stat refresh and the permanent
/// (ability and gear) modifiers both leave alone, so the gain survives a refresh and a Fortify Health ability
/// coming and going. Returns the gain applied.
///
/// UESP *Oblivion:Health*: "you gain health points equal to a tenth of your endurance, rounded down"
/// (Endurance 98 gives +9). Which Endurance the executable reads, before or after the level's attribute bonus,
/// is unverified (the skill-use leveling PRD's V8): the caller passes the value it settles on.
pub fn apply_oblivion_level_up_health(values: &mut ActorValues, health: u32, endurance: u16, per_endurance: f32) -> f32 {
    let gain = oblivion_health_gain_per_level(endurance, per_endurance).floor();
    values.add_stored(health, gain);
    gain
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::DerivedScope;
    use crate::ecs::components::ActorValues;

    // Resolved stand-in AVIF FormIDs.
    const END: u32 = 0x07;
    const INT: u32 = 0x09;
    const HEALTH: u32 = 0x08;

    /// #7044 — the whole-number tenth of Endurance lands in the stored layer and nowhere else.
    #[test]
    fn the_level_up_health_gain_rounds_down_into_the_stored_layer() {
        let mut values = ActorValues::from_pairs([(HEALTH, 80.0), (END, 98.0)]);
        // UESP *Oblivion:Health*: Endurance 98 at level-up gives +9, rounded down.
        assert_eq!(apply_oblivion_level_up_health(&mut values, HEALTH, 98, OBLIVION_HEALTH_PER_ENDURANCE), 9.0);
        assert_eq!(apply_oblivion_level_up_health(&mut values, HEALTH, 45, OBLIVION_HEALTH_PER_ENDURANCE), 4.0);
        let health = values.get(HEALTH).unwrap();
        assert_eq!((health.stored, health.base, health.permanent_mod, health.set_override), (13.0, 80.0, 0.0, 0.0));
        assert_eq!(values.current(HEALTH), 93.0);
        assert_eq!(values.stored(HEALTH), 13.0);
    }

    #[test]
    fn health_is_twice_endurance() {
        let f = oblivion_health_formula(END, OBLIVION_PC_BASE_HEALTH_MULT);
        let avs = ActorValues::from_pairs([(END, 40.0)]);
        // 2 × 40 = 80, level-independent.
        assert_eq!(f.eval(&avs, 1), 80.0);
        assert_eq!(f.eval(&avs, 20), 80.0, "base pool has no level term");
        assert_eq!(f.scope, DerivedScope::PlayerOnly);
    }

    /// #7198 — the player's pool coefficients are the load order's `fPCBaseHealthMult` and `fPCBaseMagickaMult`
    /// (Magicka's slope is `1 + mult`), truncated; a non-finite or negative value keeps the default.
    #[test]
    fn the_player_pool_coefficients_follow_the_load_orders_settings() {
        let resolve = |name: &str| match name {
            "Health" => Some(0x90),
            "Magicka" => Some(0x91),
            "Endurance" => Some(END),
            "Intelligence" => Some(INT),
            _ => None,
        };
        let avs = ActorValues::from_pairs([(END, 41.0), (INT, 41.0)]);
        let tuned = oblivion_ruleset_with_gmst(resolve, |name| match name {
            "fPCBaseHealthMult" => Some(1.5),
            "fPCBaseMagickaMult" => Some(0.5),
            _ => None,
        });
        assert_eq!(tuned.derived_value(0x90, &avs, 1), Some(61.0), "trunc(41 x 1.5) = trunc(61.5)");
        assert_eq!(tuned.derived_value(0x91, &avs, 1), Some(61.0), "trunc(41 x (1 + 0.5))");
        let vanilla = oblivion_ruleset(resolve);
        assert_eq!((vanilla.derived_value(0x90, &avs, 1), vanilla.derived_value(0x91, &avs, 1)), (Some(82.0), Some(82.0)));
        for bad in [f32::NAN, -1.0] {
            let kept = oblivion_ruleset_with_gmst(resolve, move |name: &str| name.starts_with("fPCBase").then_some(bad));
            assert_eq!(kept.derived_value(0x91, &avs, 1), Some(82.0), "{bad}");
        }
    }

    #[test]
    fn magicka_is_twice_intelligence() {
        let f = oblivion_magicka_formula(INT, OBLIVION_PC_BASE_MAGICKA_MULT);
        let avs = ActorValues::from_pairs([(INT, 50.0)]);
        assert_eq!(f.eval(&avs, 1), 100.0);
        assert_eq!(f.scope, DerivedScope::PlayerOnly);
    }

    /// #6431 — Oblivion's Health / Magicka / Fatigue rows are `player_only` as
    /// an UNsourced analogy to the Fallout convention (the capture states no
    /// pool scope; contrast the sourced-silent `ActorGeneral` Armor Rating
    /// rows). Pin the chosen scopes so a silent flip — either way — is a
    /// reviewed edit that also updates the capture's "Pool scope" note.
    #[test]
    fn oblivion_pool_rows_are_player_only_pending_a_source() {
        for row in oblivion_fatigue_formulas(1, 2, 3, 4) {
            assert_eq!(row.scope, DerivedScope::PlayerOnly);
        }
        // Health and Magicka are pinned beside their formulas above.
        let rs = oblivion_ruleset(|id| {
            Some(match id {
                "Strength" => 0x1F,
                "Intelligence" => 0x20,
                "Willpower" => 0x21,
                "Agility" => 0x22,
                "Endurance" => 0x23,
                "Health" => 0x90,
                "Magicka" => 0x92,
                "Fatigue" => 0x93,
                "LightArmor" => 0x24,
                "HeavyArmor" => 0x25,
                _ => return None,
            })
        });
        for pool in [0x90, 0x92, 0x93] {
            assert_eq!(
                rs.derived_formula(pool).unwrap().scope,
                DerivedScope::PlayerOnly,
                "{pool:#04X}: pool scope is unsourced (#6431)"
            );
        }
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
        use super::oblivion_ruleset;
        use crate::character::{AttributeSet, LevelingModel, SkillSet};

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
                "LightArmor" => 0x24,
                "HeavyArmor" => 0x25,
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
            (0x24, 50.0), // Light Armor skill
            (0x25, 20.0), // Heavy Armor skill
        ]);
        assert_eq!(rs.derived_value(0x90, &avs, 1), Some(90.0)); // Health 2·END
        assert_eq!(rs.derived_value(0x92, &avs, 1), Some(100.0)); // Magicka 2·INT
        assert_eq!(rs.derived_value(0x93, &avs, 1), Some(150.0)); // Fatigue STR+WIL+AGI+END
        // #7201 — no armor-rating row: the armor rating is `ArmorRatingModel`'s, even when a resolver answers the
        // names (the engine roster never does).
        assert_eq!(rs.derived_row_len(), 6, "Health, Magicka and four Fatigue rows");
    }

    #[test]
    fn oblivion_ruleset_skips_unresolved_pools() {
        use super::oblivion_ruleset;
        // Resolver knows the attributes but NOT the Magicka output id.
        let resolve = |id: &str| -> Option<u32> {
            Some(match id {
                "Strength" => 0x10,
                "Intelligence" => 0x11,
                "Willpower" => 0x12,
                "Agility" => 0x13,
                "Endurance" => 0x15,
                "Health" => 0x90,
                // "Magicka" and "Fatigue" intentionally absent.
                _ => return None,
            })
        };
        let rs = oblivion_ruleset(resolve);
        let avs = ActorValues::from_pairs([(0x11, 50.0), (0x15, 45.0)]);
        assert_eq!(rs.derived_value(0x90, &avs, 1), Some(90.0)); // Health present
        assert_eq!(rs.derived_value(0x92, &avs, 1), None); // Magicka skipped
    }

    #[test]
    fn attribute_bonus_tiers_match_uesp() {
        use super::oblivion_attribute_bonus;
        assert_eq!(oblivion_attribute_bonus(0, &OBLIVION_ATTRIBUTE_BONUSES), 1);
        // 1–4 → +2
        assert_eq!(oblivion_attribute_bonus(1, &OBLIVION_ATTRIBUTE_BONUSES), 2);
        assert_eq!(oblivion_attribute_bonus(4, &OBLIVION_ATTRIBUTE_BONUSES), 2);
        // 5–7 → +3
        assert_eq!(oblivion_attribute_bonus(5, &OBLIVION_ATTRIBUTE_BONUSES), 3);
        assert_eq!(oblivion_attribute_bonus(7, &OBLIVION_ATTRIBUTE_BONUSES), 3);
        // 8–9 → +4
        assert_eq!(oblivion_attribute_bonus(8, &OBLIVION_ATTRIBUTE_BONUSES), 4);
        assert_eq!(oblivion_attribute_bonus(9, &OBLIVION_ATTRIBUTE_BONUSES), 4);
        // 10+ → +5, capped (no roll-over)
        assert_eq!(oblivion_attribute_bonus(10, &OBLIVION_ATTRIBUTE_BONUSES), 5);
        assert_eq!(oblivion_attribute_bonus(30, &OBLIVION_ATTRIBUTE_BONUSES), 5);
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
        assert_eq!(oblivion_attribute_bonus(str_ups, &OBLIVION_ATTRIBUTE_BONUSES), 3); // 5 → +3
        assert_eq!(oblivion_attribute_bonus(agi_ups, &OBLIVION_ATTRIBUTE_BONUSES), 3); // 6 → +3
    }

    #[test]
    fn health_gain_per_level_is_ten_percent_of_endurance() {
        use super::oblivion_health_gain_per_level;
        assert_eq!(oblivion_health_gain_per_level(100, OBLIVION_HEALTH_PER_ENDURANCE), 10.0); // UESP anchor
        assert_eq!(oblivion_health_gain_per_level(45, OBLIVION_HEALTH_PER_ENDURANCE), 4.5); // fractional — caller rounds
        assert_eq!(oblivion_health_gain_per_level(0, OBLIVION_HEALTH_PER_ENDURANCE), 0.0);
    }

    #[test]
    fn health_base_plus_accrual_composes() {
        use super::{oblivion_health_formula, oblivion_health_gain_per_level};
        // Constant Endurance 50: base 2·50 = 100, then +5/level.
        let base = {
            let f = oblivion_health_formula(END, OBLIVION_PC_BASE_HEALTH_MULT);
            f.eval(&ActorValues::from_pairs([(END, 50.0)]), 1)
        };
        assert_eq!(base, 100.0);
        // Reaching level 3 accrues two per-level gains (levels 2 and 3).
        let total = base + 2.0 * oblivion_health_gain_per_level(50, OBLIVION_HEALTH_PER_ENDURANCE);
        assert_eq!(total, 110.0);
    }
}
