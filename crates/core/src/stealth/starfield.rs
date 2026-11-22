//! Starfield's detection pair (#7046), read from `Starfield.exe`. It is Fallout 4's model with its coefficients
//! re-based to metres and a handful of changes, so it reuses [`super::fallout4`]'s range, falloff, level and
//! stealth-point code and keeps only what differs here. The decode (addresses, the context struct, every
//! default that moved) is in `.claude/issues/7046/INVESTIGATION.md`; the engine-side summary is design §4.2.
//!
//! **The pair** (`0x140d7e2a0`; its inputs are filled by `0x141940c10`, its levels rounded up by the caller at
//! `0x141942618`, with 0 counting as −1, as Fallout 4's):
//!
//! ```text
//! range    = as Fallout 4 (fSneakMaxDistance · exterior · size), the sound range
//! visualR  = range · fSneakMaxDistanceVisualMult, then the angle falloff lerp(angle, AngleMin → AngleMax, 1 → MultExterior/Interior),
//!            the pitch falloffs of Fallout 4, and the visibility factor
//! sound    = fSneakSoundBaseValue                                       (Deafness ≥ 100: nothing more)
//!            + (att · los · fSneakSoundsMult · (s + action · fSneakActionMult) − ambient)
//!              · (1 + asleep · fSneakSleepBonus) · (100 − Deafness) / 100          no clamp: it can fall below the base
//!            s       = max(0, [moving] · (att · los · D − A) · fSneakSkillMovementMult + move)
//!            move    = moving · (fSneakEquippedWeightBase + Mult · weight) · max(0, noise) · speed
//!            speed   = sprinting ? fDetectionSprintingMult : running ? (sneaking ? fSneakRunningMult : fDetectionRunningMult) : 1
//!            ambient = trunc(fSneakAmbientSoundMult · iSoundLevel{Silent, Quiet, Normal, Loud, VeryLoud}[class − 1]), 0 for class 0
//! visual   = fSneakVisualBaseValue                                      (¬sees, asleep or a Stealth Boy: nothing more)
//!            as Fallout 4's, over `visualR` and with the light level taken whole (Starfield has no light-level mods)
//! ```
//!
//! What stays neutral in the engine: the ambient sound class (the source field of the cell is not identified),
//! the sky / weather factor the filler multiplies into the visibility, the combat set of the visual-range
//! falloffs (which pair selects it is not read), the ambush mode, the observed-target and flying range
//! multipliers, and the 0.75 non-target range factor (the engine evaluates the player as the only target).

use super::{fallout4, DetectionInputs, DetectionSettings, DetectionTerms, SneakPerkHook};

/// `iSoundLevel*` of an ambient sound class 1 to 5 (`0x140d7dd80`); anything else is 0.
fn ambient_level(s: &DetectionSettings, class: u8) -> f32 {
    let level = match class {
        1 => s.sound_level_silent,
        2 => s.sound_level_quiet,
        3 => s.sound_level_normal,
        4 => s.sound_level_loud,
        5 => s.sound_level_very_loud,
        _ => return 0.0,
    };
    (s.ambient_sound_mult * level).trunc()
}

