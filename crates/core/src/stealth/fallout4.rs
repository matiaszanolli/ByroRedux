//! Fallout 4's detection (SNK-DET-007, -015, SNK-ATK-004, SNK-PERK-004; design
//! §4.2, §4.3, §8), read from `Fallout4.exe` (#6996). Its own model, not
//! Skyrim's: a sound value and a visual value, each rounded up to an integer,
//! with stealth points split by sense.
//!
//! **The pair** (`0x140cc00c0`, which fills the formula's inputs):
//!
//! ```text
//! range   = fSneakMaxDistance · [exterior: fSneakExteriorDistanceMult]
//!             · [size class above Normal: fSneakLargeActorDistanceMult · size]
//! d       = √(horizontal² + (fDetectionDistanceHeightMult · vertical)²)
//! LOS     = tested only while d ≤ range · lerp(angle, fDetectionLOSDistanceAngle → 180°, 1 → LOSDistanceMult),
//!             or always toward the detector's combat target
//! sees    = LOS ∧ angle < fDetectionViewCone / 2
//! visible = [Invisibility < 100: (100 − Invisibility) / 100]
//!             · [occluders n > 0: max(fDetectionObjectVisibilityMult − (n − 1)·Offset, Min)]
//!             · [in cover: fSneakVisualCoverMod]
//! visualR = range · lerp(angle, fDetectionVisualDistanceAngle → 90°, 1 → VisualDistanceMult)
//!             · lerp(pitch, ±AngleUp/DownMin → ±AngleUp/DownMax, 1 → Up/DownMult) · visible
//! light   = min(level · [night eye: fDetectionNightEyeBonus], iLightLevelMax), then entry 47
//!             level = 100·luminance − (exterior: iLightLevelExteriorMod; interior: min(100·ambient, InteriorMod))
//! ```
//!
//! **The value** (`0x1406c8770`):
//!
//! ```text
//! P      = fSneakPerceptionSkillMin + (Max − Min) · Perception · 0.1
//! D      = P · (1 + inCombat·fSneakAlertMod + asleep·fSneakSleepMod + (inCombat ∧ ¬combatTarget)·fSneakCombatMod)
//! A      = sneaking ? fSneakAgilitySkillMin + entry 57(Sneak) + (AgilityMax − Min) · Agility · 0.1 : 0
//! sound  = fSneakSoundBaseValue                                      (Deafness ≥ 100: nothing more)
//!          + fSneakSoundsMult · att(range)·los · (max(0, (att(range)·los·D − A)·fSneakSkillMovementMult + move)
//!            + action·fSneakActionMult) · (1 + asleep·fSneakSleepBonus) · (100 − Deafness) / 100
//!            los  = LOS ? 1 : fSneakSoundLosMult
//!            move = moving · (fSneakEquippedWeightBase + Mult·weight) · max(0, MovementNoiseMult) · [running: fSneakRunningMult]
//! visual = fSneakVisualBaseValue                                     (¬sees or asleep: nothing more)
//!          + (1 + entry 48(moving·fSneakLightMoveMult + running·fSneakLightRunMult)) · att(visualR)
//!            · (light · (exterior ? fSneakLightExteriorMult : fSneakLightMult) + fDetectionSneakLightMod)
//!            · (100 − Blindness) / 100 · [Invisibility ≥ 100: fSneakStealthBoyMult] · fSneakLargeActorDistanceMult · size
//!          + att(visualR) · D · fSneakSkillMult − max(0, (iLightLevelMax − light) / iLightLevelMax) · A · fSneakSkillMult
//!          close range: within fSneakVisualMinDistanceMax and fSneakVisualMinAngle, raised toward
//!          fSneakVisualMinValue by clamp((d − Max) / (Min − Max), 0, 1)
//! level  = ⌈value⌉, 0 counting as −1 (`0x140c98900`); the pair's value is the higher of the two
//! ```
//!
//! **Levels and stealth points** (`0x140e94420`, `0x140e9f170`). A non-hostile
//! detector sees the player at a value above 0. A hostile one is held off while
//! the player's stealth points last, unless its visual level passes
//! `iCombatStealthPoint[Sneak]DetectionVisualThreshold` or, within
//! `fCombatStealthPointDetectionSoundThresholdRadius`, its sound level passes the
//! sound threshold: that empties the points. They drain by max(sound ·
//! `…DrainSoundMult`, visual · `…DrainVisualMult`) per second, but never below
//! `fCombatStealthPointSoundThreshold` from sound alone unless the sound reaches
//! `…LOSSoundValueThreshold` with line of sight. They refill at max((1 − the
//! nearest hostile's attenuation) · the Lost / Hostile / Alert mult,
//! `…RegenMin`) per second, eased in over the longest outstanding of the
//! alert-or-lost, attacked and detected-event delays.
//!
//! The original keeps stealth points per combat group; the engine keeps one pool
//! for the player, as for Skyrim. It also has no ambush packages, combat-group
//! "observed" target, flight, cover rays or occluder count yet: those inputs stay
//! at their neutral values (design §4.2).

