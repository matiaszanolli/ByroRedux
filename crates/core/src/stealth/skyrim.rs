//! Skyrim's detection value, stealth points and sneak attack (SNK-DET-007,
//! -015, SNK-ATK-004, SNK-PERK-004; design §4.2, §4.3, §8).
//!
//! The value is `SkyrimSE.exe` `0x14041cd90`, the CK formula
//! (*Category:Detection*) with every coefficient a setting:
//!
//! ```text
//! P       = fSneakPerceptionSkillMin + (Max − Min) · detector Sneak / 100
//! state   = 1 + alert·fSneakAlertMod + asleep·fSneakSleepMod + notCombatTarget·fSneakCombatMod
//! att     = max(0, (maxD − d) / maxD) ^ fSneakDistanceAttenuationExponent
//! sound   = max(0, (action·int(fSneakActionMult)
//!             + moving·(fSneakEquippedWeightBase + fSneakEquippedWeightMult·weight)·noise·[running: fSneakRunningMult])
//!             · fSneakSoundsMult · att·[no LOS: fSneakSoundLosMult] · (1 + asleep·fSneakSleepBonus))
//! motion  = entry 48 (Mod Detection Movement) of (1 + moving·fSneakLightMoveMult + running·fSneakLightRunMult)
//! light   = entry 47 (Mod Detection Light) of the light level · (exterior: fSneakLightExteriorMult, else fSneakLightMult)
//!             + fDetectionSneakLightMod
//! visual  = max(0, sees·att·motion·light·(asleep: 0, else (100 − Blindness)/100)·size·fDetectionLargeActorSizeMult)
//! target  = entry 57 (Mod Detection Sneak Skill) of (fPlayerDetectionSneakMult·Sneak + fPlayerDetectionSneakBase)
//! skill   = (att·state·P − sneaking·target) · fSneakSkillMult
//! value   = int(fSneakBaseValue + sound + visual + skill), 0 < value < 1 → 1
//! ```
//!
//! The entry points run on the target's perks with the detector as their other
//! actor (`0x14038cff0`), which is why the caller supplies them. The light level
//! is luminance scaled to `iLightLevelMax` plus `iLightLevelInteriorMod` /
//! `ExteriorMod` (design §6).
//!
//! **Stealth points** (CK *Stealth Points*; `0x140818570`, V11): every 0.1 s the
//! highest value any aggressive detector has toward the player drains them by
//! value · `fCombatStealthPointDrainMult` per second while it is 0 or more, and
//! refills them by max(`fCombatStealthPointRegenMin`, −value ·
//! `fCombatStealthPointRegenMult`) per second, up to `fCombatStealthPointMax`, once
//! the alert (or, searching, lost), attacked and detection-event waits have all
//! run. A value above `iCombatStealthPoint(Sneak)DetectionThreshold` empties them
//! at once (`0x14081a0da`); an attack caps them at `…AttackedMaxValue`
//! (`0x14081af9a`), a detection event at `…DetectedEventMaxValue` (`0x14081a4bb`).

use super::{DetectionInputs, DetectionLevel, DetectionSettings, DetectionTerms, SneakPerkHook, SneakWeapon};