pub(super) fn value(s: &DetectionSettings, i: &DetectionInputs, perks: &dyn Fn(SneakPerkHook, f32) -> f32) -> DetectionTerms {
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    let deg = std::f32::consts::PI / 180.0;
    let range = fallout4::range(s, i.exterior, i.target_size_class);
    let visual_max = range * s.max_distance_visual_mult;
    let d = i.distance;
    let angle = i.view_angle.abs();

    // Line of sight is tested only inside the off-axis LOS distance, taken over the visual range (`0x1419416cf`).
    let in_combat = i.detector_not_combat_target || i.detector_fighting_target;
    let los_reach = if !i.detector_fighting_target && angle > s.los_distance_angle * deg {
        let mult = if i.exterior { s.los_distance_mult_exterior } else { s.los_distance_mult_interior };
        fallout4::lerp(angle, s.los_distance_angle * deg, std::f32::consts::PI, 1.0, mult)
    } else {
        1.0
    };
    let line_of_sight = i.line_of_sight && d <= visual_max * los_reach;
    let sees = i.sees && line_of_sight;

    // The visual range: the angle falloff runs from AngleMin to AngleMax (`0x1419420ab`), the pitch falloffs are
    // Fallout 4's, and the visibility factor multiplies the result (`0x1419421b0`).
    let mut visual_range = visual_max;
    if angle > s.visual_distance_angle_min * deg {
        let mult = if i.exterior { s.visual_distance_mult_exterior } else { s.visual_distance_mult_interior };
        visual_range *= fallout4::lerp(angle, s.visual_distance_angle_min * deg, s.visual_distance_angle_max * deg, 1.0, mult);
    }
    let pitch = i.view_pitch;
    if pitch > s.visual_distance_angle_up_min * deg {
        visual_range *= fallout4::lerp(pitch, s.visual_distance_angle_up_min * deg, s.visual_distance_angle_up_max * deg, 1.0, s.visual_distance_up_mult);
    } else if pitch < -s.visual_distance_angle_down_min * deg {
        visual_range *=
            fallout4::lerp(pitch, -s.visual_distance_angle_down_min * deg, -s.visual_distance_angle_down_max * deg, 1.0, s.visual_distance_down_mult);
    }
    let stealth_boy = i.target_invisibility >= 100.0;
    let mut visible = if stealth_boy { 1.0 } else { (100.0 - i.target_invisibility) * 0.01 };
    if i.target_occluders > 0 {
        let n = f32::from(i.target_occluders);
        visible *= (s.object_visibility_mult - (n - 1.0) * s.object_visibility_mult_offset).max(s.object_visibility_mult_min);
    }
    if i.target_in_cover {
        visible *= s.visual_cover_mod;
    }
    visual_range *= visible;

    let attenuation = |max: f32| if max > 0.0 { ((max - d) / max).max(0.0).powf(s.attenuation_exponent) } else { 0.0 };

    // Entry 15 silences running and sprinting, and movement while sneaking (`0x1419419b9`).
    let running = i.target_running && !i.target_silent_movement;
    let sprinting = i.target_sprinting && !i.target_silent_movement;
    let moving = i.target_moving && !(i.target_silent_movement && i.target_sneaking);

    let perception = s.perception_skill_min + (s.perception_skill_max - s.perception_skill_min) * i.detector_perception * 0.1;
    let detector = perception
        * (1.0 + flag(in_combat) * s.alert_mod + flag(i.detector_sleeping) * s.sleep_mod + flag(i.detector_not_combat_target) * s.combat_mod);
    let agility = if i.target_sneaking {
        let sneak = perks(SneakPerkHook::DetectionSneakSkill, i.target_sneak_skill);
        s.agility_skill_min + sneak + (s.agility_skill_max - s.agility_skill_min) * i.target_agility * 0.1
    } else {
        0.0
    };

    let mut sound = s.sound_base_value;
    if i.detector_deafness < 100.0 {
        let heard = attenuation(range) * if line_of_sight { 1.0 } else { s.sound_los_mult };
        let speed = if sprinting {
            s.detection_sprinting_mult
        } else if running {
            if i.target_sneaking { s.running_mult } else { s.detection_running_mult }
        } else {
            1.0
        };
        let movement = flag(moving)
            * (s.equipped_weight_base + s.equipped_weight_mult * i.target_weight)
            * i.target_noise_mult.max(0.0)
            * speed;
        let skill = (flag(moving) * (heard * detector - agility) * s.skill_movement_mult + movement).max(0.0);
        sound += (s.sounds_mult * heard * (skill + i.action * s.action_mult) - ambient_level(s, i.ambient_sound_class))
            * (1.0 + flag(i.detector_sleeping) * s.sleep_bonus)
            * (100.0 - i.detector_deafness)
            * 0.01;
    }

    let mut visual = s.visual_base_value;
    if sees && !i.detector_sleeping && !stealth_boy {
        let seen = attenuation(visual_range);
        let motion = perks(SneakPerkHook::DetectionMovement, flag(moving) * s.light_move_mult + flag(running) * s.light_run_mult);
        let level = i.light_level * if i.detector_night_eye { s.night_eye_bonus } else { 1.0 };
        let level = perks(SneakPerkHook::DetectionLight, level.min(s.light_level_max));
        let light = level * if i.exterior { s.light_exterior_mult } else { s.light_mult } + s.light_mod;
        let size = s.large_actor_distance_mult * fallout4::size_mult(s, i.target_size_class);
        let darkness = if s.light_level_max > 0.0 { ((s.light_level_max - level) / s.light_level_max).max(0.0) } else { 0.0 };
        visual += (1.0 + motion) * seen * light * (100.0 - i.detector_blindness) * 0.01 * size + seen * detector * s.skill_mult
            - darkness * agility * s.skill_mult;
        if visual < s.visual_min_value && d > 0.0 && d < s.visual_min_distance_max && angle < s.visual_min_angle * deg {
            let t = ((d - s.visual_min_distance_max) / (s.visual_min_distance_min - s.visual_min_distance_max)).clamp(0.0, 1.0);
            visual += t * (s.visual_min_value - visual);
        }
    }

    let (sound, visual) = (fallout4::level_of(sound), fallout4::level_of(visual));
    DetectionTerms { sound, visual, skill: -agility, value: sound.max(visual) }
}

