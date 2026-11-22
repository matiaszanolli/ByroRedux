//! The Fallout 3 / New Vegas detection value (`Fallout3.exe` `0x57e300`;
//! design §4.2) and sneak-attack damage (`0x845580`, `0x845980`; design §8).

use super::{DetectionInputs, DetectionSettings, DetectionTerms, SneakWeapon};

/// The GECK-shaped value; `level_terms` adds New Vegas's target-skill level and
/// armor terms. #7213 — Fallout 3's target skill has neither: the GECK wiki's
/// *Category:Detection* (Target Skill) gives "In FO3, the Target Skill is simply
/// `Sneak * IsSneaking`", and New Vegas's form adds the level terms *and* the
/// armor penalty together.
pub(super) fn value(s: &DetectionSettings, i: &DetectionInputs, level_terms: bool) -> DetectionTerms {
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    let attenuation = super::attenuation(s, i, s.attenuation_exponent);
    let perception = s.perception_skill_min + (s.perception_skill_max - s.perception_skill_min) * i.detector_perception / 10.0;
    let state = 1.0
        + flag(i.detector_alert) * s.alert_mod
        + flag(i.detector_sleeping) * s.sleep_bonus
        + flag(i.detector_not_combat_target) * s.combat_mod;

    let los = if i.line_of_sight { 1.0 } else { s.sound_los_mult };
    let movement = if i.target_moving {
        (s.boot_weight_base + s.boot_weight_mult * i.target_weight) * if i.target_running { s.running_mult } else { 1.0 }
    } else {
        0.0
    };
    let sound = (attenuation * los * s.sounds_mult * (i.action * s.action_mult.trunc() + movement)).max(0.0);

    let visual = if i.sees && !i.target_stealth_boy {
        let night_eye = if i.detector_night_eye { s.night_eye_bonus } else { 1.0 };
        let light = (i.light_level * night_eye).min(100.0);
        let motion = 1.0 + flag(i.target_moving) * s.light_move_mult + flag(i.target_running) * s.light_run_mult;
        // `Fallout3.exe` divides as integers: any blindness of 1 or more zeroes sight.
        let blindness = f32::from(u8::from((100.0 - i.detector_blindness.clamp(0.0, 100.0)) >= 100.0));
        let size = if i.target_size > 1.0 { 1.0 + (i.target_size - 1.0) * s.large_actor_size_mult } else { 1.0 };
        (attenuation * motion * (light + s.light_mod) * s.light_mult * blindness * size).max(0.0)
    } else {
        // A Stealth Boy removes visual contact (`Fallout3.exe` also multiplies
        // by `fSneakStealthBoyMult`, 0 in every census row).
        0.0
    };

    let target = if i.target_sneaking {
        let mut skill = i.target_sneak_skill;
        if level_terms {
            let (target_level, detector_level) = (f32::from(i.target_level), f32::from(i.detector_level));
            skill += 5.0 * (target_level - detector_level) + (50.0 - 10.0 * target_level).max(0.0)
                - i.target_armor.penalty();
        }
        skill
    } else {
        0.0
    };
    let skill = (attenuation * perception * state - target) * s.skill_mult;
    DetectionTerms { sound, visual, skill, value: s.base_value + sound + visual + skill }
}

/// A sneak attack's damage (SNK-ATK-003). It is a critical unless the
/// weapon's Crit % Mult is 0 ("If set to zero, the weapon will not cause any
/// critical hits, even for sneak attacks", GECK *Weapons*): the weapon's
/// Critical Damage, or for an unarmed strike the damage ×
/// `fCombatUnarmedCritDamageMult` (`0x845753`). Then ×
/// `fCombatDamageBonusMeleeSneakingMult` for unarmed and melee weapons
/// (animation types 0–2), `fCombatDamageBonusSneakingMult` otherwise.
pub(super) fn sneak_attack_damage(s: &DetectionSettings, damage: f32, weapon: Option<SneakWeapon>) -> f32 {
    let critical = match weapon {
        Some(weapon) if weapon.critical_chance_mult > 0.0 => weapon.critical_damage,
        Some(_) => 0.0,
        None => damage * s.unarmed_crit_damage_mult,
    };
    let melee = weapon.is_none_or(|weapon| weapon.anim_type <= 2);
    let mult = if melee { s.melee_sneak_damage_mult } else { s.sneak_damage_mult };
    (damage + critical) * mult
}