use super::{DetectionInputs, DetectionLevel, DetectionSettings, DetectionTerms, SneakPerkHook, SneakWeapon, StealthPointInputs, StealthPoints};

/// `lerp` as `Fallout4.exe` `0x14101b560` computes it: `from` at `x0`, `to` at `x1`, clamped between.
pub(super) fn lerp(x: f32, x0: f32, x1: f32, from: f32, to: f32) -> f32 {
    let t = if x1 != x0 { ((x - x0) / (x1 - x0)).clamp(0.0, 1.0) } else { 1.0 };
    from + t * (to - from)
}

pub(super) fn size_mult(s: &DetectionSettings, class: u8) -> f32 {
    match class {
        0 => s.size_small,
        1 => s.size_normal,
        2 => s.size_large,
        3 => s.size_very_large,
        _ => 1.0,
    }
}

/// The detection range toward a target of `size_class` (`0x1406c82a0`).
pub(super) fn range(s: &DetectionSettings, exterior: bool, size_class: u8) -> f32 {
    let mut range = s.max_distance * if exterior { s.exterior_distance_mult } else { 1.0 };
    let size = size_mult(s, size_class);
    if size > 1.0 {
        range *= s.large_actor_distance_mult * size;
    }
    range
}

/// The pair distance, the height difference weighted by `fDetectionDistanceHeightMult` (`0x140cc0379`).
pub(super) fn pair_distance(s: &DetectionSettings, horizontal: f32, vertical: f32) -> f32 {
    horizontal.hypot(s.distance_height_mult * vertical)
}

/// `⌈value⌉`, with 0 counting as −1 (`0x140c98979`).
pub(super) fn level_of(value: f32) -> f32 {
    let ceiling = value.ceil();
    if ceiling == 0.0 { -1.0 } else { ceiling }
}

