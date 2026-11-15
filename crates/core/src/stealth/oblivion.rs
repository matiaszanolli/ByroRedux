//! Oblivion's detection value, level and sneak attack (SNK-DET-005, -012,
//! SNK-ATK-002; design §4.2, §4.3, §8).
//!
//! The value is the CS formula (*Category:Detection*), each term scaled by the
//! linear "Distance Factor … 1 - (distance between detector and target) /
//! MaxDetectionRange":
//!
//! ```text
//! value  = fSneakBaseValue [+ fSneakTargetAttackBonus, the target attacked this detector]
//!          [+ sound, target not swimming] [+ fSneakSleepBonus, detector asleep]
//!          [+ sight, detector awake with line of sight] + skill
//!          (−100 for an invisible target or one at 100 % Chameleon)
//! sound  = max(0, fSneakSoundsMult · DF · ([moving, mastery < Expert] movement
//!          [+ fSneakTargetInCombatBonus, target in combat])) [· fSneakSoundLosMult, no LOS]
//! movement = (fSneakBootWeightBase [+ fSneakBootWeightMult · boots, mastery < Journeyman])
//!          [· fSneakRunningMult, running]
//! sight  = max(0, fSneakLightMult · DF · (fDetectionSneakLightMod + light [· fDetectionNightEyeBonus])
//!          · (1 − Chameleon/100) · (1 − Blindness/100)) [· fSneakSwimmingLightMult, swimming]
//! skill  = fSneakSkillMult · (detector Sneak · DF [− target Sneak, sneaking and not in combat])
//! ```
//!
//! The level is `Oblivion.exe` `0x640900`: Seen above `fSneakSeenMin`, Noticed
//! between `fSneakNoticedMin` and it, Lost otherwise. Its jump table switches on a level
//! computed from the same value a few instructions earlier, so the Unseen and
//! `fSneakUnseenMin` / `fSneakLostMin` arms are unreachable from it (V10).

use super::{DetectionInputs, DetectionLevel, DetectionSettings, DetectionTerms, SneakWeapon};

/// Oblivion's skill mastery (Novice 0–24, Apprentice 25–49, Journeyman 50–74,
/// Expert 75–99, Master 100; UESP *Oblivion:Skills*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mastery {
    Novice,
    Apprentice,
    Journeyman,
    Expert,
    Master,
}

impl Mastery {
    #[must_use]
    pub fn of(skill: f32) -> Self {
        match skill {
            s if s >= 100.0 => Self::Master,
            s if s >= 75.0 => Self::Expert,
            s if s >= 50.0 => Self::Journeyman,
            s if s >= 25.0 => Self::Apprentice,
            _ => Self::Novice,
        }
    }
}

pub(super) fn value(s: &DetectionSettings, i: &DetectionInputs) -> DetectionTerms {
    if i.target_invisible || i.target_chameleon >= 100.0 {
        return DetectionTerms { value: -100.0, ..DetectionTerms::default() };
    }
    let df = super::attenuation(s, i, 1.0);
    let mastery = Mastery::of(i.target_sneak_skill);
    let sound = if i.target_swimming {
        0.0
    } else {
        let mut movement = 0.0;
        if i.target_moving && mastery < Mastery::Expert {
            movement = s.boot_weight_base;
            if mastery < Mastery::Journeyman {
                movement += s.boot_weight_mult * i.target_boot_weight;
            }
            if i.target_running {
                movement *= s.running_mult;
            }
        }
        let combat = if i.target_in_combat { s.target_in_combat_bonus } else { 0.0 };
        let los = if i.line_of_sight { 1.0 } else { s.sound_los_mult };
        (s.sounds_mult * df * (movement + combat)).max(0.0) * los
    };
    let sight = if !i.detector_sleeping && i.sees {
        let night_eye = if i.detector_night_eye { s.night_eye_bonus } else { 1.0 };
        let light = s.light_mod + i.light_level * night_eye;
        let chameleon = (1.0 - i.target_chameleon / 100.0).clamp(0.0, 1.0);
        let blindness = (1.0 - i.detector_blindness / 100.0).clamp(0.0, 1.0);
        let swimming = if i.target_swimming { s.swimming_light_mult } else { 1.0 };
        (s.light_mult * df * light * chameleon * blindness).max(0.0) * swimming
    } else {
        0.0
    };
    let target = if i.target_sneaking && !i.target_in_combat { i.target_sneak_skill } else { 0.0 };
    let skill = s.skill_mult * (i.detector_sneak_skill * df - target);
    let sleeping = if i.detector_sleeping { s.sleep_bonus } else { 0.0 };
    let attacked = if i.target_attacked_detector { s.target_attack_bonus } else { 0.0 };
    DetectionTerms { sound, visual: sight, skill, value: s.base_value + attacked + sound + sleeping + sight + skill }
}

