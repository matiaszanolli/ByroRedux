//! Oblivion's spell-cast rules: what a cast costs, which tier it needs, which school it feeds, and the
//! Fatigue it spends (player-spellcasting PRD `docs/prd/player-spellcasting.md` PSC-CST-003..005, PSC-CST-008,
//! PSC-USE-003; decoded in `docs/engine/charal-oblivion-ruleset.md`, "Spell casting", #7078).
//!
//! Pure data and functions: no ECS, no game branches. The host supplies each effect's authored numbers and
//! the caster's skills; the per-game seam is the [`CharacterRulesProfile`](super::CharacterRulesProfile)
//! `spell_cast` row (`Some` on Oblivion), and the load order's `GMST` records retune the model
//! ([`SpellCastModel::with_gmst`]), as [`ArmorModel`](super::ArmorModel) does.
//!
//! ```text
//! effect cost    = 0.1 * base * D * A * M * T            D = duration (1 if 0)   A = max(1, area * 0.15)
//!                                                        M = magnitude ^ 1.28 (1 if 0)   T = 1.5 for a Target effect
//! base cost      = sum floor(effect cost), at least 1 each      (the stored SPIT cost when Manual Spell Cost)
//! actual cost    = sum floor(effect cost * modifier(its school's skill)), at least 1 each
//!                  (Manual Spell Cost: stored cost * modifier(primary school), not rounded)
//! modifier(skill)= costBase + costMult * (1 - S / 100)    S = clamp(skill + 0.4 * Luck - 20, 0, 100)
//! ```

use crate::ecs::resource::Resource;

/// `MGEF` flag bit 7 (`0x80`): the effect has no duration, so its cost reads 1 for `D` (xEdit TES4 `MGEF` flags).
pub const MGEF_NO_DURATION: u32 = 0x80;
/// `MGEF` flag bit 8 (`0x100`): no magnitude, so the cost reads 1 for `M`.
pub const MGEF_NO_MAGNITUDE: u32 = 0x100;
/// `MGEF` flag bit 9 (`0x200`): no area, so the cost reads 0 for the area.
pub const MGEF_NO_AREA: u32 = 0x200;
/// `MGEF` flag bit 22, unnamed in xEdit: the executable leaves such effects out of the cost sum and of the
/// primary-school choice (`0x4151c0`, `0x4152c0`, #7078).
pub const MGEF_COST_SKIPPED: u32 = 1 << 22;

/// An effect's range: where it lands (`EFIT` `Type`, xEdit `wbEffectTypeEnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectRange {
    SelfRange,
    Touch,
    Target,
}

impl EffectRange {
    /// The `EFIT` `Type` value, `None` outside 0 to 2.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::SelfRange),
            1 => Some(Self::Touch),
            2 => Some(Self::Target),
            _ => None,
        }
    }

    /// The label the cast reports use.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SelfRange => "Self",
            Self::Touch => "Touch",
            Self::Target => "Target",
        }
    }
}

/// What the cost and the school need to know about one effect of a spell, as authored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectCostInput {
    /// The `MGEF` base cost (`DATA` @4).
    pub base_cost: f32,
    pub magnitude: f32,
    /// Seconds; 0 is instant.
    pub duration: f32,
    /// Feet.
    pub area: f32,
    pub range: EffectRange,
    /// The `MGEF` flags (`DATA` @0).
    pub mgef_flags: u32,
    /// The effect's school, 0 (Alteration) to 5 (Restoration): its `SCIT` school for a script effect, else its
    /// `MGEF` school. `None` for school 6 (None).
    pub school: Option<u8>,
}

/// The tier a spell requires, Novice (0) to Master (4).
pub type Tier = u8;

