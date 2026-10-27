//! VATS tuning model (CHARAL) — the `fVATS*` / `iVATS*` GMST family and
//! the to-hit chain, mod-compatibility-first (#6350).
//!
//! ## Mod compatibility contract
//!
//! Every number VATS uses resolves as *load-order-merged GMST → compiled-in
//! vanilla default*, exactly like [`super::LevelingModel`] / [`super::CrimeModel`]. The
//! compiled-in defaults below are the GECK wiki *VATS Settings* /
//! *FVATSScreenPercentFactor* pages' documented defaults, which the FO3
//! master matches value-for-value; FalloutNV.esm re-tunes six of them,
//! so [`VatsModel::FALLOUT_NV`] carries its own (#6424). No gameplay constant lives anywhere else: a modded load order
//! that overrides `fVATSHitChanceMult` changes [`VatsModel::to_hit_chance`]
//! output the same frame the model is rebuilt, and the acceptance test in
//! this file pins that end-to-end.
//!
//! ## The to-hit chain is reconstructed — and says so
//!
//! No public source documents the unified to-hit equation: the GECK wiki
//! describes each factor's *role*, the Fallout wiki states the inputs
//! (distance, part visibility, combat skill) and the 95 % cap, but neither
//! publishes the operation order. The chain below composes the documented
//! anchors (weapon base "factored before distance, gun skill, and screen
//! percentage"; `fVATSHitChanceMult` multiplies weapon accuracy; distance
//! decay is non-linear in `fVATSDistanceFactor`; Concentrated Fire adds
//! `[bonus/100] × (shot − 1)` after everything; the result caps at
//! `fVATSMaxChance`) and marks every *reconstructed* step as such. The
//! arbiter is [`VatsModel::to_hit_chance`]'s ground-truth test: fixed
//! in-game scenarios, recorded by hand from the real game, pinned as
//! constants. Until those captures land, that test is `#[ignore]`d —
//! it must never pass on the strength of the reconstruction matching
//! itself.

use super::regen::POOL_REGEN_DT;
use crate::ecs::resource::Resource;

/// The `fActionPoints*` family — the per-action AP *standards* the GECK's
/// weapon-Override field replaces ("Action Points: overrides the standard
/// number of action points the weapon uses"), plus the restore rate.
/// Vanilla values corpus-pinned (#6350, `probe_gmst_vats`; FO3 and FNV
/// author identical attack costs — only the restore rate differs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StandardActionPoints {
    /// `fActionPointsRestoreRate`. **Unit differs per game**: FO3 authors
    /// `4` per second; FNV authors `0.06` per frame (the 30-vs-60 fps
    /// regen divergence FNV inherited). Normalize through
    /// [`VatsModel::ap_regen_per_second`], never read this raw.
    pub restore_rate: f32,
    /// True on the FNV dialect (rate is per frame on a 60 Hz clock).
    pub restore_rate_per_frame: bool,
    /// `fActionPointsAttackPistol` (20).
    pub attack_pistol: f32,
    /// `fActionPointsAttackRifle` (25).
    pub attack_rifle: f32,
    /// `fActionPointsAttackOneHandMelee` (15).
    pub attack_one_hand_melee: f32,
    /// `fActionPointsAttackTwoHandMelee` (30).
    pub attack_two_hand_melee: f32,
    /// `fActionPointsAttackUnarmed` (22).
    pub attack_unarmed: f32,
    /// `fActionPointsAttackGrenade` (35).
    pub attack_grenade: f32,
    /// `fActionPointsAttackMine` (10).
    pub attack_mine: f32,
    /// `fActionPointsAttackLauncher` (40).
    pub attack_launcher: f32,
    /// `fActionPointsAttackHandle` (20).
    pub attack_handle: f32,
    /// `fActionPointsReload` (10).
    pub reload: f32,
    /// `fActionPointsHeal` (10).
    pub heal: f32,
    /// `fActionPointsPowerAttackMult` (1.5).
    pub power_attack_mult: f32,
    /// `fActionPointsVATSMeleeMult` (1).
    pub vats_melee_mult: f32,
}