/// `Oblivion.exe` `0x640900` (V10): a value exactly at `fSneakSeenMin` is Lost
/// (the `0x640a35` arm stores 0 for a value not below it).
pub(super) fn level(s: &DetectionSettings, value: f32) -> DetectionLevel {
    if value > s.seen_min {
        DetectionLevel::Seen
    } else if value < s.seen_min && value > s.noticed_min {
        DetectionLevel::Noticed
    } else {
        DetectionLevel::Lost
    }
}

/// The sneak-attack bar (`Oblivion.exe` `0x5ff23a`–`0x5ff278`; V12): the
/// target's detection value toward the attacker is at most 0, and at most
/// `iAICombatMinDetection` while the target is fighting the attacker.
pub(super) fn unaware(s: &DetectionSettings, value: f32, fighting_attacker: bool) -> bool {
    value <= 0.0 && !(fighting_attacker && value > s.ai_combat_min_detection)
}

/// The sneak-attack multiplier (`Oblivion.exe` `0x5477f0`; V13): the melee
/// ladder for one-handed weapons and hand-to-hand, the marksman ladder for
/// bows, 1 for two-handed weapons and staves (`DATA` type 0 Blade 1H, 1 Blade
/// 2H, 2 Blunt 1H, 3 Blunt 2H, 4 Staff, 5 Bow). Spells never reach it.
pub(super) fn sneak_attack_mult(s: &DetectionSettings, weapon: Option<SneakWeapon>, sneak_skill: f32) -> f32 {
    let ladder = |l: [f32; 5]| l[Mastery::of(sneak_skill) as usize];
    let melee = [s.sneak_melee_novice, s.sneak_melee_apprentice, s.sneak_melee_journeyman, s.sneak_melee_expert, s.sneak_melee_master];
    let marksman = [
        s.sneak_marksman_novice,
        s.sneak_marksman_apprentice,
        s.sneak_marksman_journeyman,
        s.sneak_marksman_expert,
        s.sneak_marksman_master,
    ];
    match weapon.map(|w| w.oblivion_type) {
        None | Some(0) | Some(2) => ladder(melee),
        Some(5) => ladder(marksman),
        Some(_) => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DetectionInputs, LevelContext, SneakWeapon, StealthKind, StealthRules};
    use super::*;

    /// `Oblivion.esm`'s authored values (design §13.1) over `Oblivion.exe`'s defaults.
    fn rules() -> StealthRules {
        let authored = |name: &str| match name {
            "fSneakBaseValue" => Some(-25.0),
            "fSneakMaxDistance" => Some(1500.0),
            "fSneakSoundLosMult" => Some(1.0),
            "fSneakBootWeightBase" => Some(14.0),
            "fSneakBootWeightMult" => Some(1.0),
            "fSneakRunningMult" => Some(1.3),
            "fSneakLightMult" => Some(1.4),
            "fDetectionSneakLightMod" => Some(-5.0),
            "fSneakSkillMult" => Some(0.5),
            "fSneakSleepBonus" => Some(-10.0),
            "fSneakTargetInCombatBonus" => Some(25.0),
            "fSneakTargetAttackBonus" => Some(100.0),
            "fPerkSneakAttackMarksmanNoviceMult" => Some(2.0),
            "fPerkSneakAttackMarksmanApprenticeMult" => Some(3.0),
            "fPerkSneakAttackMarksmanJourneymanMult" => Some(3.0),
            "fPerkSneakAttackMarksmanExpertMult" => Some(3.0),
            "fPerkSneakAttackMarksmanMasterMult" => Some(3.0),
            _ => None,
        };
        StealthRules::resolve(StealthKind::Oblivion, authored).0
    }

    /// The CS *Category:Detection* table, written out with `Oblivion.esm`'s
    /// constants (PRD §7.2 scenario 2).
    fn cs(i: &DetectionInputs) -> f32 {
        if i.target_invisible || i.target_chameleon >= 100.0 {
            return -100.0;
        }
        let range = if i.exterior { 3000.0 } else { 1500.0 };
        let df = (1.0 - i.distance / range).max(0.0);
        let novice_to_apprentice = i.target_sneak_skill < 50.0;
        let below_expert = i.target_sneak_skill < 75.0;
        let mut movement = 0.0;
        if i.target_moving && below_expert {
            movement = 14.0 + if novice_to_apprentice { i.target_boot_weight } else { 0.0 };
            if i.target_running {
                movement *= 1.3;
            }
        }
        let sound = if i.target_swimming {
            0.0
        } else {
            (1.6 * df * (movement + if i.target_in_combat { 25.0 } else { 0.0 })).max(0.0)
        };
        let sight = if !i.detector_sleeping && i.line_of_sight {
            let light = -5.0 + i.light_level * if i.detector_night_eye { 3.0 } else { 1.0 };
            let v = 1.4 * df * light * (1.0 - i.target_chameleon / 100.0) * (1.0 - i.detector_blindness / 100.0);
            v.max(0.0) * if i.target_swimming { 0.5 } else { 1.0 }
        } else {
            0.0
        };
        let target = if i.target_sneaking && !i.target_in_combat { i.target_sneak_skill } else { 0.0 };
        -25.0 + if i.target_attacked_detector { 100.0 } else { 0.0 } + sound
            + if i.detector_sleeping { -10.0 } else { 0.0 }
            + sight
            + 0.5 * (i.detector_sneak_skill * df - target)
    }

    #[test]
    fn the_oblivion_row_matches_the_cs_formula() {
        let rules = rules();
        let base = DetectionInputs { detector_sneak_skill: 40.0, light_level: 50.0, ..DetectionInputs::default() };
        let cases = [
            base,
            DetectionInputs { distance: 700.0, ..base },
            DetectionInputs { distance: 1200.0, exterior: true, target_moving: true, target_boot_weight: 20.0, ..base },
            DetectionInputs { target_moving: true, target_running: true, target_sneak_skill: 60.0, ..base },
            DetectionInputs { target_moving: true, target_sneak_skill: 80.0, target_boot_weight: 30.0, ..base },
            DetectionInputs { line_of_sight: false, sees: false, target_in_combat: true, ..base },
            DetectionInputs { target_sneaking: true, target_sneak_skill: 55.0, ..base },
            DetectionInputs { target_sneaking: true, target_sneak_skill: 55.0, target_in_combat: true, ..base },
            DetectionInputs { detector_sleeping: true, ..base },
            DetectionInputs { detector_night_eye: true, light_level: 10.0, target_chameleon: 40.0, detector_blindness: 25.0, ..base },
            DetectionInputs { target_swimming: true, target_moving: true, ..base },
            DetectionInputs { target_attacked_detector: true, ..base },
            DetectionInputs { target_chameleon: 100.0, ..base },
        ];
        for inputs in cases {
            let got = rules.value(&inputs).unwrap().value;
            assert!((got - cs(&inputs)).abs() < 1e-3, "{inputs:?}: {got} vs {}", cs(&inputs));
        }
        assert_eq!(rules.view_cone_degrees(), None, "no cone: the CS formula reads line of sight only");
    }

    /// V10: Seen above 0, Noticed strictly between −20 and 0, Lost otherwise.
    #[test]
    fn the_level_follows_the_classifier() {
        let rules = rules();
        let level = |v| rules.level(v, LevelContext::default());
        assert_eq!(level(0.5), DetectionLevel::Seen);
        assert_eq!(level(0.0), DetectionLevel::Lost, "at fSneakSeenMin");
        assert_eq!(level(-0.5), DetectionLevel::Noticed);
        assert_eq!(level(-19.9), DetectionLevel::Noticed);
        assert_eq!(level(-20.0), DetectionLevel::Lost);
        assert!(rules.noticed(-19.0) && !rules.noticed(-20.0));
    }

    /// PRD §7.2 scenario 8 (Oblivion, SNK-ATK-002; V12, V13).
    #[test]
    fn an_oblivion_sneak_attack_takes_the_mastery_ladder() {
        let rules = rules();
        let weapon = |oblivion_type| Some(SneakWeapon { oblivion_type, ..SneakWeapon::default() });
        let attack = |weapon, skill| rules.sneak_attack(10.0, weapon, skill, &|m| m).unwrap();
        assert_eq!(attack(weapon(0), 10.0), (40.0, true), "blade 1H, Novice: executable 4");
        assert_eq!(attack(weapon(2), 60.0), (60.0, true), "blunt 1H, Journeyman: 6");
        assert_eq!(attack(None, 100.0), (60.0, true), "hand-to-hand takes the melee ladder");
        assert_eq!(attack(weapon(5), 10.0), (20.0, true), "bow, Novice: Oblivion.esm's 2");
        assert_eq!(attack(weapon(5), 30.0), (30.0, true), "bow, Apprentice: 3");
        assert_eq!(attack(weapon(1), 100.0), (10.0, true), "two-handed: no bonus");
        assert_eq!(attack(weapon(4), 100.0), (10.0, true), "staff: no bonus");
        assert!(rules.unaware(0.0, false) && !rules.unaware(1.0, false));
        assert!(!rules.unaware(-30.0, true), "fighting the attacker: above iAICombatMinDetection is aware");
        assert!(rules.unaware(-60.0, true));
    }

    #[test]
    fn the_oblivion_census_reads_its_own_settings() {
        let (_, readings) = StealthRules::resolve(StealthKind::Oblivion, |_| None);
        let names: Vec<_> = readings.iter().map(|r| r.name).collect();
        assert!(names.contains(&"fSneakTargetInCombatBonus") && names.contains(&"fPerkSneakAttackMeleeMasterMult"));
        assert!(!names.contains(&"fSneakDistanceAttenuationExponent"), "Oblivion has no such setting");
        assert!(!names.contains(&"fDetectionViewCone"));
        let default = |name| readings.iter().find(|r| r.name == name).unwrap().value;
        assert_eq!((default("fSneakSkillMult"), default("fSneakBootWeightBase")), (0.75, 7.0), "Oblivion.exe's own defaults");
    }
}