pub(super) fn value(s: &DetectionSettings, i: &DetectionInputs, perks: &dyn Fn(SneakPerkHook, f32) -> f32) -> DetectionTerms {
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    let deg = std::f32::consts::PI / 180.0;
    let range = range(s, i.exterior, i.target_size_class);
    let d = i.distance;
    let angle = i.view_angle.abs();

    // Line of sight is tested only inside the off-axis LOS distance (`0x140cc0635`).
    let in_combat = i.detector_not_combat_target || i.detector_fighting_target;
    let los_reach = if !i.detector_fighting_target && angle > s.los_distance_angle * deg {
        let mult = if i.exterior { s.los_distance_mult_exterior } else { s.los_distance_mult_interior };
        lerp(angle, s.los_distance_angle * deg, std::f32::consts::PI, 1.0, mult)
    } else {
        1.0
    };
    let line_of_sight = i.line_of_sight && d <= range * los_reach;
    let sees = i.sees && line_of_sight;

    // The visual range: off-axis and vertical falloffs, then visibility (`0x140cc0f6c`, `0x140cc0fd2`).
    let mut visual_range = range;
    if angle > s.visual_distance_angle * deg {
        let mult = if i.exterior { s.visual_distance_mult_exterior } else { s.visual_distance_mult_interior };
        visual_range *= lerp(angle, s.visual_distance_angle * deg, std::f32::consts::FRAC_PI_2, 1.0, mult);
    }
    let pitch = i.view_pitch;
    if pitch > s.visual_distance_angle_up_min * deg {
        visual_range *= lerp(pitch, s.visual_distance_angle_up_min * deg, s.visual_distance_angle_up_max * deg, 1.0, s.visual_distance_up_mult);
    } else if pitch < -s.visual_distance_angle_down_min * deg {
        visual_range *=
            lerp(pitch, -s.visual_distance_angle_down_min * deg, -s.visual_distance_angle_down_max * deg, 1.0, s.visual_distance_down_mult);
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

    // Entry 15 (Ignore Running During Detection) silences running, and movement while sneaking (`0x140cc08cf`).
    let running = i.target_running && !i.target_silent_movement;
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
        let movement = flag(moving)
            * (s.equipped_weight_base + s.equipped_weight_mult * i.target_weight)
            * i.target_noise_mult.max(0.0)
            * if running { s.running_mult } else { 1.0 };
        let skill = ((heard * detector - agility) * s.skill_movement_mult + movement).max(0.0);
        sound += s.sounds_mult * heard * (skill + i.action * s.action_mult) * (1.0 + flag(i.detector_sleeping) * s.sleep_bonus)
            * (100.0 - i.detector_deafness)
            * 0.01;
    }

    let mut visual = s.visual_base_value;
    if sees && !i.detector_sleeping {
        let seen = attenuation(visual_range);
        let motion = perks(SneakPerkHook::DetectionMovement, flag(moving) * s.light_move_mult + flag(running) * s.light_run_mult);
        let ambient_mod = if i.exterior { s.light_level_exterior_mod } else { i.light_ambient.min(s.light_level_interior_mod) };
        let level = (i.light_level - ambient_mod).max(0.0) * if i.detector_night_eye { s.night_eye_bonus } else { 1.0 };
        let level = perks(SneakPerkHook::DetectionLight, level.min(s.light_level_max));
        let light = level * if i.exterior { s.light_exterior_mult } else { s.light_mult } + s.light_mod;
        let boy = if stealth_boy { s.stealth_boy_mult } else { 1.0 };
        let size = s.large_actor_distance_mult * size_mult(s, i.target_size_class);
        let darkness = if s.light_level_max > 0.0 { ((s.light_level_max - level) / s.light_level_max).max(0.0) } else { 0.0 };
        visual += (1.0 + motion) * seen * light * (100.0 - i.detector_blindness) * 0.01 * boy * size + seen * detector * s.skill_mult
            - darkness * agility * s.skill_mult;
        if !stealth_boy
            && visual < s.visual_min_value
            && d > 0.0
            && d < s.visual_min_distance_max
            && angle < s.visual_min_angle * deg
        {
            let t = ((d - s.visual_min_distance_max) / (s.visual_min_distance_min - s.visual_min_distance_max)).clamp(0.0, 1.0);
            visual += t * (s.visual_min_value - visual);
        }
    }

    let (sound, visual) = (level_of(sound), level_of(visual));
    DetectionTerms { sound, visual, skill: -agility, value: sound.max(visual) }
}

/// Whether a hostile detector's levels pass the stealth-point thresholds (`0x140e9465d`): it then finds the player
/// whatever points remain, and empties them.
pub(super) fn exceeds_thresholds(s: &DetectionSettings, terms: &DetectionTerms, distance: f32, target_sneaking: bool) -> bool {
    let (visual, sound) = if target_sneaking {
        (s.stealth_point_sneak_visual_threshold, s.stealth_point_sneak_sound_threshold)
    } else {
        (s.stealth_point_visual_threshold, s.stealth_point_sound_threshold)
    };
    terms.visual > visual || (distance <= s.stealth_point_sound_threshold_radius && terms.sound > sound)
}