/// The `fVATS*` / `iVATS*` tuning family. Field-per-GMST, so a later
/// phase (playback, melee warp, the Stranger) consumes its knobs without
/// reopening the model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VatsModel {
    /// The `fActionPoints*` per-action standards + restore rate.
    pub ap: StandardActionPoints,
    /// `fVATSHitChanceMult` (default 1) — "the value at which VATS
    /// multiplies the accuracy of a weapon".
    pub hit_chance_mult: f32,
    /// `fVATSMaxChance` (default 95) — the cap on any displayed hit
    /// chance.
    pub max_chance: f32,
    /// `fVATSDistanceFactor` (default 2) — "controls how rapidly the
    /// chance to hit decreases as the range of the target increases.
    /// The rate is non linear."
    pub distance_factor: f32,
    /// `fVATSSkillFactor` (default 1) — how strongly weapon skill moves
    /// the chance.
    pub skill_factor: f32,
    /// `fVATSMaxEngageDistance` (default 5000) — actors farther than
    /// this are not valid VATS targets at all.
    pub max_engage_distance: f32,
    /// `fVATSScreenPercentFactor` (default 1) — scales the limb
    /// screen-percentage term ("so the effect of limb screen percentage
    /// can be reduced or eliminated").
    pub screen_percent_factor: f32,
    /// `fVATSMeleeMaxDistance` (default 300) — VATS melee reach cap.
    pub melee_max_distance: f32,
    /// `fVATSMeleeReachMult` (default 2) — VATS multiplies a melee
    /// weapon's real reach by this.
    pub melee_reach_mult: f32,
    /// `fVATSCriticalChanceBonus` (default 15) — flat critical-chance
    /// bonus while in VATS (feeds the Critical Hit Chance formula).
    pub critical_chance_bonus: f32,
    /// `fVATSPlayerDamageMult` (default 0.1) — damage taken by the
    /// *player* is multiplied by this while VATS is active.
    pub player_damage_mult: f32,
    /// `fVATSShotBurstTime` (default 0.43) — automatic-weapon burst
    /// window per VATS attack.
    pub shot_burst_time: f32,
    /// `fVATSShotLongBurstTime` (default 1.75) — same, for the
    /// Long Bursts weapon flag.
    pub shot_long_burst_time: f32,
    /// `fVATSSpreadMult` (default 0.5) — Gun Spread Formula spread is
    /// multiplied by this in VATS.
    pub spread_mult: f32,
    /// `fVATSStealthMult` (default 0.5).
    pub stealth_mult: f32,
    /// `fVATSAutomaticMeleeDamageMult` (default 2).
    pub automatic_melee_damage_mult: f32,
    /// `iVATSConcentratedFireBonus` (default 5, percentage points) —
    /// Concentrated Fire: "+ [bonus/100] × [current shot − 1]".
    pub concentrated_fire_bonus: f32,
    /// `fVATSDamageToWeaponMult` (default 5) — extra weapon wear per VATS
    /// shot, as a multiple of base shot damage.
    pub damage_to_weapon_mult: f32,
    /// `fVATSDestructibleMult` (default 1.25).
    pub destructible_mult: f32,
    /// `fVATSMeleeWarpDistanceMult` (default 0.32).
    pub melee_warp_distance_mult: f32,
    /// `fVATSH2HWarpDistanceMult` (default 0.27).
    pub h2h_warp_distance_mult: f32,
    /// `fVATSParalyzePalmChance` (default 0.3).
    pub paralyze_palm_chance: f32,
    /// `fVATSPlaybackDelay` (default 0.17).
    pub playback_delay: f32,
    /// `fVATSStrangerOdds` (default 0.1) — Mysterious Stranger chance.
    pub stranger_odds: f32,
    /// `iVATSStrangerMaxHP` (default 150) — the Stranger only acts on
    /// targets below this health.
    pub stranger_max_hp: f32,
    /// `fVATSStrangerDistance` (default 164).
    pub stranger_distance: f32,
    /// `fVATSRangeSpreadMax` (default 3).
    pub range_spread_max: f32,
    /// `fVATSRangeSpreadUncertainty` (default 0.7).
    pub range_spread_uncertainty: f32,
}

impl Resource for VatsModel {}