pub(super) fn value(s: &DetectionSettings, i: &DetectionInputs, perks: &dyn Fn(SneakPerkHook, f32) -> f32) -> DetectionTerms {
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    let attenuation = super::attenuation(s, i, s.attenuation_exponent);
    let perception = s.perception_skill_min + (s.perception_skill_max - s.perception_skill_min) * i.detector_sneak_skill * 0.01;
    let state = 1.0
        + flag(i.detector_alert) * s.alert_mod
        + flag(i.detector_sleeping) * s.sleep_mod
        + flag(i.detector_not_combat_target) * s.combat_mod;

    let sound_attenuation = if i.line_of_sight { attenuation } else { attenuation * s.sound_los_mult };
    let movement = (s.equipped_weight_base + s.equipped_weight_mult * i.target_weight)
        * i.target_noise_mult.max(0.0)
        * flag(i.target_moving)
        * if i.target_running { s.running_mult } else { 1.0 };
    let action = i.action * s.action_mult.trunc();
    let sleeping = 1.0 + flag(i.detector_sleeping) * s.sleep_bonus;
    let sound = ((action + movement) * s.sounds_mult * sound_attenuation * sleeping).max(0.0);

    let motion = perks(
        SneakPerkHook::DetectionMovement,
        1.0 + flag(i.target_moving) * s.light_move_mult + flag(i.target_running) * s.light_run_mult,
    );
    let level = i.light_level / 100.0 * s.light_level_max
        + if i.exterior { s.light_level_exterior_mod } else { s.light_level_interior_mod };
    let light = perks(SneakPerkHook::DetectionLight, level) * if i.exterior { s.light_exterior_mult } else { s.light_mult }
        + s.light_mod;
    let blindness = if i.detector_sleeping { 0.0 } else { (100.0 - i.detector_blindness) * 0.01 };
    let size = match i.target_size_class {
        0 => s.size_small,
        1 => s.size_normal,
        2 => s.size_large,
        _ => s.size_very_large,
    } * s.large_actor_size_mult;
    let contact = flag(i.sees && !i.target_invisible);
    let visual = (contact * attenuation * motion * light * blindness * size).max(0.0);

    let target = perks(
        SneakPerkHook::DetectionSneakSkill,
        s.player_detection_sneak_mult * i.target_sneak_skill + s.player_detection_sneak_base,
    );
    let skill = (attenuation * state * perception - flag(i.target_sneaking) * target) * s.skill_mult;
    let raw = s.base_value + sound + visual + skill;
    let value = if raw > 0.0 && raw < 1.0 { 1.0 } else { raw.trunc() };
    DetectionTerms { sound, visual, skill, value }
}

/// The value above which a detector finds the player whatever stealth points
/// remain (`0x14081a0da`).
pub(super) fn combat_threshold(s: &DetectionSettings, target_sneaking: bool) -> f32 {
    if target_sneaking { s.stealth_point_sneak_detection_threshold } else { s.stealth_point_detection_threshold }
}

/// The level toward the player (CK *Category:Detection* and *Stealth Points*):
/// a non-hostile detector sees the player at any value above 0 ("Stealth points
/// only apply to 'aggro' detection"); a hostile one once the player's stealth
/// points are gone or its value passes the combat threshold, and until then
/// has only Noticed ("they would detect me if it weren't for my Stealth Points").
pub(super) fn level(s: &DetectionSettings, value: f32, hostile: bool, target_sneaking: bool, points: f32) -> DetectionLevel {
    if value <= 0.0 {
        DetectionLevel::Lost
    } else if !hostile || points <= 0.0 || value > combat_threshold(s, target_sneaking) {
        DetectionLevel::Seen
    } else {
        DetectionLevel::Noticed
    }
}

/// The base sneak-attack multiplier by animation type (`0x14028c410`): the ten
/// `fCombatSneak<Class>Mult` settings, 1 for anything else. Entry 18 then runs
/// over it (`0x1407f1b69`).
pub(super) fn sneak_attack_mult(s: &DetectionSettings, weapon: Option<SneakWeapon>) -> f32 {
    let table = [
        s.sneak_hand,
        s.sneak_one_hand_sword,
        s.sneak_dagger,
        s.sneak_one_hand_axe,
        s.sneak_mace,
        s.sneak_two_hand_sword,
        s.sneak_two_hand_axe,
        s.sneak_bow,
        s.sneak_staff,
        s.sneak_crossbow,
    ];
    weapon.and_then(|w| table.get(usize::from(w.anim_type)).copied()).unwrap_or(1.0)
}