/// The level toward the player: Lost at a value of 0 or less; Seen for a non-hostile detector, or once the
/// stealth points are gone or the thresholds pass; Noticed while the points hold a hostile one off.
pub(super) fn level(s: &DetectionSettings, terms: &DetectionTerms, distance: f32, hostile: bool, target_sneaking: bool, points: f32) -> DetectionLevel {
    if terms.value <= 0.0 {
        DetectionLevel::Lost
    } else if !hostile || points <= 0.0 || exceeds_thresholds(s, terms, distance, target_sneaking) {
        DetectionLevel::Seen
    } else {
        DetectionLevel::Noticed
    }
}

/// The sneak-attack multiplier by animation type 0-11 (`0x140479660`): Skyrim's first nine classes, then Gun,
/// Grenade and Mine; 1 for anything else. Entry 18 runs over it (`0x140e3a1d9`).
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
        s.sneak_gun,
        s.sneak_grenade,
        s.sneak_mine,
    ];
    weapon.and_then(|w| table.get(usize::from(w.anim_type)).copied()).unwrap_or(1.0)
}

/// "Never": the elapsed-time sentinel `StealthPoints` starts from.
const NEVER: f32 = f32::MAX / 4.0;

/// One frame of the player's stealth points (`0x140e9f170` for the drain and refill, `0x140e94420` for the
/// thresholds and the attack / detection-event caps).
pub(super) fn step_points(s: &DetectionSettings, sp: &mut StealthPoints, i: StealthPointInputs, dt: f32) {
    for elapsed in [&mut sp.since_alert, &mut sp.since_attacked, &mut sp.since_event] {
        *elapsed = (*elapsed + dt).min(f32::MAX / 2.0);
    }
    if i.attacked {
        sp.points = sp.points.min(s.stealth_point_attacked_max);
        sp.since_attacked = 0.0;
        sp.ever_attacked = true;
    }
    if i.detection_event {
        sp.points = sp.points.min(s.stealth_point_event_max);
        sp.since_event = 0.0;
    }
    if i.over_threshold {
        sp.points = 0.0;
    }
    let visual = i.highest_hostile_visual.unwrap_or(-1.0);
    let sound = i.highest_hostile_sound.unwrap_or(-1.0);
    if visual.max(sound) > 0.0 {
        sp.since_alert = 0.0;
        sp.ever_detected = true;
    }
    if visual.max(sound) >= 0.0 && sp.points > 0.0 {
        // Drain; sound alone stops at the floor unless loud with line of sight.
        let floor = if visual > 0.0 || (i.sound_line_of_sight && sound >= s.stealth_point_los_sound_value_threshold) {
            0.0
        } else {
            s.stealth_point_sound_floor
        };
        let drain = (sound * s.stealth_point_drain_sound_mult).max(visual * s.stealth_point_drain_visual_mult) * dt;
        sp.points = (sp.points - drain).max(sp.points.min(floor));
    } else if sp.points < s.stealth_point_max {
        sp.points = (sp.points + regen_rate(s, sp, i) * dt).min(s.stealth_point_max);
    }
}