impl VatsModel {
    /// Vanilla FO3 defaults — the GECK wiki *VATS Settings* page
    /// value-for-value, plus the FO3 `fActionPoints` corpus values.
    /// FalloutNV.esm re-tunes six of the `fVATS*` settings (see
    /// [`Self::FALLOUT_NV`]); the rest of the family, and every
    /// `fActionPoints*` attack standard, is shared.
    pub const FALLOUT3: Self = Self {
        hit_chance_mult: 1.0,
        max_chance: 95.0,
        distance_factor: 2.0,
        skill_factor: 1.0,
        max_engage_distance: 5000.0,
        screen_percent_factor: 1.0,
        melee_max_distance: 300.0,
        melee_reach_mult: 2.0,
        critical_chance_bonus: 15.0,
        player_damage_mult: 0.1,
        shot_burst_time: 0.43,
        shot_long_burst_time: 1.75,
        spread_mult: 0.5,
        stealth_mult: 0.5,
        automatic_melee_damage_mult: 2.0,
        concentrated_fire_bonus: 5.0,
        damage_to_weapon_mult: 5.0,
        destructible_mult: 1.25,
        melee_warp_distance_mult: 0.32,
        h2h_warp_distance_mult: 0.27,
        paralyze_palm_chance: 0.3,
        playback_delay: 0.17,
        stranger_odds: 0.1,
        stranger_max_hp: 150.0,
        stranger_distance: 164.0,
        range_spread_max: 3.0,
        range_spread_uncertainty: 0.7,
        ap: StandardActionPoints {
            restore_rate: 4.0,
            restore_rate_per_frame: false,
            attack_pistol: 20.0,
            attack_rifle: 25.0,
            attack_one_hand_melee: 15.0,
            attack_two_hand_melee: 30.0,
            attack_unarmed: 22.0,
            attack_grenade: 35.0,
            attack_mine: 10.0,
            attack_launcher: 40.0,
            attack_handle: 20.0,
            reload: 10.0,
            heal: 10.0,
            power_attack_mult: 1.5,
            vats_melee_mult: 1.0,
        },
    };

    /// Vanilla FNV: FO3's family with the six `fVATS*` settings
    /// FalloutNV.esm authors differently (read with
    /// `examples/probe_gmst_vats`, #6424), and the AP restore rate on the
    /// per-frame dialect (0.06 × 60 Hz = 3.6/s). The wiki-only defaults
    /// no master authors stay shared.
    pub const FALLOUT_NV: Self = Self {
        // FNV authors 5 (the wiki's 15 is the FO3-era default; Fallout3.esm
        // does not author the setting at all).
        critical_chance_bonus: 5.0,
        player_damage_mult: 0.75,
        damage_to_weapon_mult: 1.0,
        spread_mult: 1.1,
        range_spread_max: 100.0,
        range_spread_uncertainty: 0.01,
        ap: StandardActionPoints {
            restore_rate: 0.06,
            restore_rate_per_frame: true,
            ..Self::FALLOUT3.ap
        },
        ..Self::FALLOUT3
    };

