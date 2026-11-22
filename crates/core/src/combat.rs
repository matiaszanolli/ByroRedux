//! Classic Oblivion combat-damage math (CHARAL-adjacent, not CHARAL itself).
//!
//! Sibling of [`crate::stealth`], not a submodule of [`crate::character`]:
//! these formulas read SPECIAL/skill actor values as *inputs* but are
//! evaluated at combat-resolution time against transient per-hit state
//! (weapon condition, base weapon damage) that never lives in `ActorValues`
//! — the same "AV in, mechanic-formula out" boundary already drawn for
//! Sneak Detection (`crate::stealth`) and documented for Lockpicking/
//! Acrobatics in `docs/engine/charal-oblivion-ruleset.md`.
//!
//! ## Status: shipped consumer, but not this module's consumer (#2979)
//!
//! A combat/attack-resolution consumer *does* now exist —
//! `byroredux/src/combat.rs`'s `combat_input_system` + `combat_damage_system`
//! (two `Stage::Update` exclusives, `byroredux/src/boot/schedule/update/`), a ray → hit →
//! damage → death melee slice that landed 2026-08-15/16. It does not route
//! through this module: its damage model is `EquippedWeapon.damage` plus the
//! CHARAL `MeleeDamage` bonus (#3092), or the flat `UNARMED_DAMAGE` baseline
//! — nothing here had a caller outside its own tests until the player's bow
//! (#7055): `BowModel`'s Oblivion damage rule carries [`OblivionDamageSettings`]
//! and rates each arrow through it.
//!
//! That is not (yet) a bug: this module is *classic Oblivion* math, and the
//! live slice runs on Skyrim/FNV content, where these formulas do not apply.
//! Whether the shipped consumer should dispatch per-game into this module is
//! the open design question tracked by #2962 and
//! `AUDIT_CHARACTER_2026-08-16` § CHAR-2026-08-16-D1-01 — read the
//! zero-caller state as *unresolved*, not as deliberate.
//!
//! Source: UESP *Oblivion:The Complete Damage Formula*, *Oblivion:Blade*,
//! *Oblivion:Blunt* (`docs/engine/charal-oblivion-ruleset.md`). Scope is
//! **classic Oblivion (2006, Gamebryo)** — Oblivion Remastered's diverging
//! damage/regen math (noted throughout that doc) is out of scope, matching
//! the precedent already set in `character::tes::oblivion_health_gain_per_level`.

/// #7199 — the settings of Oblivion's weapon and Hand-to-Hand damage, each the load order's `GMST` over its
/// documented default (UESP *Oblivion:The Complete Damage Formula*, "Modifiable Settings"; the Luck pair is the CS
/// wiki's *Effective Skill Level*, `fActorLuckSkillMult` / `iActorLuckSkillBase`; the Fatigue pair the CS wiki's
/// *Fatigue Game Settings*, `fFatigueBase` / `fFatigueMult`). `Oblivion.esm` authors most of them, so a rebalance
/// mod retunes them; [`Self::with_gmst`] reads them all, and the spell-cast row takes its Luck defaults from here,
/// so the effective skill has one home.
///
/// ```text
/// ModifiedSkill = Skill + fActorLuckSkillMult * Luck + iActorLuckSkillBase              (0.4, -20: Skill + 0.4 (Luck - 50))
/// WeaponDamage  = Base * fDamageWeaponMult * (fDamageStrengthBase + Attribute / 100 * fDamageStrengthMult)
///                      * (fDamageSkillBase + ModifiedSkill / 100 * fDamageSkillMult)
///                      * (fDamageWeaponConditionBase + Health / BaseHealth * fDamageWeaponConditionMult)
/// FatigueFactor = fFatigueBase - fFatigueMult * (1 - Fatigue / MaxFatigue)                (1, 0.5: (Fatigue / Max + 1) / 2)
/// HandHealth    = fHandHealthMin + (fHandHealthMax - fHandHealthMin)
///                 * (fHandDamageStrengthBase + Strength / 100 * fHandDamageStrengthMult)
///                 * (fHandDamageSkillBase + ModifiedSkill / 100 * fHandDamageSkillMult)
/// HandFatigue   = fHandFatigueDamageBase + HandHealth * fHandFatigueDamageMult
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OblivionDamageSettings {
    /// `fActorLuckSkillMult` (0.4).
    pub luck_skill_mult: f32,
    /// `iActorLuckSkillBase` (-20).
    pub luck_skill_base: f32,
    /// `fDamageWeaponMult` (0.5).
    pub weapon_mult: f32,
    /// `fDamageStrengthBase` (0.75).
    pub strength_base: f32,
    /// `fDamageStrengthMult` (0.5, per 100 points).
    pub strength_mult: f32,
    /// `fDamageSkillBase` (0.2).
    pub skill_base: f32,
    /// `fDamageSkillMult` (1.5, per 100 points).
    pub skill_mult: f32,
    /// `fDamageWeaponConditionBase` (0.5).
    pub condition_base: f32,
    /// `fDamageWeaponConditionMult` (0.5).
    pub condition_mult: f32,
    /// `fFatigueBase` (1.0).
    pub fatigue_base: f32,
    /// `fFatigueMult` (0.5).
    pub fatigue_mult: f32,
    /// `fHandHealthMin` (1).
    pub hand_health_min: f32,
    /// `fHandHealthMax` (15).
    pub hand_health_max: f32,
    /// `fHandDamageStrengthBase` (0).
    pub hand_strength_base: f32,
    /// `fHandDamageStrengthMult` (0.75, per 100 points).
    pub hand_strength_mult: f32,
    /// `fHandDamageSkillBase` (0).
    pub hand_skill_base: f32,
    /// `fHandDamageSkillMult` (1, per 100 points).
    pub hand_skill_mult: f32,
    /// `fHandFatigueDamageBase` (1).
    pub hand_fatigue_base: f32,
    /// `fHandFatigueDamageMult` (0.5).
    pub hand_fatigue_mult: f32,
}