/// The refill rate (`0x140e9f3a9` to `0x140e9f5ca`): the base rate eased in over the longest outstanding delay,
/// so the points reach full when the delay plus a full refill at the base rate has passed.
fn regen_rate(s: &DetectionSettings, sp: &StealthPoints, i: StealthPointInputs) -> f32 {
    let lost = sp.ever_detected;
    let mult = if lost {
        s.stealth_point_regen_lost_mult
    } else if sp.ever_attacked {
        s.stealth_point_regen_hostile_mult
    } else {
        s.stealth_point_regen_alert_mult
    };
    let attenuation = i
        .nearest_hostile_distance
        .map_or(0.0, |d| {
            let range = range(s, i.exterior, 1);
            if range > 0.0 { ((range - d) / range).max(0.0).powf(s.attenuation_exponent) } else { 0.0 }
        });
    let rate = ((1.0 - attenuation) * mult).max(s.stealth_point_regen_min);
    let alert_delay = if lost { s.stealth_point_regen_lost_delay } else { s.stealth_point_regen_alert_delay };
    let since = |t: f32| if t >= NEVER { f32::MAX } else { t };
    // The executable's comparisons (`0x140e9f539`): the alert-or-lost delay binds only when strictly the longest
    // outstanding, then the detected-event one when it outlasts the attacked one.
    let alert = alert_delay - since(sp.since_alert);
    let attacked = s.stealth_point_regen_attacked_delay - since(sp.since_attacked);
    let event = s.stealth_point_regen_event_delay - since(sp.since_event);
    let total = if alert > attacked && alert > event {
        alert_delay
    } else if event > attacked {
        s.stealth_point_regen_event_delay
    } else {
        s.stealth_point_regen_attacked_delay
    };
    let elapsed = total - attacked.max(alert).max(event);
    let missing = s.stealth_point_max - sp.points;
    if elapsed >= total + s.stealth_point_max / rate {
        return rate;
    }
    if elapsed <= 0.0 {
        return 0.0;
    }
    let b = (total - elapsed) * rate + missing;
    (((4.0 * elapsed * rate * missing + b * b).sqrt() - b) * 0.5 / elapsed).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::super::{DetectionInputs, LevelContext, StealthKind, StealthRules};
    use super::*;

    /// `Fallout4.esm`'s authored values (read 2026-11-16) over `Fallout4.exe`'s defaults.
    fn rules() -> StealthRules {
        let authored = |name: &str| match name {
            "fSneakMaxDistance" => Some(4096.0),
            "fSneakExteriorDistanceMult" => Some(1.25),
            "fSneakSoundLosMult" => Some(0.4),
            "fSneakLightMult" => Some(0.33),
            "fSneakLightExteriorMult" => Some(0.5),
            "fDetectionSneakLightMod" => Some(15.0),
            "iLightLevelMax" => Some(150.0),
            "iLightLevelInteriorMod" => Some(25.0),
            "fSneakAgilitySkillMax" => Some(50.0),
            "fSneakVisualMinValue" => Some(25.0),
            "fCombatStealthPointSoundThreshold" => Some(25.0),
            "fCombatStealthPointRegenAlertMult" => Some(50.0),
            "fCombatStealthPointRegenHostileMult" => Some(15.0),
            "fCombatStealthPointRegenLostMult" => Some(20.0),
            "fCombatStealthPointRegenMin" => Some(2.0),
            "fCombatSneakHandMult" | "fCombatSneak1HSwordMult" | "fCombatSneak1HDaggerMult" | "fCombatSneak1HAxeMult" => Some(3.0),
            "fCombatSneak1HMaceMult" | "fCombatSneak2HSwordMult" | "fCombatSneak2HAxeMult" => Some(3.0),
            _ => None,
        };
        StealthRules::resolve(StealthKind::Fallout4, authored).0
    }

    /// The decoded formula (`0x1406c8770` with the pair from `0x140cc00c0`) written out with `Fallout4.esm`'s
    /// constants, as the transcription oracle (PRD §7.2 scenario 2). `perks` adds 10 to the light level,
    /// doubles the light-movement term and adds 20 to the target's Sneak (entries 47, 48 and 57).
    fn exe(i: &DetectionInputs, perks: bool) -> (f32, f32) {
        let rad = std::f32::consts::PI / 180.0;
        let size = [0.5, 1.0, 1.5, 2.0][usize::from(i.target_size_class.min(3))];
        let range = 4096.0 * if i.exterior { 1.25 } else { 1.0 } * if size > 1.0 { size } else { 1.0 };
        let angle = i.view_angle.abs();
        let los_reach = if !i.detector_fighting_target && angle > 45.0 * rad {
            1.0 - 0.5 * ((angle - 45.0 * rad) / (180.0 * rad - 45.0 * rad)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let los = i.line_of_sight && i.distance <= range * los_reach;
        let sees = i.sees && los;
        let mut visual_range = range;
        if angle > 30.0 * rad {
            visual_range *= 1.0 - 0.67 * ((angle - 30.0 * rad) / (90.0 * rad - 30.0 * rad)).clamp(0.0, 1.0);
        }
        if i.view_pitch > 15.0 * rad {
            visual_range *= 1.0 - 0.9 * ((i.view_pitch - 15.0 * rad) / (30.0 * rad)).clamp(0.0, 1.0);
        } else if i.view_pitch < -15.0 * rad {
            visual_range *= 1.0 - 0.9 * ((i.view_pitch + 15.0 * rad) / (-45.0 * rad)).clamp(0.0, 1.0);
        }
        let boy = i.target_invisibility >= 100.0;
        visual_range *= if boy { 1.0 } else { (100.0 - i.target_invisibility) / 100.0 };
        let att = |max: f32| ((max - i.distance) / max).max(0.0).powi(2);
        let p = 10.0 + 80.0 * i.detector_perception / 10.0;
        // fSneakAlertMod and fSneakSleepMod are 0 in `Fallout4.esm`: only the not-combat-target −0.4 remains.
        let d = p * (1.0 + if i.detector_not_combat_target { -0.4 } else { 0.0 });
        let a = if i.target_sneaking { i.target_sneak_skill + if perks { 20.0 } else { 0.0 } + 50.0 * i.target_agility / 10.0 } else { 0.0 };
        let mut sound = -20.0;
        if i.detector_deafness < 100.0 {
            let heard = att(range) * if los { 1.0 } else { 0.4 };
            let movement = if i.target_moving { (12.0 + 0.5 * i.target_weight) * if i.target_running { 2.0 } else { 1.0 } } else { 0.0 };
            sound += heard * (((heard * d - a) * 0.25 + movement).max(0.0) + i.action) * (100.0 - i.detector_deafness) / 100.0;
        }
        let mut visual = -20.0;
        if sees && !i.detector_sleeping {
            let seen = att(visual_range);
            let lm = (if i.target_moving { 0.2 } else { 0.0 } + if i.target_running { 0.4 } else { 0.0 }) * if perks { 2.0 } else { 1.0 };
            let ambient = if i.exterior { 0.0 } else { i.light_ambient.min(25.0) };
            let level = ((i.light_level - ambient).max(0.0) * if i.detector_night_eye { 3.0 } else { 1.0 }).min(150.0)
                + if perks { 10.0 } else { 0.0 };
            let light = level * if i.exterior { 0.5 } else { 0.33 } + 15.0;
            visual += (1.0 + lm) * seen * light * (100.0 - i.detector_blindness) / 100.0 * if boy { 0.0 } else { 1.0 } * size
                + seen * d * 0.5
                - ((150.0 - level) / 150.0).max(0.0) * a * 0.5;
            if !boy && visual < 25.0 && i.distance > 0.0 && i.distance < 300.0 && angle < 45.0 * rad {
                visual += ((i.distance - 300.0) / -150.0).clamp(0.0, 1.0) * (25.0 - visual);
            }
        }
        let round = |v: f32| if v.ceil() == 0.0 { -1.0 } else { v.ceil() };
        (round(sound), round(visual))
    }

    fn cases() -> Vec<DetectionInputs> {
        let rad = std::f32::consts::PI / 180.0;
        let base = DetectionInputs { distance: 600.0, light_level: 60.0, light_ambient: 20.0, ..DetectionInputs::default() };
        vec![
            base,
            DetectionInputs { distance: 120.0, light_level: 5.0, light_ambient: 5.0, target_sneaking: true, target_agility: 8.0, ..base },
            DetectionInputs { distance: 200.0, view_angle: 30.0 * rad, target_sneaking: true, target_agility: 10.0, light_level: 0.0, ..base },
            DetectionInputs { distance: 2500.0, exterior: true, target_moving: true, target_running: true, target_weight: 40.0, ..base },
            DetectionInputs { view_angle: 120.0 * rad, distance: 2200.0, ..base },
            DetectionInputs { view_angle: 120.0 * rad, distance: 2200.0, detector_fighting_target: true, ..base },
            DetectionInputs { view_pitch: 35.0 * rad, ..base },
            DetectionInputs { view_pitch: -50.0 * rad, target_size_class: 3, ..base },
            DetectionInputs { line_of_sight: false, sees: false, action: 50.0, detector_perception: 9.0, ..base },
            DetectionInputs { target_invisibility: 100.0, target_moving: true, ..base },
            DetectionInputs { target_invisibility: 40.0, detector_night_eye: true, light_level: 70.0, ..base },
            DetectionInputs { detector_sleeping: true, detector_deafness: 50.0, ..base },
            DetectionInputs { detector_not_combat_target: true, detector_blindness: 30.0, target_sneaking: true, target_sneak_skill: 15.0, ..base },
            DetectionInputs { detector_deafness: 100.0, ..base },
        ]
    }

    /// PRD §7.2 scenario 2 (Fallout 4): the settings-driven row over `Fallout4.esm`'s values matches the
    /// decoded executable formula, with and without perks.
    #[test]
    fn the_fallout4_row_matches_the_decoded_formula() {
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
    fn sneaking_in_the_dark_and_off_axis_hides_and_the_close_range_floor_reveals() {
        let rules = rules();
        let rad = std::f32::consts::PI / 180.0;
        let value = |i: DetectionInputs| rules.value(&i).unwrap();
        let lit = DetectionInputs { distance: 800.0, light_level: 80.0, ..DetectionInputs::default() };
        let hidden = DetectionInputs { light_level: 0.0, target_sneaking: true, target_agility: 9.0, ..lit };
        assert!(value(lit).visual > 0.0, "{:?}", value(lit));
        assert!(value(hidden).visual < value(lit).visual);
        assert!(value(DetectionInputs { view_angle: 80.0 * rad, ..lit }).visual < value(lit).visual, "off-axis shortens the range");
        // Within 150 units and 45°, the visual level reaches fSneakVisualMinValue whatever the light.
        let close = DetectionInputs { distance: 100.0, ..hidden };
        assert_eq!(value(close).visual, 25.0);
        assert!(value(DetectionInputs { view_angle: 60.0 * rad, ..close }).visual < 25.0);
        // A Stealth Boy zeroes the light term and skips the floor; the detector's skill term still counts.
        let boy = value(DetectionInputs { target_invisibility: 100.0, ..close }).visual;
        assert!(boy < 25.0 && boy > -20.0, "{boy}");
        assert_eq!(value(DetectionInputs { detector_deafness: 100.0, ..lit }).sound, -20.0);
    }

    #[test]
    fn a_hostile_detector_is_held_off_until_the_points_run_out_or_a_threshold_passes() {
        let rules = rules();
        let ctx = |hostile, points, visual, sound, distance, sneaking| LevelContext {
            hostile,
            target_sneaking: sneaking,
            stealth_points: points,
            terms: DetectionTerms { visual, sound, skill: 0.0, value: f32::max(visual, sound) },
            distance,
        };
        let level = |c: LevelContext| rules.level(c.terms.value, c);
        assert_eq!(level(ctx(true, 100.0, -1.0, -1.0, 100.0, true)), DetectionLevel::Lost);
        assert_eq!(level(ctx(false, 100.0, 5.0, -1.0, 100.0, true)), DetectionLevel::Seen, "not hostile");
        assert_eq!(level(ctx(true, 100.0, 60.0, 35.0, 100.0, true)), DetectionLevel::Noticed, "under 70 / 40 sneaking");
        assert_eq!(level(ctx(true, 100.0, 71.0, -1.0, 100.0, true)), DetectionLevel::Seen, "visual over 70");
        assert_eq!(level(ctx(true, 100.0, -1.0, 41.0, 400.0, true)), DetectionLevel::Seen, "sound over 40 within 512");
        assert_eq!(level(ctx(true, 100.0, -1.0, 41.0, 600.0, true)), DetectionLevel::Noticed, "too far for the sound threshold");
        assert_eq!(level(ctx(true, 100.0, 26.0, -1.0, 100.0, false)), DetectionLevel::Seen, "standing: visual over 25");
        assert_eq!(level(ctx(true, 0.0, 5.0, -1.0, 100.0, true)), DetectionLevel::Seen, "no points left");
        assert!(rules.noticed(1.0) && !rules.noticed(-1.0));
    }

    #[test]
    fn stealth_points_drain_by_sense_stop_at_the_sound_floor_and_ease_back() {
        let rules = rules();
        let mut sp = StealthPoints::full(100.0);
        let heard = StealthPointInputs { highest_hostile_visual: Some(-1.0), highest_hostile_sound: Some(20.0), ..StealthPointInputs::default() };
        rules.step_stealth_points(&mut sp, heard, 1.0);
        assert!((sp.points - 70.0).abs() < 1e-3, "20 · 1.5 per second: {}", sp.points);
        rules.step_stealth_points(&mut sp, heard, 10.0);
        assert_eq!(sp.points, 25.0, "sound alone stops at fCombatStealthPointSoundThreshold");
        let loud = StealthPointInputs { highest_hostile_sound: Some(50.0), sound_line_of_sight: true, ..heard };
        rules.step_stealth_points(&mut sp, loud, 0.1);
        assert!((sp.points - 17.5).abs() < 1e-3, "loud with line of sight passes the floor: {}", sp.points);
        let seen = StealthPointInputs { highest_hostile_visual: Some(5.0), highest_hostile_sound: Some(-1.0), ..heard };
        rules.step_stealth_points(&mut sp, seen, 1.0);
        assert_eq!(sp.points, 2.5, "visual 5 · 3 per second");
        let hidden = StealthPointInputs { highest_hostile_visual: Some(-1.0), highest_hostile_sound: Some(-1.0), ..StealthPointInputs::default() };
        rules.step_stealth_points(&mut sp, hidden, 0.5);
        let early = sp.points;
        // 0.5 s into the 1 s Lost delay the eased rate is (√(4·0.5·20·97.5 + 107.5²) − 107.5) / 1 ≈ 16.8 per
        // second, short of Lost's full 20.
        assert!((early - (2.5 + 0.5 * 16.82)).abs() < 0.05, "inside the Lost delay the refill eases in: {early}");
        for _ in 0..100 {
            rules.step_stealth_points(&mut sp, hidden, 0.1);
        }
        assert!(sp.points > 90.0, "past the delay, Lost's 20 per second: {}", sp.points);
        rules.step_stealth_points(&mut sp, StealthPointInputs { attacked: true, ..hidden }, 0.0);
        assert!(sp.points <= 65.0);
        rules.step_stealth_points(&mut sp, StealthPointInputs { over_threshold: true, ..hidden }, 0.0);
        assert_eq!(sp.points, 0.0);
    }

    /// SNK-ATK-004 (Fallout 4): animation types 0-11, `Fallout4.esm`'s ×3 melee over the executable's ×2 Gun.
    #[test]
    fn the_sneak_attack_table_adds_gun_grenade_and_mine() {
        let rules = rules();
        let strike = |anim_type| rules.sneak_attack(10.0, Some(SneakWeapon { anim_type, ..Default::default() }), 0.0, &|m| m).unwrap();
        assert_eq!(strike(2), (30.0, true), "dagger ×3");
        assert_eq!(strike(9), (20.0, true), "gun ×2");
        assert_eq!(strike(11), (20.0, true), "mine ×2");
        assert_eq!(strike(8), (10.0, false), "staff ×1: not flagged");
        assert_eq!(strike(40), (10.0, false), "unknown type ×1");
        let doubled = rules.sneak_attack(10.0, Some(SneakWeapon { anim_type: 9, ..Default::default() }), 0.0, &|m| m * 2.0).unwrap();
        assert_eq!(doubled, (40.0, true), "entry 18 over the class multiplier");
    }

    #[test]
    fn the_pair_distance_weights_height_and_the_period_draws_from_the_range() {
        let rules = rules();
        assert_eq!(rules.pair_distance(300.0, 400.0), 300.0f32.hypot(100.0));
        assert_eq!(rules.view_cone_degrees(), Some(190.0));
        assert!((rules.period(0.5) - 1.5).abs() < 1e-6);
        assert_eq!(rules.range(true), 4096.0 * 1.25);
    }
}