#[cfg(test)]
mod tests {
    use super::super::{DetectionInputs, StealthKind, StealthRules};
    use super::*;

    /// `Starfield.exe`'s defaults, its distances lifted from metres to Bethesda units at resolve (×70).
    fn rules() -> StealthRules {
        StealthRules::resolve(StealthKind::Starfield, |_| None).0
    }

    /// The decoded formula (`0x140d7e2a0` with the pair from `0x141940c10`) written out with `Starfield.exe`'s
    /// constants, as the transcription oracle (PRD §7.2 scenario 2). `perks` adds 10 to the light level, doubles
    /// the light-movement term and adds 20 to the target's Sneak (entries 47, 48 and 57).
    fn exe(i: &DetectionInputs, perks: bool) -> (f32, f32) {
        let rad = std::f32::consts::PI / 180.0;
        let size = [0.5, 1.0, 1.5, 2.0][usize::from(i.target_size_class.min(3))];
        let range = 64.0 * 70.0 * if i.exterior { 1.25 } else { 1.0 } * if size > 1.0 { size } else { 1.0 };
        let visual_max = range * 1.5;
        let angle = i.view_angle.abs();
        let los_reach = if !i.detector_fighting_target && angle > 45.0 * rad {
            1.0 - 0.5 * ((angle - 45.0 * rad) / (180.0 * rad - 45.0 * rad)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let los = i.line_of_sight && i.distance <= visual_max * los_reach;
        let sees = i.sees && los;
        let mut visual_range = visual_max;
        if angle > 30.0 * rad {
            visual_range *= 1.0 - 0.8 * ((angle - 30.0 * rad) / (70.0 * rad - 30.0 * rad)).clamp(0.0, 1.0);
        }
        if i.view_pitch > 5.0 * rad {
            visual_range *= 1.0 - 0.9 * ((i.view_pitch - 5.0 * rad) / (25.0 * rad)).clamp(0.0, 1.0);
        } else if i.view_pitch < -10.0 * rad {
            visual_range *= 1.0 - 0.9 * ((i.view_pitch + 10.0 * rad) / (-35.0 * rad)).clamp(0.0, 1.0);
        }
        let boy = i.target_invisibility >= 100.0;
        visual_range *= if boy { 1.0 } else { (100.0 - i.target_invisibility) / 100.0 };
        let att = |max: f32| ((max - i.distance) / max).max(0.0).powi(2);
        let p = 10.0 + 80.0 * i.detector_perception / 10.0;
        // fSneakAlertMod and fSneakSleepMod are 0: only the not-combat-target −0.4 remains.
        let d = p * (1.0 + if i.detector_not_combat_target { -0.4 } else { 0.0 });
        let a = if i.target_sneaking { i.target_sneak_skill + if perks { 20.0 } else { 0.0 } + 50.0 * i.target_agility / 10.0 } else { 0.0 };
        let mut sound = -20.0;
        if i.detector_deafness < 100.0 {
            let heard = att(range) * if los { 1.0 } else { 0.3 };
            // fDetectionSprintingMult, fSneakRunningMult and fDetectionRunningMult are all 2 at the defaults.
            let speed = if i.target_sprinting || i.target_running { 2.0 } else { 1.0 };
            let movement = if i.target_moving { (12.0 + 0.5 * i.target_weight) * speed } else { 0.0 };
            let skill = if i.target_moving { (heard * d - a) * 0.25 } else { 0.0 };
            let ambient = [0.0, 10.0, 25.0, 50.0, 100.0, 200.0][usize::from(i.ambient_sound_class.min(5))];
            sound += (heard * ((skill + movement).max(0.0) + i.action) - ambient) * (100.0 - i.detector_deafness) / 100.0;
        }
        let mut visual = -20.0;
        if sees && !i.detector_sleeping && !boy {
            let seen = att(visual_range);
            let lm = (if i.target_moving { 0.2 } else { 0.0 } + if i.target_running { 0.4 } else { 0.0 }) * if perks { 2.0 } else { 1.0 };
            let level = (i.light_level * if i.detector_night_eye { 3.0 } else { 1.0 }).min(150.0) + if perks { 10.0 } else { 0.0 };
            let light = level * if i.exterior { 0.5 } else { 0.33 } + 15.0;
            visual += (1.0 + lm) * seen * light * (100.0 - i.detector_blindness) / 100.0 * size
                + seen * d * 0.5
                - ((150.0 - level) / 150.0).max(0.0) * a * 0.5;
            if visual < 25.0 && i.distance > 0.0 && i.distance < 4.0 * 70.0 && angle < 45.0 * rad {
                visual += ((i.distance - 280.0) / -140.0).clamp(0.0, 1.0) * (25.0 - visual);
            }
        }
        let round = |v: f32| if v.ceil() == 0.0 { -1.0 } else { v.ceil() };
        (round(sound), round(visual))
    }

    fn cases() -> Vec<DetectionInputs> {
        let rad = std::f32::consts::PI / 180.0;
        let base = DetectionInputs { distance: 600.0, light_level: 60.0, ..DetectionInputs::default() };
        vec![
            base,
            DetectionInputs { distance: 120.0, light_level: 5.0, target_sneaking: true, target_agility: 8.0, ..base },
            DetectionInputs { distance: 200.0, view_angle: 30.0 * rad, target_sneaking: true, target_agility: 10.0, light_level: 0.0, ..base },
            DetectionInputs { distance: 2500.0, exterior: true, target_moving: true, target_running: true, target_weight: 40.0, ..base },
            DetectionInputs { distance: 2500.0, exterior: true, target_moving: true, target_running: true, target_sprinting: true, ..base },
            DetectionInputs { distance: 900.0, target_moving: true, target_running: true, target_sneaking: true, target_agility: 4.0, ..base },
            DetectionInputs { view_angle: 120.0 * rad, distance: 6000.0, ..base },
            DetectionInputs { view_angle: 120.0 * rad, distance: 6000.0, detector_fighting_target: true, ..base },
            DetectionInputs { view_pitch: 35.0 * rad, ..base },
            DetectionInputs { view_pitch: -50.0 * rad, target_size_class: 3, ..base },
            DetectionInputs { line_of_sight: false, sees: false, action: 50.0, detector_perception: 9.0, ..base },
            DetectionInputs { target_invisibility: 100.0, target_moving: true, ..base },
            DetectionInputs { target_invisibility: 40.0, detector_night_eye: true, light_level: 70.0, ..base },
            DetectionInputs { detector_sleeping: true, detector_deafness: 50.0, ..base },
            DetectionInputs { detector_not_combat_target: true, detector_blindness: 30.0, target_sneaking: true, target_sneak_skill: 15.0, ..base },
            DetectionInputs { detector_deafness: 100.0, ..base },
            DetectionInputs { ambient_sound_class: 1, action: 100.0, ..base },
            DetectionInputs { ambient_sound_class: 3, action: 100.0, ..base },
            DetectionInputs { ambient_sound_class: 5, action: 100.0, target_moving: true, ..base },
        ]
    }

    /// PRD §7.2 scenario 2 (Starfield): the settings-driven row over `Starfield.exe`'s defaults matches the decoded
    /// executable formula, with and without perks.
    #[test]
    fn the_starfield_row_matches_the_decoded_formula() {
        let rules = rules();
        let with_perks = |hook, v: f32| match hook {
            SneakPerkHook::DetectionLight => v + 10.0,
            SneakPerkHook::DetectionMovement => v * 2.0,
            SneakPerkHook::DetectionSneakSkill => v + 20.0,
        };
        for inputs in cases() {
            for perks in [false, true] {
                let terms = if perks { rules.value_with(&inputs, &with_perks) } else { rules.value(&inputs) }.unwrap();
                let (sound, visual) = exe(&inputs, perks);
                assert_eq!((terms.sound, terms.visual), (sound, visual), "perks {perks}: {inputs:?}");
                assert_eq!(terms.value, sound.max(visual));
            }
        }
    }

    #[test]
    fn the_distances_are_lifted_from_metres_and_the_readings_keep_the_metres() {
        let (rules, readings) = StealthRules::resolve(StealthKind::Starfield, |_| None);
        assert_eq!(rules.settings.max_distance, 64.0 * 70.0);
        assert_eq!(rules.settings.visual_min_distance_max, 4.0 * 70.0);
        assert_eq!(rules.settings.stealth_point_sound_threshold_radius, 8.0 * 70.0);
        let max = readings.iter().find(|r| r.name == "fSneakMaxDistance").unwrap();
        assert_eq!((max.value, max.authored), (64.0, false));
        // An authored value is lifted the same way, and Fallout 4's distances are left alone.
        assert_eq!(StealthRules::resolve(StealthKind::Starfield, |n| (n == "fSneakMaxDistance").then_some(50.0)).0.settings.max_distance, 3500.0);
        assert_eq!(StealthRules::resolve(StealthKind::Fallout4, |_| None).0.settings.max_distance, 4096.0);
    }

    #[test]
    fn ambient_noise_masks_quiet_sound_and_can_push_it_below_the_base_value() {
        let rules = rules();
        let heard = DetectionInputs { distance: 300.0, action: 100.0, ..DetectionInputs::default() };
        let sound = |class: u8| rules.value(&DetectionInputs { ambient_sound_class: class, ..heard }).unwrap().sound;
        assert!(sound(0) > sound(1) && sound(1) > sound(3) && sound(3) > sound(5), "{} {} {} {}", sound(0), sound(1), sound(3), sound(5));
        // A class the table does not list (6) is read as none.
        assert_eq!(sound(6), sound(0));
        // Fully masked: class 5 subtracts 200 from a rating of 100 and goes below fSneakSoundBaseValue (−20).
        assert!(sound(5) < -20.0, "{}", sound(5));
    }

    /// `Starfield.esm` authors `fDetectionRunningMult` 1.5 and `fSneakRunningMult` 1.25 (the census): a run is
    /// 1.25 while sneaking and 1.5 otherwise, a sprint its own 2 (`0x140d7e478..0x140d7e4bc`).
    #[test]
    fn the_run_multiplier_depends_on_the_stance_and_a_sprint_has_its_own() {
        let authored = |name: &str| match name {
            "fDetectionRunningMult" => Some(1.5),
            "fSneakRunningMult" => Some(1.25),
            _ => None,
        };
        let rules = StealthRules::resolve(StealthKind::Starfield, authored).0;
        let step = DetectionInputs { distance: 300.0, target_moving: true, target_weight: 0.0, light_level: 0.0, ..DetectionInputs::default() };
        // A walking step at distance 300: sound = (att · (move)) with move = 12 · speed; distinct speeds, distinct levels.
        let sound = |i: DetectionInputs| rules.value(&i).unwrap().sound;
        let walk = sound(step);
        let run = sound(DetectionInputs { target_running: true, ..step });
        let sneak_run = sound(DetectionInputs { target_running: true, target_sneaking: true, ..step });
        let sprint = sound(DetectionInputs { target_running: true, target_sprinting: true, ..step });
        assert!(walk < sneak_run && sneak_run < run && run < sprint, "{walk} {sneak_run} {run} {sprint}");
    }

    #[test]
    fn a_sprint_is_a_louder_step_than_a_walk_and_a_stealth_boy_ends_the_visual_term() {
        let rules = rules();
        let step = DetectionInputs { distance: 500.0, target_moving: true, target_weight: 20.0, ..DetectionInputs::default() };
        let sound = |i: DetectionInputs| rules.value(&i).unwrap().sound;
        assert!(sound(DetectionInputs { target_running: true, target_sprinting: true, ..step }) > sound(step));
        // Entry 15 silences the run and the sprint.
        let silent = DetectionInputs { target_running: true, target_sprinting: true, target_silent_movement: true, ..step };
        assert_eq!(sound(silent), sound(step));
        let boy = DetectionInputs { target_invisibility: 100.0, ..step };
        assert_eq!(rules.value(&boy).unwrap().visual, -20.0, "the whole visual term is skipped");
    }
}