/// The player's stealth points (Skyrim). A resource the detection tick steps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StealthPoints {
    pub points: f32,
    /// Seconds since an aggressive detector last had a value of 0 or more.
    pub since_alert: f32,
    /// Seconds since the player last attacked.
    pub since_attacked: f32,
    /// Seconds since the player's last detection event.
    pub since_event: f32,
    /// Fallout 4: a hostile detector has detected the player (the Lost refill rate and delay apply).
    pub ever_detected: bool,
    /// Fallout 4: the player has attacked (the Hostile refill rate applies).
    pub ever_attacked: bool,
    /// Time not yet stepped (the original steps every 0.1 s).
    pending: f32,
}

impl StealthPoints {
    /// The original's step (`0x14081879a`).
    pub const STEP: f32 = 0.1;

    #[must_use]
    pub fn full(max: f32) -> Self {
        Self {
            points: max,
            since_alert: f32::MAX,
            since_attacked: f32::MAX,
            since_event: f32::MAX,
            ever_detected: false,
            ever_attacked: false,
            pending: 0.0,
        }
    }
}

impl crate::ecs::resource::Resource for StealthPoints {}

/// What one frame of stealth points needs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StealthPointInputs {
    /// The highest value an aggressive detector in range has toward the player.
    pub highest_hostile_value: Option<f32>,
    /// A detector is in combat with the player ("Stealth Points always = 0 when
    /// enemies are in combat with you").
    pub in_combat: bool,
    /// A detector is searching (alert state Lost): the lost wait applies.
    pub searching: bool,
    pub target_sneaking: bool,
    pub attacked: bool,
    pub detection_event: bool,
    /// Fallout 4: the highest visual and sound levels of the hostile detectors in range.
    pub highest_hostile_visual: Option<f32>,
    pub highest_hostile_sound: Option<f32>,
    /// Fallout 4: a hostile detector hearing the player has line of sight (a loud enough sound then drains
    /// past the sound floor).
    pub sound_line_of_sight: bool,
    /// Fallout 4: a hostile detector passed the stealth-point thresholds (the points empty).
    pub over_threshold: bool,
    /// Fallout 4: the nearest hostile detector's distance (the refill slows with its attenuation).
    pub nearest_hostile_distance: Option<f32>,
    pub exterior: bool,
}