impl OblivionDamageSettings {
    /// The documented defaults (UESP *Oblivion:The Complete Damage Formula*; CS wiki).
    pub const OBLIVION: Self = Self {
        luck_skill_mult: 0.4,
        luck_skill_base: -20.0,
        weapon_mult: 0.5,
        strength_base: 0.75,
        strength_mult: 0.5,
        skill_base: 0.2,
        skill_mult: 1.5,
        condition_base: 0.5,
        condition_mult: 0.5,
        fatigue_base: 1.0,
        fatigue_mult: 0.5,
        hand_health_min: 1.0,
        hand_health_max: 15.0,
        hand_strength_base: 0.0,
        hand_strength_mult: 0.75,
        hand_skill_base: 0.0,
        hand_skill_mult: 1.0,
        hand_fatigue_base: 1.0,
        hand_fatigue_mult: 0.5,
    };

    /// The settings with each one the load order authors replacing its default. A non-finite value is ignored.
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        let authored = |name: &str, default: f32| gmst(name).filter(|value| value.is_finite()).unwrap_or(default);
        Self {
            luck_skill_mult: authored("fActorLuckSkillMult", self.luck_skill_mult),
            luck_skill_base: authored("iActorLuckSkillBase", self.luck_skill_base),
            weapon_mult: authored("fDamageWeaponMult", self.weapon_mult),
            strength_base: authored("fDamageStrengthBase", self.strength_base),
            strength_mult: authored("fDamageStrengthMult", self.strength_mult),
            skill_base: authored("fDamageSkillBase", self.skill_base),
            skill_mult: authored("fDamageSkillMult", self.skill_mult),
            condition_base: authored("fDamageWeaponConditionBase", self.condition_base),
            condition_mult: authored("fDamageWeaponConditionMult", self.condition_mult),
            fatigue_base: authored("fFatigueBase", self.fatigue_base),
            fatigue_mult: authored("fFatigueMult", self.fatigue_mult),
            hand_health_min: authored("fHandHealthMin", self.hand_health_min),
            hand_health_max: authored("fHandHealthMax", self.hand_health_max),
            hand_strength_base: authored("fHandDamageStrengthBase", self.hand_strength_base),
            hand_strength_mult: authored("fHandDamageStrengthMult", self.hand_strength_mult),
            hand_skill_base: authored("fHandDamageSkillBase", self.hand_skill_base),
            hand_skill_mult: authored("fHandDamageSkillMult", self.hand_skill_mult),
            hand_fatigue_base: authored("fHandFatigueDamageBase", self.hand_fatigue_base),
            hand_fatigue_mult: authored("fHandFatigueDamageMult", self.hand_fatigue_mult),
        }
    }

    /// Oblivion's combat-time **Luck-chained effective skill**: `Skill + fActorLuckSkillMult × Luck +
    /// iActorLuckSkillBase` (`Skill + 0.4 × (Luck − 50)` at the defaults). Used at combat/haggling resolution,
    /// distinct from Luck's role in a stat's base value — reused verbatim across weapon damage, Hand-to-Hand damage
    /// and spell cost (`docs/engine/charal-oblivion-ruleset.md`). Not clamped here; each formula states its own clamp.
    #[must_use]
    pub fn modified_skill(&self, skill: f32, luck: f32) -> f32 {
        skill + self.luck_skill_mult * luck + self.luck_skill_base
    }

    /// The shared Blade/Blunt/Marksman weapon-damage multiplier: `fDamageWeaponMult × (fDamageStrengthBase +
    /// Attribute/100 × fDamageStrengthMult) × (fDamageSkillBase + ModifiedSkill/100 × fDamageSkillMult)` —
    /// `0.5 × (0.75 + 0.005·Attribute) × (0.2 + 0.015·ModifiedSkill)` at the defaults.
    ///
    /// `Attribute` is Strength for Blade/Blunt, Agility for Marksman; `weapon_skill` is the governing weapon-type skill.
    /// Both `Attribute` and the Luck-chained [`Self::modified_skill`] are clamped to `[0, 100]` before use — UESP
    /// states this explicitly ("Attribute and ModifiedSkill are constrained between 0 and 100"). The caller multiplies
    /// this by `BaseWeaponDamage` and by [`Self::condition_factor`], **not** by the raw condition ratio (#6762).
    #[must_use]
    pub fn weapon_multiplier(&self, attribute: f32, weapon_skill: f32, luck: f32) -> f32 {
        let attribute = attribute.clamp(0.0, 100.0);
        let skill = self.modified_skill(weapon_skill, luck).clamp(0.0, 100.0);
        self.weapon_mult
            * (self.strength_base + attribute / 100.0 * self.strength_mult)
            * (self.skill_base + skill / 100.0 * self.skill_mult)
    }

    /// The weapon-condition factor: `fDamageWeaponConditionBase + ratio × fDamageWeaponConditionMult`
    /// (`0.5 + 0.5 · ratio`), with `ratio` the weapon's current health over its base health. Together with
    /// `fDamageWeaponMult` it is `(ratio + 1) / 4` at the defaults, so a worn weapon still deals a quarter (#6762).
    #[must_use]
    pub fn condition_factor(&self, ratio: f32) -> f32 {
        self.condition_base + ratio * self.condition_mult
    }

    /// The attacker's Fatigue factor: `fFatigueBase − fFatigueMult × (1 − Fatigue / MaxFatigue)` —
    /// `(fraction + 1) / 2` at the defaults. `fraction` is passed as the caller reads it.
    #[must_use]
    pub fn fatigue_factor(&self, fraction: f32) -> f32 {
        self.fatigue_base - self.fatigue_mult * (1.0 - fraction)
    }

    /// Hand-to-Hand damage per hit, `(Health, Fatigue)`: `Health = fHandHealthMin + (fHandHealthMax −
    /// fHandHealthMin) × (fHandDamageStrengthBase + Strength/100 × fHandDamageStrengthMult) × (fHandDamageSkillBase +
    /// ModifiedSkill/100 × fHandDamageSkillMult)` — `1 + 10.5 × (Strength/100) × (ModifiedSkill/100)` at the defaults —
    /// and `Fatigue = fHandFatigueDamageBase + Health × fHandFatigueDamageMult`.
    ///
    /// A **pure cross-term** formula at the defaults (both bases are 0), confirmed structurally divergent from
    /// Blade/Blunt/Marksman, per UESP *The Complete Damage Formula*. Neither term is clamped here — UESP's `[0,100]`
    /// clamp note appears only in the Weapon Damage section, so this function doesn't assume it applies (no-guessing).
    #[must_use]
    pub fn hand_to_hand(&self, strength: f32, hand_to_hand_skill: f32, luck: f32) -> (f32, f32) {
        let skill = self.modified_skill(hand_to_hand_skill, luck);
        let health = self.hand_health_min
            + (self.hand_health_max - self.hand_health_min)
                * (self.hand_strength_base + strength / 100.0 * self.hand_strength_mult)
                * (self.hand_skill_base + skill / 100.0 * self.hand_skill_mult);
        let fatigue = self.hand_fatigue_base + health * self.hand_fatigue_mult;
        (health, fatigue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: OblivionDamageSettings = OblivionDamageSettings::OBLIVION;

    #[test]
    fn modified_skill_is_skill_at_luck_fifty() {
        // Luck 50 is the formula's neutral point — no adjustment either way.
        assert_eq!(DEFAULT.modified_skill(40.0, 50.0), 40.0);
        // Luck 100 → +20; Luck 0 → −20.
        assert_eq!(DEFAULT.modified_skill(40.0, 100.0), 60.0);
        assert_eq!(DEFAULT.modified_skill(40.0, 0.0), 20.0);
    }

    #[test]
    fn weapon_damage_multiplier_matches_hand_derivation() {
        // STR 0, Skill 0, Luck 50 → 0.5×0.75×0.2 = 0.075 (the bias term
        // recorded in charal-oblivion-ruleset.md's bilinear expansion).
        assert!((DEFAULT.weapon_multiplier(0.0, 0.0, 50.0) - 0.075).abs() < 1e-6);
        // STR 100, Skill 100, Luck 50 → 0.5×1.25×1.7 = 1.0625.
        assert!((DEFAULT.weapon_multiplier(100.0, 100.0, 50.0) - 1.0625).abs() < 1e-6);
        // The condition and Fatigue factors at their ends: (ratio + 1) / 2 and (fraction + 1) / 2.
        assert_eq!((DEFAULT.condition_factor(0.0), DEFAULT.condition_factor(1.0)), (0.5, 1.0));
        assert_eq!((DEFAULT.fatigue_factor(0.0), DEFAULT.fatigue_factor(1.0)), (0.5, 1.0));
    }

    #[test]
    fn weapon_damage_multiplier_luck_chains_into_skill() {
        // Skill 50 with Luck 100 (ModifiedSkill 70) must out-damage the same
        // Skill 50 at Luck 50 (ModifiedSkill 50) — Luck strictly helps here.
        let low_luck = DEFAULT.weapon_multiplier(50.0, 50.0, 50.0);
        let high_luck = DEFAULT.weapon_multiplier(50.0, 50.0, 100.0);
        assert!(high_luck > low_luck);
    }

    #[test]
    fn weapon_damage_multiplier_clamps_inputs_past_100() {
        // Attribute/ModifiedSkill are stated to be constrained to [0,100];
        // pushing either past 100 must not out-scale the capped value.
        let capped = DEFAULT.weapon_multiplier(100.0, 100.0, 100.0);
        let overdriven = DEFAULT.weapon_multiplier(200.0, 200.0, 100.0);
        assert_eq!(capped, overdriven);
    }

    #[test]
    fn hand_to_hand_damage_matches_formula() {
        // STR 0, Skill 0 → Health 1, Fatigue 1.5.
        let (health, fatigue) = DEFAULT.hand_to_hand(0.0, 0.0, 50.0);
        assert_eq!(health, 1.0);
        assert_eq!(fatigue, 1.5);
        // STR 100, Skill 100, Luck 50 → Health 1+10.5=11.5, Fatigue 1+5.75=6.75.
        let (health, fatigue) = DEFAULT.hand_to_hand(100.0, 100.0, 50.0);
        assert!((health - 11.5).abs() < 1e-6);
        assert!((fatigue - 6.75).abs() < 1e-6);
    }

    #[test]
    fn hand_to_hand_fatigue_always_chains_off_health() {
        let (health, fatigue) = DEFAULT.hand_to_hand(73.0, 61.0, 80.0);
        assert!((fatigue - (1.0 + 0.5 * health)).abs() < 1e-6);
    }

    /// #7199 — every setting is the load order's when authored: a rebalance that doubles `fDamageWeaponMult`
    /// doubles the multiplier, a retuned `fActorLuckSkillMult` moves the effective skill, the Hand-to-Hand ceiling
    /// follows `fHandHealthMax`, and a non-finite value keeps its default.
    #[test]
    fn the_damage_settings_follow_the_load_order() {
        let doubled = DEFAULT.with_gmst(|name| (name == "fDamageWeaponMult").then_some(1.0));
        assert!((doubled.weapon_multiplier(100.0, 100.0, 50.0) - 2.125).abs() < 1e-6);
        let lucky = DEFAULT.with_gmst(|name| match name {
            "fActorLuckSkillMult" => Some(1.0),
            "iActorLuckSkillBase" => Some(-50.0),
            _ => None,
        });
        assert_eq!(lucky.modified_skill(40.0, 100.0), 90.0);
        let stronger = DEFAULT.with_gmst(|name| (name == "fHandHealthMax").then_some(29.0));
        assert!((stronger.hand_to_hand(100.0, 100.0, 50.0).0 - 22.0).abs() < 1e-6, "1 + 28 x 0.75 x 1");
        let tired = DEFAULT.with_gmst(|name| (name == "fFatigueMult").then_some(1.0));
        assert_eq!(tired.fatigue_factor(0.0), 0.0);
        assert_eq!(DEFAULT.with_gmst(|_| Some(f32::NAN)), DEFAULT);
        let every = DEFAULT.with_gmst(|_| Some(7.0));
        assert_eq!((every.weapon_mult, every.hand_fatigue_mult, every.luck_skill_base), (7.0, 7.0, 7.0), "all nineteen are read");
    }
}