    /// The model with each setting the load order authors replacing its
    /// default. Non-finite or negative authored values are ignored
    /// (the [`super::CrimeModel::with_gmst`] discipline).
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        let sane = |v: Option<f32>| v.filter(|x| x.is_finite() && *x >= 0.0);
        let f = |name: &str, field: f32| sane(gmst(name)).unwrap_or(field);
        Self {
            hit_chance_mult: f("fVATSHitChanceMult", self.hit_chance_mult),
            max_chance: f("fVATSMaxChance", self.max_chance),
            distance_factor: f("fVATSDistanceFactor", self.distance_factor),
            skill_factor: f("fVATSSkillFactor", self.skill_factor),
            max_engage_distance: f("fVATSMaxEngageDistance", self.max_engage_distance),
            screen_percent_factor: f("fVATSScreenPercentFactor", self.screen_percent_factor),
            melee_max_distance: f("fVATSMeleeMaxDistance", self.melee_max_distance),
            melee_reach_mult: f("fVATSMeleeReachMult", self.melee_reach_mult),
            critical_chance_bonus: f("fVATSCriticalChanceBonus", self.critical_chance_bonus),
            player_damage_mult: f("fVATSPlayerDamageMult", self.player_damage_mult),
            shot_burst_time: f("fVATSShotBurstTime", self.shot_burst_time),
            shot_long_burst_time: f("fVATSShotLongBurstTime", self.shot_long_burst_time),
            spread_mult: f("fVATSSpreadMult", self.spread_mult),
            stealth_mult: Self::weak_bounded(gmst("fVATSStealthMult"), self.stealth_mult, 1.0),
            automatic_melee_damage_mult: f(
                "fVATSAutomaticMeleeDamageMult",
                self.automatic_melee_damage_mult,
            ),
            concentrated_fire_bonus: f("iVATSConcentratedFireBonus", self.concentrated_fire_bonus),
            damage_to_weapon_mult: f("fVATSDamageToWeaponMult", self.damage_to_weapon_mult),
            destructible_mult: f("fVATSDestructibleMult", self.destructible_mult),
            melee_warp_distance_mult: f("fVATSMeleeWarpDistanceMult", self.melee_warp_distance_mult),
            h2h_warp_distance_mult: f("fVATSH2HWarpDistanceMult", self.h2h_warp_distance_mult),
            paralyze_palm_chance: f("fVATSParalyzePalmChance", self.paralyze_palm_chance),
            playback_delay: f("fVATSPlaybackDelay", self.playback_delay),
            stranger_odds: f("fVATSStrangerOdds", self.stranger_odds),
            stranger_max_hp: f("iVATSStrangerMaxHP", self.stranger_max_hp),
            stranger_distance: f("fVATSStrangerDistance", self.stranger_distance),
            range_spread_max: f("fVATSRangeSpreadMax", self.range_spread_max),
            range_spread_uncertainty: f("fVATSRangeSpreadUncertainty", self.range_spread_uncertainty),
            ap: StandardActionPoints {
                restore_rate: f("fActionPointsRestoreRate", self.ap.restore_rate),
                restore_rate_per_frame: self.ap.restore_rate_per_frame,
                attack_pistol: f("fActionPointsAttackPistol", self.ap.attack_pistol),
                attack_rifle: f("fActionPointsAttackRifle", self.ap.attack_rifle),
                attack_one_hand_melee: f(
                    "fActionPointsAttackOneHandMelee",
                    self.ap.attack_one_hand_melee,
                ),
                attack_two_hand_melee: f(
                    "fActionPointsAttackTwoHandMelee",
                    self.ap.attack_two_hand_melee,
                ),
                attack_unarmed: f("fActionPointsAttackUnarmed", self.ap.attack_unarmed),
                attack_grenade: f("fActionPointsAttackGrenade", self.ap.attack_grenade),
                attack_mine: f("fActionPointsAttackMine", self.ap.attack_mine),
                attack_launcher: f("fActionPointsAttackLauncher", self.ap.attack_launcher),
                attack_handle: f("fActionPointsAttackHandle", self.ap.attack_handle),
                reload: f("fActionPointsReload", self.ap.reload),
                heal: f("fActionPointsHeal", self.ap.heal),
                power_attack_mult: f("fActionPointsPowerAttackMult", self.ap.power_attack_mult),
                vats_melee_mult: f("fActionPointsVATSMeleeMult", self.ap.vats_melee_mult),
            },
        }
    }

    /// The restore rate normalized to per-second on the fixed 60 Hz
    /// clock — the number [`crate::character::regen::ActionPointsRegen`]
    /// consumes. A mod re-pricing `fActionPointsRestoreRate` flows
    /// through with its game's unit semantics intact.
    #[must_use]
    pub fn ap_regen_per_second(&self) -> f32 {
        if self.ap.restore_rate_per_frame {
            self.ap.restore_rate / POOL_REGEN_DT
        } else {
            self.ap.restore_rate
        }
    }

    /// Values in `0.0..=bound` — a multiplier family where >1 would
    /// invert the setting's documented meaning.
    fn weak_bounded(v: Option<f32>, default: f32, bound: f32) -> f32 {
        v.filter(|x| x.is_finite() && *x >= 0.0 && *x <= bound)
            .unwrap_or(default)
    }

    /// The chance to hit one body part of one target, in percent.
    ///
    /// Documented anchors (GECK wiki *Weapons* / *VATS Settings*, Fallout
    /// wiki *Fallout 3 V.A.T.S.*):
    ///
    /// * the weapon's Base VATS To-Hit Chance is "factored before
    ///   distance, gun skill, and screen percentage";
    /// * `fVATSHitChanceMult` multiplies the weapon's accuracy;
    /// * distance decay is non-linear in `fVATSDistanceFactor`
    ///   ("smaller values slow the rate accuracy decreases");
    /// * the limb screen percentage term is scaled by
    ///   `fVATSScreenPercentFactor`;
    /// * Concentrated Fire adds `[iVATSConcentratedFireBonus/100] ×
    ///   (current shot − 1)` — the wiki's own worked example: 63 % → 68 %
    ///   → 73 %;
    /// * the result caps at `fVATSMaxChance` (95).
    ///
    /// Everything between those anchors is **RECONSTRUCTED** (marked per
    /// step) and arbitrated by the ground-truth test at the bottom of
    /// this file. A wrong operation order will pass every structural
    /// gate here and fail that one — which is the point of it.
    pub fn to_hit_chance(&self, input: VatsHitInput) -> f32 {
        // Beyond the engage distance the target is not valid at all
        // (documented): no chance, not a decayed one.
        if input.distance > self.max_engage_distance {
            return 0.0;
        }
        // Step 1 (documented order): the weapon's base chance times the
        // global hit multiplier, scaled by the part's authored share.
        // RECONSTRUCTED: the part-share composition (weapon × part/100)
        // — the wiki pins that both are "factored before" distance and
        // skill, not how they compose with each other.
        let mut chance = input.weapon_base_to_hit
            * self.hit_chance_mult
            * (input.part_to_hit / 100.0);
        // Step 2 (documented inputs, RECONSTRUCTED form): weapon skill
        // moves the chance around a skill-50 neutral point.
        let skill_term = 1.0 + (input.skill / 100.0 - 0.5) * self.skill_factor;
        chance *= skill_term.max(0.01);
        // Step 3 (documented inputs, RECONSTRUCTED form): non-linear
        // distance decay to zero at the engage boundary; the exponent
        // is fVATSDistanceFactor, matching the documented monotonicity
        // in that setting.
        let range = (1.0 - input.distance / self.max_engage_distance).max(0.0);
        chance *= range.powf(self.distance_factor.max(0.0));
        // Step 4 (documented): the limb's visible screen percentage,
        // scaled by fVATSScreenPercentFactor so the term can be reduced
        // or eliminated. RECONSTRUCTED: multiplicative composition.
        let screen = 1.0 - self.screen_percent_factor * (1.0 - input.screen_percent);
        chance *= screen.max(0.0);
        // Step 5 (documented, wiki-exact): Concentrated Fire adds
        // [bonus/100] × (shot − 1) in the wiki's 0..1 fraction space —
        // 63 % → 68 % → 73 % with the default bonus of 5 — i.e. `bonus`
        // percentage points per prior shot, after everything. (The first
        // draft of this step divided in percent space and added 0.05;
        // the wiki-example test below caught it.)
        chance += self.concentrated_fire_bonus * input.concentrated_shot.saturating_sub(1) as f32;
        // Step 6 (documented): cap at fVATSMaxChance; the floor is the
        // natural zero.
        chance.clamp(0.0, self.max_chance)
    }
}