/// The model's settings: each is a game setting, the defaults are `Oblivion.exe`'s and `Oblivion.esm`'s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpellCastModel {
    /// `fMagicCasterSkillCostBase`.
    pub skill_cost_base: f32,
    /// `fMagicCasterSkillCostMult`.
    pub skill_cost_mult: f32,
    /// `fMagicCostScale`: the exponent on the magnitude.
    pub magnitude_exponent: f32,
    /// `fMagicDurMagBaseCostMult`: scales the MGEF base cost.
    pub base_cost_scale: f32,
    /// `fMagicAreaBaseCostMult`: scales the area in feet.
    pub area_scale: f32,
    /// `fMagicRangeTargetCostMult`: a Target effect's multiplier.
    pub target_multiplier: f32,
    /// `fActorLuckSkillMult`: Luck's weight on the skill the modifier reads.
    pub luck_skill_mult: f32,
    /// `iActorLuckSkillBase`: the offset added with it (so Luck 50 adds nothing).
    pub luck_skill_base: f32,
    /// `fMagicSpellLevelCostMult` and `fMagicSpellLevelCostBase`: the affine map from a base cost to the value the
    /// tier thresholds read.
    pub level_cost_mult: f32,
    pub level_cost_base: f32,
    /// `fMagicSpellLevelApprenticeMin`, `JourneymanMin`, `ExpertMin`, `MasterMin`: the base-cost thresholds.
    pub spell_level_min: [f32; 4],
    /// `iSkillApprenticeMin`, `JourneymanMin`, `ExpertMin`, `MasterMin`: the skill that reaches each player tier.
    pub skill_tier_min: [f32; 4],
    /// `fFatigueCastBase` and `fFatigueCastMult`: the Fatigue a cast costs, base plus the multiple of its Magicka.
    pub fatigue_cast_base: f32,
    pub fatigue_cast_mult: f32,
}

impl Resource for SpellCastModel {}

impl SpellCastModel {
    /// Oblivion's: `Oblivion.exe`'s defaults where the executable was decoded, and `Oblivion.esm`'s authored values
    /// where the master authors the setting (`fMagicCasterSkillCostBase` 0.2, `fMagicCostScale` 1.28, the four
    /// `fMagicSpellLevel*Min`, `fFatigueCastBase` 1.0). `fMagicDurMagBaseCostMult` 0.1, `fMagicAreaBaseCostMult` 0.15
    /// and `fMagicRangeTargetCostMult` 1.5 are the CS wiki's defaults.
    pub const OBLIVION: Self = Self {
        skill_cost_base: 0.2,
        skill_cost_mult: 1.2,
        magnitude_exponent: 1.28,
        base_cost_scale: 0.1,
        area_scale: 0.15,
        target_multiplier: 1.5,
        luck_skill_mult: 0.4,
        luck_skill_base: -20.0,
        level_cost_mult: 1.0,
        level_cost_base: 0.0,
        spell_level_min: [25.5, 62.5, 149.5, 399.5],
        skill_tier_min: [25.0, 50.0, 75.0, 100.0],
        fatigue_cast_base: 1.0,
        fatigue_cast_mult: 0.0,
    };