pub(super) fn step_points(s: &DetectionSettings, sp: &mut StealthPoints, i: StealthPointInputs, dt: f32) {
    if i.attacked {
        sp.points = sp.points.min(s.stealth_point_attacked_max);
        sp.since_attacked = 0.0;
    }
    if i.detection_event {
        sp.points = sp.points.min(s.stealth_point_event_max);
        sp.since_event = 0.0;
    }
    let value = i.highest_hostile_value;
    if i.in_combat || value.is_some_and(|v| v > combat_threshold(s, i.target_sneaking)) {
        sp.points = 0.0;
    }
    sp.pending += dt;
    // Whole steps, counted with a tolerance so 1.0 s is ten of them, not nine.
    let steps = ((sp.pending + 1e-4) / StealthPoints::STEP).floor();
    sp.pending = (sp.pending - steps * StealthPoints::STEP).max(0.0);
    for _ in 0..steps as u32 {
        let step = StealthPoints::STEP;
        for elapsed in [&mut sp.since_alert, &mut sp.since_attacked, &mut sp.since_event] {
            *elapsed = (*elapsed + step).min(f32::MAX / 2.0);
        }
        if value.is_some_and(|v| v >= 0.0) {
            sp.since_alert = 0.0;
        }
        match value {
            Some(v) if v >= 0.0 && sp.points > 0.0 => {
                sp.points = (sp.points - v * s.stealth_point_drain_mult * step).max(0.0);
            }
            _ if !i.in_combat && sp.points < s.stealth_point_max => {
                let alert_wait = if i.searching { s.stealth_point_regen_lost_wait } else { s.stealth_point_regen_alert_wait };
                let waited = sp.since_alert > alert_wait
                    && sp.since_attacked > s.stealth_point_regen_attacked_wait
                    && sp.since_event > s.stealth_point_regen_event_wait;
                if waited {
                    let rate = (-value.unwrap_or(0.0) * s.stealth_point_regen_mult).max(s.stealth_point_regen_min);
                    sp.points = (sp.points + rate * step).min(s.stealth_point_max);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DetectionInputs, LevelContext, StealthKind, StealthRules};
    use super::*;

    /// `Skyrim.esm`'s authored values (design §13.1) over `SkyrimSE.exe`'s defaults.
    fn rules() -> StealthRules {
        let authored = |name: &str| match name {
            "fSneakBaseValue" => Some(-15.0),
            "fSneakMaxDistance" => Some(2500.0),
            "fSneakExteriorDistanceMult" => Some(2.1),
            "fSneakSoundsMult" => Some(1.0),
            "fSneakSoundLosMult" => Some(0.3),
            "fSneakRunningMult" => Some(2.0),
            "fSneakActionMult" => Some(2.0),
            "fSneakLightMult" => Some(0.33),
            "fSneakLightExteriorMult" => Some(0.5),
            "fDetectionSneakLightMod" => Some(15.0),
            "iLightLevelMax" => Some(300.0),
            "iLightLevelInteriorMod" => Some(25.0),
            "fSneakSkillMult" => Some(0.5),
            "fSneakPerceptionSkillMin" => Some(0.0),
            "fSneakPerceptionSkillMax" => Some(100.0),
            "fSneakAlertMod" => Some(0.0),
            "fSneakSleepBonus" => Some(0.0),
            "fSneakSleepMod" => Some(0.0),
            "fPlayerDetectionSneakBase" => Some(10.0),
            "fPlayerDetectionSneakMult" => Some(0.4),
            "iCombatStealthPointDetectionThreshold" => Some(25.0),
            "fCombatStealthPointDrainMult" => Some(3.0),
            "fCombatStealthPointRegenMin" => Some(5.0),
            "fCombatStealthPointRegenMult" => Some(0.2),
            "fCombatStealthPointRegenAttackedWaitTime" => Some(15.0),
            "fCombatStealthPointRegenDetectedEventWaitTime" => Some(10.0),
            "fCombatSneak1HSwordMult" | "fCombatSneak1HDaggerMult" | "fCombatSneak1HAxeMult" | "fCombatSneak1HMaceMult" => Some(3.0),
            "fCombatSneak2HSwordMult" | "fCombatSneak2HAxeMult" | "fCombatSneakHandMult" => Some(2.0),
            _ => None,
        };
        StealthRules::resolve(StealthKind::Skyrim, authored).0
    }

    /// The CK *Category:Detection* formula as `SkyrimSE.exe` `0x14041cd90`
    /// computes it, written out with `Skyrim.esm`'s constants (PRD §7.2
    /// scenario 2). `perks` doubles the movement multiplier and adds 20 to the
    /// target skill, as `MQ101IncreaseDetection` (48) and Stealth rank 1 (57)
    /// would.
    fn ck(i: &DetectionInputs, perks: bool) -> f32 {
        let range = if i.exterior { 2500.0 * 2.1 } else { 2500.0 };
        let att = ((range - i.distance) / range).max(0.0).powi(2);
        let p = i.detector_sneak_skill; // Min 0, Max 100: P = Sneak
        let state = 1.0 + if i.detector_not_combat_target { -0.4 } else { 0.0 };
        let sound_att = if i.line_of_sight { att } else { att * 0.3 };
        let movement = if i.target_moving { (12.0 + 0.5 * i.target_weight) * if i.target_running { 2.0 } else { 1.0 } } else { 0.0 };
        let sound = ((2.0 * i.action + movement) * sound_att).max(0.0);
        let mut motion = 1.0 + if i.target_moving { 0.01 } else { 0.0 } + if i.target_running { 0.2 } else { 0.0 };
        if perks {
            motion *= 2.0;
        }
        let level = i.light_level * 3.0 + if i.exterior { 0.0 } else { 25.0 };
        let light = level * if i.exterior { 0.5 } else { 0.33 } + 15.0;
        let blind = if i.detector_sleeping { 0.0 } else { (100.0 - i.detector_blindness) / 100.0 };
        let visual = if i.sees && !i.target_invisible { (att * motion * light * blind).max(0.0) } else { 0.0 };
        let mut target = 0.4 * i.target_sneak_skill + 10.0;
        if perks {
            target += 20.0;
        }
        let skill = (att * state * p - if i.target_sneaking { target } else { 0.0 }) * 0.5;
        let raw = -15.0 + sound + visual + skill;
        if raw > 0.0 && raw < 1.0 { 1.0 } else { raw.trunc() }
    }

    fn cases() -> Vec<DetectionInputs> {
        let base = DetectionInputs { detector_sneak_skill: 30.0, light_level: 20.0, ..DetectionInputs::default() };
        vec![
            base,
            DetectionInputs { distance: 900.0, ..base },
            DetectionInputs { distance: 1500.0, exterior: true, light_level: 80.0, ..base },
            DetectionInputs { target_moving: true, target_weight: 40.0, ..base },
            DetectionInputs { target_moving: true, target_running: true, target_weight: 10.0, ..base },
            DetectionInputs { line_of_sight: false, sees: false, action: 50.0, ..base },
            DetectionInputs { target_sneaking: true, target_sneak_skill: 60.0, ..base },
            DetectionInputs { detector_not_combat_target: true, detector_sneak_skill: 80.0, ..base },
            DetectionInputs { detector_sleeping: true, ..base },
            DetectionInputs { detector_blindness: 50.0, ..base },
            DetectionInputs { target_invisible: true, light_level: 100.0, ..base },
        ]
    }

    #[test]
    fn the_skyrim_row_matches_the_ck_formula() {
        let rules = rules();
        for inputs in cases() {
            assert_eq!(rules.value(&inputs).unwrap().value, ck(&inputs, false), "{inputs:?}");
            let perks = |hook, v: f32| match hook {
                SneakPerkHook::DetectionMovement => v * 2.0,
                SneakPerkHook::DetectionSneakSkill => v + 20.0,
                SneakPerkHook::DetectionLight => v,
            };
            assert_eq!(rules.value_with(&inputs, &perks).unwrap().value, ck(&inputs, true), "{inputs:?} with perks");
        }
        assert_eq!(rules.view_cone_degrees(), Some(190.0));
        let period = (rules.period(0.0), rules.period(1.0));
        assert_eq!(period, (1.0, 2.0), "fDetectionUpdateTimeMin / Max");
    }

    /// SNK-PERK-004: entry 47 scales the light level before the multipliers apply.
    #[test]
    fn mod_detection_light_scales_the_light_level() {
        let rules = rules();
        let inputs = DetectionInputs { detector_sneak_skill: 30.0, light_level: 50.0, ..DetectionInputs::default() };
        let plain = rules.value(&inputs).unwrap().visual;
        let halved = rules
            .value_with(&inputs, &|hook, v| if hook == SneakPerkHook::DetectionLight { v * 0.5 } else { v })
            .unwrap()
            .visual;
        let light = |level: f32| level * 0.33 + 15.0;
        assert!((halved / plain - light(175.0 / 2.0) / light(175.0)).abs() < 1e-4);
    }

    /// SNK-DET-015: a hostile detector above 0 is held at Noticed until the
    /// points run out or its value passes the threshold; a friendly one sees.
    #[test]
    fn the_level_follows_the_stealth_points() {
        let rules = rules();
        let ctx = |hostile, points| LevelContext { hostile, target_sneaking: true, stealth_points: points, ..LevelContext::default() };
        assert_eq!(rules.level(0.0, ctx(true, 0.0)), DetectionLevel::Lost);
        assert_eq!(rules.level(10.0, ctx(false, 100.0)), DetectionLevel::Seen, "not aggro: no stealth points");
        assert_eq!(rules.level(10.0, ctx(true, 100.0)), DetectionLevel::Noticed);
        assert_eq!(rules.level(10.0, ctx(true, 0.0)), DetectionLevel::Seen);
        assert_eq!(rules.level(71.0, ctx(true, 100.0)), DetectionLevel::Seen, "above the sneaking threshold (70)");
        let standing = LevelContext { target_sneaking: false, ..ctx(true, 100.0) };
        assert_eq!(rules.level(26.0, standing), DetectionLevel::Seen, "above the standing threshold (25)");
        assert!(rules.noticed(1.0) && !rules.noticed(0.0));
        assert!(rules.alert_expired(0.0, 100.0) && !rules.alert_expired(1000.0, 99.0));
    }

    #[test]
    fn stealth_points_drain_refill_after_the_waits_and_empty_in_combat() {
        let rules = rules();
        let mut sp = StealthPoints::full(100.0);
        let seen = StealthPointInputs {
            highest_hostile_value: Some(10.0),
            in_combat: false,
            searching: false,
            target_sneaking: true,
            attacked: false,
            detection_event: false,
            ..StealthPointInputs::default()
        };
        rules.step_stealth_points(&mut sp, seen, 1.0);
        assert!((sp.points - 70.0).abs() < 1e-3, "10 · 3 per second: {}", sp.points);
        let hidden = StealthPointInputs { highest_hostile_value: Some(-40.0), ..seen };
        rules.step_stealth_points(&mut sp, hidden, 5.0);
        assert!((sp.points - 70.0).abs() < 1e-3, "the alert wait (10 s) holds the refill");
        rules.step_stealth_points(&mut sp, hidden, 6.0);
        // 1 s past the wait, at max(5, 40 · 0.2) = 8 per second.
        assert!(sp.points > 70.0 && sp.points < 80.0, "{}", sp.points);
        rules.step_stealth_points(&mut sp, StealthPointInputs { attacked: true, ..hidden }, 0.0);
        assert!(sp.points <= 50.0, "an attack caps them at fCombatStealthPointAttackedMaxValue");
        rules.step_stealth_points(&mut sp, StealthPointInputs { highest_hostile_value: Some(80.0), ..seen }, 0.0);
        assert_eq!(sp.points, 0.0, "above the threshold empties them at once");
        let mut fighting = StealthPoints::full(100.0);
        rules.step_stealth_points(&mut fighting, StealthPointInputs { in_combat: true, ..hidden }, 0.1);
        assert_eq!(fighting.points, 0.0);
    }

    /// PRD §7.2 scenario 8 (Skyrim, SNK-ATK-004, SNK-PERK-005).
    #[test]
    fn a_skyrim_sneak_attack_takes_the_class_multiplier_through_entry_18() {
        let rules = rules();
        let weapon = |anim_type| Some(SneakWeapon { anim_type, ..SneakWeapon::default() });
        let plain = |weapon| rules.sneak_attack(10.0, weapon, 0.0, &|m| m).unwrap();
        assert_eq!(plain(weapon(2)), (30.0, true), "dagger ×3 (Skyrim.esm)");
        assert_eq!(plain(weapon(5)), (20.0, true), "two-handed ×2");
        assert_eq!(plain(weapon(7)), (20.0, true), "bow ×2 (executable)");
        assert_eq!(plain(weapon(8)), (10.0, false), "staff ×1: not flagged");
        assert_eq!(plain(None), (10.0, false), "no weapon: 1");
        // Assassin's Blade: "a total of fifteen times" on a dagger (×5 over the base ×3).
        assert_eq!(rules.sneak_attack(10.0, weapon(2), 0.0, &|m| m * 5.0).unwrap(), (150.0, true));
        assert!(rules.unaware(0.0, true) && !rules.unaware(1.0, false));
    }
}