/// One to-hit evaluation: a weapon, a body part, a range, a shooter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VatsHitInput {
    /// Weapon Base VATS To-Hit Chance, percent (0-100; `WEAP` DNAM @28).
    pub weapon_base_to_hit: f32,
    /// Body part base hit chance, percent (0-100, 255 = "always";
    /// `BPTD` BPND @6).
    pub part_to_hit: f32,
    /// Shooter-to-part distance in game units.
    pub distance: f32,
    /// Relevant weapon skill, 0-100.
    pub skill: f32,
    /// Fraction of the part's bounding volume visible on screen and
    /// unoccluded, 0.0-1.0.
    pub screen_percent: f32,
    /// 1-based index of this shot at the same part in one VATS queue
    /// (Concentrated Fire). 1 for the first shot.
    pub concentrated_shot: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// The documented anchors hold on the vanilla model.
    #[test]
    fn vanilla_defaults_match_the_geck_wiki_page() {
        let m = VatsModel::FALLOUT3;
        assert_eq!(m.max_chance, 95.0);
        assert_eq!(m.hit_chance_mult, 1.0);
        assert_eq!(m.distance_factor, 2.0);
        assert_eq!(m.max_engage_distance, 5000.0);
        assert_eq!(m.concentrated_fire_bonus, 5.0);
        assert_eq!(m.critical_chance_bonus, 15.0);
        assert_eq!(m.player_damage_mult, 0.1);
        assert_eq!(m.screen_percent_factor, 1.0);
    }

    /// The AP corpus values pin per game: identical attack standards,
    /// the restore-rate unit split FO3 (4/s) vs FNV (0.06/frame).
    #[test]
    fn ap_standards_pin_the_per_game_corpus_values() {
        assert_eq!(VatsModel::FALLOUT3.ap.restore_rate, 4.0);
        // Compile-time pins (clippy: assertions_on_constants).
        const _: () = assert!(!VatsModel::FALLOUT3.ap.restore_rate_per_frame);
        assert_eq!(VatsModel::FALLOUT3.ap_regen_per_second(), 4.0);
        assert_eq!(VatsModel::FALLOUT_NV.ap.restore_rate, 0.06);
        const _: () = assert!(VatsModel::FALLOUT_NV.ap.restore_rate_per_frame);
        assert!((VatsModel::FALLOUT_NV.ap_regen_per_second() - 3.6).abs() < 1e-4);
        // The attack standards are the shared corpus values.
        for m in [VatsModel::FALLOUT3, VatsModel::FALLOUT_NV] {
            assert_eq!(m.ap.attack_pistol, 20.0);
            assert_eq!(m.ap.attack_rifle, 25.0);
            assert_eq!(m.ap.attack_one_hand_melee, 15.0);
            assert_eq!(m.ap.attack_two_hand_melee, 30.0);
            assert_eq!(m.ap.reload, 10.0);
        }
        // A mod re-pricing the restore rate keeps its unit: FNV 0.1 per
        // frame -> 6/s.
        let modded = VatsModel::FALLOUT_NV
            .with_gmst(|name| (name == "fActionPointsRestoreRate").then_some(0.1));
        assert!((modded.ap_regen_per_second() - 6.0).abs() < 1e-4);
    }

    /// #6424 — the compiled-in FNV fallbacks are FNV's own, not FO3's:
    /// the six `fVATS*` values FalloutNV.esm authors (probe_gmst_vats on
    /// the master) against the FO3 values Fallout3.esm authors.
    #[test]
    fn fnv_fallbacks_carry_the_six_values_falloutnv_esm_authors() {
        let (fo3, fnv) = (VatsModel::FALLOUT3, VatsModel::FALLOUT_NV);
        assert_eq!(fnv.critical_chance_bonus, 5.0);
        assert_eq!(fnv.player_damage_mult, 0.75);
        assert_eq!(fnv.damage_to_weapon_mult, 1.0);
        assert_eq!(fnv.spread_mult, 1.1);
        assert_eq!(fnv.range_spread_max, 100.0);
        assert_eq!(fnv.range_spread_uncertainty, 0.01);
        // FO3's own side of the same six (Fallout3.esm; the crit bonus is
        // the GECK wiki default — the master does not author it).
        assert_eq!(fo3.player_damage_mult, 0.1);
        assert_eq!(fo3.damage_to_weapon_mult, 5.0);
        assert_eq!(fo3.spread_mult, 0.5);
        assert_eq!(fo3.range_spread_max, 3.0);
        assert_eq!(fo3.range_spread_uncertainty, 0.7);
        // Everything the masters do not split stays shared.
        assert_eq!(fnv.max_chance, fo3.max_chance);
        assert_eq!(fnv.distance_factor, fo3.distance_factor);
        assert_eq!(fnv.concentrated_fire_bonus, fo3.concentrated_fire_bonus);
    }

    /// Point-blank, fully visible, skill 50, torso part (100), a 65 %
    /// weapon: every reconstructed term is neutral except the weapon
    /// and part bases, so the chain's multiplicative skeleton is
    /// directly observable.
    #[test]
    fn neutral_conditions_reduce_to_weapon_times_part_share() {
        let m = VatsModel::FALLOUT3;
        let chance = m.to_hit_chance(VatsHitInput {
            weapon_base_to_hit: 65.0,
            part_to_hit: 100.0,
            distance: 0.0,
            skill: 50.0,
            screen_percent: 1.0,
            concentrated_shot: 1,
        });
        assert!(close(chance, 65.0), "got {chance}");
    }

    /// The engage boundary is a gate, not a decay: one unit inside is a
    /// real (if small) chance, one unit outside is zero.
    #[test]
    fn max_engage_distance_gates_rather_than_decays() {
        let m = VatsModel::FALLOUT3;
        let base = VatsHitInput {
            weapon_base_to_hit: 65.0,
            part_to_hit: 100.0,
            distance: 4999.0,
            skill: 50.0,
            screen_percent: 1.0,
            concentrated_shot: 1,
        };
        assert!(m.to_hit_chance(base) > 0.0);
        assert_eq!(m.to_hit_chance(VatsHitInput { distance: 5001.0, ..base }), 0.0);
    }

    /// Concentrated Fire is the one wiki-exact worked example: 63 %
    /// becomes 68 on the second shot and 73 on the third.
    #[test]
    fn concentrated_fire_matches_the_wiki_example() {
        let m = VatsModel::FALLOUT3;
        // Any input producing 63 % on shot 1 works; solved for the
        // quadratic distance term: 90 × (1 − d/5000)² = 63 → d ≈ 816.7.
        let input = VatsHitInput {
            weapon_base_to_hit: 90.0,
            part_to_hit: 100.0,
            distance: 816.7,
            skill: 50.0,
            screen_percent: 1.0,
            concentrated_shot: 1,
        };
        let first = m.to_hit_chance(input);
        assert!(close(first, 63.0), "fixture drifted: first={first}");
        assert!(close(m.to_hit_chance(VatsHitInput { concentrated_shot: 2, ..input }), 68.0));
        assert!(close(m.to_hit_chance(VatsHitInput { concentrated_shot: 3, ..input }), 73.0));
    }

    /// Monotonicity in the documented directions: closer, more skill,
    /// more visible part, bigger weapon base — each strictly helps.
    #[test]
    fn chance_is_monotonic_in_every_documented_input() {
        let m = VatsModel::FALLOUT3;
        let base = VatsHitInput {
            weapon_base_to_hit: 65.0,
            part_to_hit: 60.0,
            distance: 2000.0,
            skill: 40.0,
            screen_percent: 0.7,
            concentrated_shot: 1,
        };
        let mid = m.to_hit_chance(base);
        assert!(m.to_hit_chance(VatsHitInput { distance: 1000.0, ..base }) > mid);
        assert!(m.to_hit_chance(VatsHitInput { skill: 80.0, ..base }) > mid);
        assert!(m.to_hit_chance(VatsHitInput { screen_percent: 0.95, ..base }) > mid);
        assert!(m.to_hit_chance(VatsHitInput { weapon_base_to_hit: 80.0, ..base }) > mid);
        assert!(m.to_hit_chance(VatsHitInput { part_to_hit: 80.0, ..base }) > mid);
    }

    /// The result never exceeds fVATSMaxChance, including the
    /// "always" part sentinel (255) and Concentrated Fire stacking.
    #[test]
    fn chance_caps_at_max_chance() {
        let m = VatsModel::FALLOUT3;
        let chance = m.to_hit_chance(VatsHitInput {
            weapon_base_to_hit: 100.0,
            part_to_hit: 255.0,
            distance: 0.0,
            skill: 100.0,
            screen_percent: 1.0,
            concentrated_shot: 5,
        });
        assert_eq!(chance, m.max_chance);
    }

    /// THE MOD-COMPAT ACCEPTANCE: a modded load order overriding the
    /// GMSTs changes the math — the whole point of #6350. A mod that
    /// halves the hit multiplier and lowers the cap must produce lower
    /// chances through the same code path, and a *new-FormID* GMST
    /// (the 1a case) must resolve identically to a proper override.
    #[test]
    fn authored_gmsts_reprice_the_model() {
        let vanilla = VatsModel::FALLOUT3;
        let modded = vanilla.with_gmst(|name| match name {
            "fVATSHitChanceMult" => Some(0.5),
            "fVATSMaxChance" => Some(50.0),
            _ => None,
        });
        let input = VatsHitInput {
            weapon_base_to_hit: 65.0,
            part_to_hit: 100.0,
            distance: 1000.0,
            skill: 50.0,
            screen_percent: 1.0,
            concentrated_shot: 1,
        };
        let a = vanilla.to_hit_chance(input);
        let b = modded.to_hit_chance(input);
        assert!(close(b, a * 0.5), "halved multiplier: {a} → {b}");
        let hot = modded.to_hit_chance(VatsHitInput {
            weapon_base_to_hit: 100.0,
            part_to_hit: 255.0,
            distance: 0.0,
            skill: 100.0,
            screen_percent: 1.0,
            concentrated_shot: 1,
        });
        assert_eq!(hot, 50.0, "the authored cap, not 95");
    }

    /// GROUND TRUTH — fixed vanilla scenarios, recorded by hand from the
    /// real game. This is the only test that can catch a wrong operation
    /// ORDER (every structural test above passes under a permuted chain
    /// that produces different numbers).
    ///
    /// # Capture procedure (FNV, vanilla load order, no mods)
    ///
    /// 1. Make a clean save outdoors (e.g. Goodsprings source exit),
    ///    during daytime, unarmed skill irrelevant.
    /// 2. `player.setav guns 50` (or the weapon's skill) so the skill
    ///    term is the skill-50 neutral point.
    /// 3. Spawn the fixture target: `player.placeatme 000E32D7`
    ///    (a Goodsprings settler — DefaultBodyPartData limb table).
    /// 4. Equip the 10mm pistol (`0000434F`, base 65 %, AP 17).
    /// 5. Stand at the listed distance (`tdt` + `getdistance player`,
    ///    or pace it: 64 units ≈ one meter) with the listed part fully
    ///    visible and no obstruction.
    /// 6. Enter VATS, hover the listed part, record the displayed
    ///    percentage.
    ///
    /// Fill one row per scenario, delete this `#[ignore]`, and let the
    /// test fail its first run if the reconstruction is wrong — that
    /// failure is the reconstruction being corrected, not a regression.
    #[test]
    #[ignore = "ground-truth captures pending: record the scenarios below in vanilla FNV, then fill GROUND_TRUTH and remove this attribute"]
    fn to_hit_chance_matches_recorded_in_game_percentages() {
        /// (label, input, displayed percentage as recorded by hand).
        const GROUND_TRUTH: &[(&str, VatsHitInput, f32)] = &[
            // (
            //     "10mm pistol, torso, 512 units, Guns 50, full visibility",
            //     VatsHitInput {
            //         weapon_base_to_hit: 65.0,
            //         part_to_hit: 100.0,
            //         distance: 512.0,
            //         skill: 50.0,
            //         screen_percent: 1.0,
            //         concentrated_shot: 1,
            //     },
            //     0.0, // ← replace with the in-game display
            // ),
        ];
        let model = VatsModel::FALLOUT3.with_gmst(|name| {
            // Fill from the load order's GMSTs when capturing; vanilla
            // values equal the compiled defaults, so `|_| None` is
            // correct for a clean vanilla run.
            let _ = name;
            None
        });
        for (label, input, recorded) in GROUND_TRUTH {
            let computed = model.to_hit_chance(*input);
            // The in-game display is integer percent.
            assert!(
                (computed - recorded).abs() <= 1.0,
                "{label}: computed {computed:.1} vs recorded {recorded}"
            );
        }
        assert!(
            !GROUND_TRUTH.is_empty(),
            "fill GROUND_TRUTH before enabling this test"
        );
    }
}