    /// The model with each setting the load order authors replacing its default. A non-finite authored value is ignored.
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        let authored = |name: &str, default: f32| gmst(name).filter(|value| value.is_finite()).unwrap_or(default);
        Self {
            skill_cost_base: authored("fMagicCasterSkillCostBase", self.skill_cost_base),
            skill_cost_mult: authored("fMagicCasterSkillCostMult", self.skill_cost_mult),
            magnitude_exponent: authored("fMagicCostScale", self.magnitude_exponent),
            base_cost_scale: authored("fMagicDurMagBaseCostMult", self.base_cost_scale),
            area_scale: authored("fMagicAreaBaseCostMult", self.area_scale),
            target_multiplier: authored("fMagicRangeTargetCostMult", self.target_multiplier),
            luck_skill_mult: authored("fActorLuckSkillMult", self.luck_skill_mult),
            luck_skill_base: authored("iActorLuckSkillBase", self.luck_skill_base),
            level_cost_mult: authored("fMagicSpellLevelCostMult", self.level_cost_mult),
            level_cost_base: authored("fMagicSpellLevelCostBase", self.level_cost_base),
            spell_level_min: [
                authored("fMagicSpellLevelApprenticeMin", self.spell_level_min[0]),
                authored("fMagicSpellLevelJourneymanMin", self.spell_level_min[1]),
                authored("fMagicSpellLevelExpertMin", self.spell_level_min[2]),
                authored("fMagicSpellLevelMasterMin", self.spell_level_min[3]),
            ],
            skill_tier_min: [
                authored("iSkillApprenticeMin", self.skill_tier_min[0]),
                authored("iSkillJourneymanMin", self.skill_tier_min[1]),
                authored("iSkillExpertMin", self.skill_tier_min[2]),
                authored("iSkillMasterMin", self.skill_tier_min[3]),
            ],
            fatigue_cast_base: authored("fFatigueCastBase", self.fatigue_cast_base),
            fatigue_cast_mult: authored("fFatigueCastMult", self.fatigue_cast_mult),
        }
    }

    /// One effect's cost before the caster's skill adjusts it, a float (`0x548b50`). The MGEF flags zero the magnitude,
    /// duration and area, and a Self effect has no area.
    #[must_use]
    pub fn effect_cost(&self, effect: &EffectCostInput) -> f32 {
        let duration = if effect.mgef_flags & MGEF_NO_DURATION != 0 { 0.0 } else { effect.duration };
        let magnitude = if effect.mgef_flags & MGEF_NO_MAGNITUDE != 0 { 0.0 } else { effect.magnitude };
        let area = if effect.mgef_flags & MGEF_NO_AREA != 0 || effect.range == EffectRange::SelfRange { 0.0 } else { effect.area };
        let d = if duration == 0.0 { 1.0 } else { duration };
        let a = (area * self.area_scale).max(1.0);
        let m = if magnitude == 0.0 { 1.0 } else { magnitude.powf(self.magnitude_exponent) };
        let t = if effect.range == EffectRange::Target { self.target_multiplier } else { 1.0 };
        self.base_cost_scale * effect.base_cost * d * a * m * t
    }

    /// The cost multiplier for a caster whose skill in the effect's school is `skill` and whose Luck is `luck`
    /// (`0x548c00`, `0x547b90`).
    #[must_use]
    pub fn modifier(&self, skill: f32, luck: f32) -> f32 {
        let s = (skill + self.luck_skill_mult * luck + self.luck_skill_base).clamp(0.0, 100.0);
        self.skill_cost_base + self.skill_cost_mult * (1.0 - s / 100.0)
    }

    /// A whole-number effect cost of at least 1: the executable floors each effect (`0x4138fa`).
    fn whole(cost: f32) -> f32 {
        cost.floor().max(1.0)
    }

    /// The spell's base cost: the sum of its effects' floored costs (those the executable counts), or the stored `SPIT`
    /// cost for a Manual Spell Cost spell (`manual_cost`).
    #[must_use]
    pub fn base_cost(&self, effects: &[EffectCostInput], manual_cost: Option<u32>) -> f32 {
        match manual_cost {
            Some(stored) => stored as f32,
            None => effects
                .iter()
                .filter(|effect| effect.mgef_flags & MGEF_COST_SKIPPED == 0)
                .map(|effect| Self::whole(self.effect_cost(effect)))
                .sum(),
        }
    }

    /// The Magicka a cast of the spell costs a caster whose skill in school `s` is `skill(s)` and whose Luck is `luck`
    /// (`0x41d320`). An auto-calculated spell sums each effect's own modified, floored cost; a Manual Spell Cost spell
    /// is its stored cost times the modifier of its primary school, not rounded. An effect with no school reads skill 0
    /// (not decoded: no vanilla Spell has one).
    #[must_use]
    pub fn actual_cost(&self, effects: &[EffectCostInput], manual_cost: Option<u32>, skill: impl Fn(u8) -> f32, luck: f32) -> f32 {
        let skill_of = |school: Option<u8>| school.map_or(0.0, &skill);
        match manual_cost {
            Some(stored) => stored as f32 * self.modifier(skill_of(self.primary_school(effects)), luck),
            None => effects
                .iter()
                .filter(|effect| effect.mgef_flags & MGEF_COST_SKIPPED == 0)
                .map(|effect| Self::whole(self.effect_cost(effect) * self.modifier(skill_of(effect.school), luck)))
                .sum(),
        }
    }

    /// The tier a spell requires (`0x41d3c0`, `0x548c50`): for an auto-calculated spell, its base cost against the four
    /// `fMagicSpellLevel*Min` thresholds; for a Manual Spell Cost spell, its stored `SPIT` level.
    #[must_use]
    pub fn spell_tier(&self, effects: &[EffectCostInput], manual: Option<(u32, u8)>) -> Tier {
        match manual {
            Some((_, level)) => level.min(4),
            None => {
                let value = self.base_cost(effects, None) * self.level_cost_mult + self.level_cost_base;
                self.spell_level_min.iter().filter(|min| value >= **min).count() as Tier
            }
        }
    }

    /// The player's tier in a school from their unfortified (base) skill (`0x5f23b0`): the number of
    /// `iSkill*Min` thresholds the skill reaches.
    #[must_use]
    pub fn player_tier(&self, base_skill: f32) -> Tier {
        self.skill_tier_min.iter().filter(|min| base_skill >= **min).count() as Tier
    }

    /// The school a cast feeds and whose skill gates a Manual Spell Cost spell (`0x415360`): the school of the effect
    /// with the highest floored caster-less cost, the first of equal ones. `None` when no counted effect carries a school.
    #[must_use]
    pub fn primary_school(&self, effects: &[EffectCostInput]) -> Option<u8> {
        let mut best: Option<(f32, Option<u8>)> = None;
        for effect in effects.iter().filter(|effect| effect.mgef_flags & MGEF_COST_SKIPPED == 0) {
            let cost = Self::whole(self.effect_cost(effect));
            if best.is_none_or(|(top, _)| cost > top) {
                best = Some((cost, effect.school));
            }
        }
        best.and_then(|(_, school)| school)
    }

    /// The Fatigue a cast costs when its Magicka cost is `magicka_cost` (`0x548da0`).
    #[must_use]
    pub fn fatigue_cost(&self, magicka_cost: f32) -> f32 {
        self.fatigue_cast_base + self.fatigue_cast_mult * magicka_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(base_cost: f32, magnitude: f32, duration: f32, area: f32, range: EffectRange, school: u8) -> EffectCostInput {
        EffectCostInput { base_cost, magnitude, duration, area, range, mgef_flags: 0, school: Some(school) }
    }

    /// Flare (`StandardFireDamageTarget1Novice`, 000A97DF): Fire Damage (base cost 7.5, Destruction), 6 pts on Target.
    fn flare() -> EffectCostInput {
        effect(7.5, 6.0, 0.0, 0.0, EffectRange::Target, 2)
    }

    #[test]
    fn flare_costs_what_uesp_lists_not_the_stored_cost() {
        let model = SpellCastModel::OBLIVION;
        let raw = model.effect_cost(&flare());
        assert!((raw - 11.148).abs() < 0.01, "{raw}");
        // UESP lists 11 Magicka (Oblivion/Destruction Spells); the stored SPIT cost is 9.
        assert_eq!(model.base_cost(&[flare()], None), 11.0);
    }

    #[test]
    fn the_skill_modifier_is_the_affine_one_and_floors_each_effect() {
        let model = SpellCastModel::OBLIVION;
        // 1.4 - 0.012 * skill at Luck 50 (which adds nothing).
        assert!((model.modifier(0.0, 50.0) - 1.4).abs() < 1e-6);
        assert!((model.modifier(100.0, 50.0) - 0.2).abs() < 1e-6);
        assert!((model.modifier(33.0, 50.0) - 1.004).abs() < 1e-5);
        let cost = |skill: f32| model.actual_cost(&[flare()], None, |_| skill, 50.0);
        assert_eq!(cost(5.0), 14.0); // 11.148 * 1.34 = 14.94
        assert_eq!(cost(33.0), 11.0);
        assert_eq!(cost(100.0), 2.0); // 11.148 * 0.2 = 2.23
    }

    #[test]
    fn luck_adds_two_skill_points_per_five_above_fifty_and_is_capped() {
        let model = SpellCastModel::OBLIVION;
        // Luck 100: +20 skill.
        assert!((model.modifier(40.0, 100.0) - model.modifier(60.0, 50.0)).abs() < 1e-6);
        // Luck 0: -20 skill, clamped at 0.
        assert!((model.modifier(10.0, 0.0) - model.modifier(0.0, 50.0)).abs() < 1e-6);
        // The skill is held at 100.
        assert!((model.modifier(95.0, 100.0) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn every_effect_costs_at_least_one_and_a_zero_magnitude_or_duration_reads_one() {
        let model = SpellCastModel::OBLIVION;
        let cheap = effect(0.1, 1.0, 0.0, 0.0, EffectRange::SelfRange, 0);
        assert_eq!(model.base_cost(&[cheap, cheap], None), 2.0);
        let no_mag = EffectCostInput { mgef_flags: MGEF_NO_MAGNITUDE | MGEF_NO_DURATION, ..effect(50.0, 99.0, 99.0, 0.0, EffectRange::SelfRange, 0) };
        assert!((model.effect_cost(&no_mag) - 5.0).abs() < 1e-5, "{}", model.effect_cost(&no_mag));
    }

    #[test]
    fn a_self_effect_has_no_area_and_area_is_scaled_by_fifteen_hundredths() {
        let model = SpellCastModel::OBLIVION;
        let self_area = effect(10.0, 1.0, 0.0, 40.0, EffectRange::SelfRange, 0);
        let touch_area = EffectCostInput { range: EffectRange::Touch, ..self_area };
        assert!((model.effect_cost(&self_area) - 1.0).abs() < 1e-6);
        assert!((model.effect_cost(&touch_area) - 6.0).abs() < 1e-5, "{}", model.effect_cost(&touch_area));
    }

    #[test]
    fn a_manual_spell_costs_its_stored_cost_times_the_primary_schools_modifier_unrounded() {
        let model = SpellCastModel::OBLIVION;
        // Mirili's Expert Command (SE37CommandCreature4Expert): stored 368, Illusion, Manual Spell Cost.
        let effects = [effect(8.0, 80.0, 30.0, 0.0, EffectRange::Touch, 3)];
        let cost = model.actual_cost(&effects, Some(368), |school| if school == 3 { 50.0 } else { 0.0 }, 50.0);
        assert!((cost - 368.0 * (1.4 - 0.012 * 50.0)).abs() < 1e-3, "{cost}");
        assert_ne!(cost, cost.floor());
    }

    #[test]
    fn the_tier_of_an_auto_calculated_spell_comes_from_its_base_cost() {
        let model = SpellCastModel::OBLIVION;
        let spell = |cost: f32| [EffectCostInput { base_cost: cost * 10.0 + 0.1, ..effect(1.0, 1.0, 0.0, 0.0, EffectRange::SelfRange, 0) }];
        assert_eq!(model.spell_tier(&spell(25.0), None), 0);
        assert_eq!(model.spell_tier(&spell(26.0), None), 1);
        assert_eq!(model.spell_tier(&spell(62.0), None), 1);
        assert_eq!(model.spell_tier(&spell(63.0), None), 2);
        assert_eq!(model.spell_tier(&spell(149.0), None), 2);
        assert_eq!(model.spell_tier(&spell(150.0), None), 3);
        assert_eq!(model.spell_tier(&spell(399.0), None), 3);
        assert_eq!(model.spell_tier(&spell(400.0), None), 4);
        // Flare is Novice; a Manual Spell Cost spell reads its stored level.
        assert_eq!(model.spell_tier(&[flare()], None), 0);
        assert_eq!(model.spell_tier(&[], Some((368, 3))), 3);
    }

    #[test]
    fn the_players_tier_reaches_each_threshold_exactly() {
        let model = SpellCastModel::OBLIVION;
        let tiers: Vec<Tier> = [0.0, 24.0, 25.0, 49.0, 50.0, 74.0, 75.0, 99.0, 100.0].iter().map(|skill| model.player_tier(*skill)).collect();
        assert_eq!(tiers, [0, 0, 1, 1, 2, 2, 3, 3, 4]);
    }

    #[test]
    fn the_primary_school_is_the_costliest_effects_and_ties_go_to_the_first() {
        let model = SpellCastModel::OBLIVION;
        let cheap_alteration = effect(5.0, 10.0, 0.0, 0.0, EffectRange::SelfRange, 0);
        let costly_illusion = effect(40.0, 20.0, 5.0, 0.0, EffectRange::SelfRange, 3);
        assert_eq!(model.primary_school(&[cheap_alteration, costly_illusion]), Some(3));
        let tie_a = effect(10.0, 10.0, 0.0, 0.0, EffectRange::SelfRange, 1);
        let tie_b = effect(10.0, 10.0, 0.0, 0.0, EffectRange::SelfRange, 5);
        assert_eq!(model.primary_school(&[tie_a, tie_b]), Some(1));
        // A skipped effect never counts, and a spell with no school has none.
        let skipped = EffectCostInput { mgef_flags: MGEF_COST_SKIPPED, ..costly_illusion };
        assert_eq!(model.primary_school(&[cheap_alteration, skipped]), Some(0));
        assert_eq!(model.primary_school(&[EffectCostInput { school: None, ..cheap_alteration }]), None);
    }

    #[test]
    fn a_cast_costs_the_authored_fatigue() {
        let model = SpellCastModel::OBLIVION;
        assert_eq!(model.fatigue_cost(11.0), 1.0);
        let executable_defaults = model.with_gmst(|name| (name == "fFatigueCastBase").then_some(15.0));
        assert_eq!(executable_defaults.fatigue_cost(11.0), 15.0);
        let scaled = model.with_gmst(|name| (name == "fFatigueCastMult").then_some(0.5));
        assert_eq!(scaled.fatigue_cost(10.0), 6.0);
    }

    #[test]
    fn the_load_orders_settings_retune_the_model_and_a_bad_value_is_ignored() {
        let retuned = SpellCastModel::OBLIVION.with_gmst(|name| match name {
            "fMagicCasterSkillCostBase" => Some(0.1),
            "fMagicSpellLevelMasterMin" => Some(f32::NAN),
            "iSkillExpertMin" => Some(70.0),
            _ => None,
        });
        assert_eq!(retuned.skill_cost_base, 0.1);
        assert_eq!(retuned.spell_level_min[3], 399.5);
        assert_eq!(retuned.skill_tier_min[2], 70.0);
    }
}
