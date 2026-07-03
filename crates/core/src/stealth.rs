//! Stealth detection — the FO3/NV sneak-detection formula.
//!
//! A **pure, standalone transcription** of the shared FO3/New Vegas GECK
//! engine's detection algorithm (source: fandom *Sneak (Fallout: New
//! Vegas)*, 2026-07-03; see `docs/engine/charal-fnv-fo3-ruleset.md`). It
//! answers one question: given a detector and a target actor's current
//! state, what is the detector's `Detection` score, and does that mean
//! undetected / suspicious / detected?
//!
//! ## Why this lives outside CHARAL
//!
//! The formula consumes CHARAL-produced values (Sneak skill, Perception,
//! `CharacterLevel`) but also ~10 inputs CHARAL doesn't own — distance,
//! indoor/outdoor, per-weapon noise, light level, movement state, AI alert
//! state, the *other* actor's level. That's a perception/AI-subsystem
//! concern, not character progression (`charal.md` §7 draws the same line
//! around combat/dialogue). This module is the **math only** — no ECS
//! component, no system, no consumer wired yet.
//!
//! ## Status: greenfield, math-only, by design
//!
//! Nothing in the engine feeds this yet: there's no AI-package evaluator, no
//! line-of-sight/vision system, no alert-state component, no sneak/crouch
//! flag (see the survey behind this module — ROADMAP.md's M42 "AI packages"
//! milestone, which this formula will eventually plug into, is Tier 7 and
//! blocked on `PACK` record parsing, #446). Building the detection math now,
//! decoupled from that unbuilt behavior layer, mirrors how the CHARAL
//! affliction mechanism ([`crate::character::affliction`]) was built ahead
//! of its threshold data: the reusable, testable piece lands now; the ECS
//! wiring (a `Sneaking` marker, an `AlertState` component, a tick system
//! iterating detector/target pairs) waits until M42 gives it something to
//! drive.
//!
//! ## No-guessing caveat
//!
//! Unlike most CHARAL formulas, the source page gives **no worked numeric
//! example** for the full `Detection` formula (only for its sub-terms, e.g.
//! Action Points elsewhere). [`detection_score`] is a direct algebraic
//! transcription of the cited formula, not a guess — but it is verified here
//! by structural/monotonicity tests (distance, sound, armor, etc. each move
//! the score the direction the source's prose says), not a "matches wiki
//! example" test like the rest of CHARAL.

/// Whether the detection roll happens indoors or outdoors — sets the maximum
/// detection distance (2500 / 5000 game units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    Indoor,
    Outdoor,
}

impl Locale {
    #[inline]
    fn max_distance(self) -> f32 {
        match self {
            Locale::Indoor => 2500.0,
            Locale::Outdoor => 5000.0,
        }
    }
}

/// The target's current movement state — drives both the sound and visual
/// terms. `SilentRunning` is the perk-driven exception that zeroes movement
/// sound entirely (not just reduces it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementState {
    Stationary,
    Walking,
    Running,
    SilentRunning,
}

/// The loudness class of whatever action the target just performed (firing
/// a weapon, swinging melee, throwing a grenade, …), or none this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSound {
    None,
    Silent,
    Normal,
    Loud,
}

impl ActionSound {
    #[inline]
    fn value(self) -> f32 {
        match self {
            ActionSound::None => 0.0,
            ActionSound::Silent => 10.0,
            ActionSound::Normal => 50.0,
            ActionSound::Loud => 100.0,
        }
    }
}

/// The target's worn-armor noise class (light armor is silent; medium/heavy
/// add a flat penalty to `TargetSkill`, making the target easier to detect).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmorClass {
    Light,
    Medium,
    Heavy,
}

impl ArmorClass {
    #[inline]
    fn penalty(self) -> f32 {
        match self {
            ArmorClass::Light => 0.0,
            ArmorClass::Medium => 10.0,
            ArmorClass::Heavy => 20.0,
        }
    }
}

/// The detector's current AI state — scales `DetectorSkill`. Sleeping actors
/// and actors already fighting their current target are *less* alert
/// (0.8×); actors on edge (alert, lost, or fighting someone else) are *more*
/// alert (1.2×); anything else is neutral (1×).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectorState {
    SleepingOrFightingThisTarget,
    AlertLostOrFightingOther,
    Normal,
}

impl DetectorState {
    #[inline]
    fn multiplier(self) -> f32 {
        match self {
            DetectorState::SleepingOrFightingThisTarget => 0.8,
            DetectorState::AlertLostOrFightingOther => 1.2,
            DetectorState::Normal => 1.0,
        }
    }
}

/// Everything one detection roll needs — one detector, one target, and the
/// environment between them. No field is a CHARAL type directly (the caller
/// resolves `ActorValues`/`CharacterLevel`/`Transform` into these plain
/// values); this module stays independent of the ECS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectionInputs {
    /// World-unit distance between detector and target (~64 units/meter).
    pub distance: f32,
    pub locale: Locale,
    /// Whether the detector has an unobstructed line of sight to the target.
    pub detector_has_los: bool,

    // Detector-side.
    /// Detector's Perception SPECIAL value.
    pub detector_perception: f32,
    pub detector_state: DetectorState,
    pub detector_level: u16,
    pub detector_has_night_eye: bool,

    // Target-side.
    /// `false` short-circuits `TargetSkill` to `0` (not sneaking at all).
    pub target_is_sneaking: bool,
    /// `true` (Chameleon / invisibility effects) zeroes the `Visual` term
    /// regardless of light or line of sight.
    pub target_is_invisible: bool,
    pub target_sneak_skill: f32,
    pub target_level: u16,
    pub target_armor: ArmorClass,
    pub target_equipped_weight: f32,
    pub target_movement: MovementState,
    pub target_action_sound: ActionSound,

    /// Ambient light level at the target's position (source units, ~0–100
    /// before the night-eye multiplier).
    pub light_level: f32,
}

/// The three detection bands the source page defines. `Detection < −20` is
/// undetected; `−20..=0` moves an AI from Normal to Alert (or Combat to
/// Lost); `> 0` is a hard detect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionState {
    Undetected,
    Suspicious,
    Detected,
}

/// Classify a raw [`detection_score`] into the three bands.
#[inline]
#[must_use]
pub fn classify(score: f32) -> DetectionState {
    if score < -20.0 {
        DetectionState::Undetected
    } else if score <= 0.0 {
        DetectionState::Suspicious
    } else {
        DetectionState::Detected
    }
}

/// The FO3/NV `Detection` formula, transcribed exactly from the source (see
/// module docs). Higher is easier to detect; see [`classify`] for the bands.
#[must_use]
pub fn detection_score(inputs: &DetectionInputs) -> f32 {
    let max_distance = inputs.locale.max_distance();
    let attenuation = ((max_distance - inputs.distance) / max_distance).powi(2);

    let sound_multiplier = if inputs.detector_has_los { 1.6 } else { 0.16 };
    let movement_multiplier = match inputs.target_movement {
        MovementState::Stationary | MovementState::SilentRunning => 0.0,
        MovementState::Running => 1.5,
        MovementState::Walking => 1.0,
    };
    let movement_sound = (12.0 + inputs.target_equipped_weight / 2.0) * movement_multiplier;
    let sound = sound_multiplier * (movement_sound + 2.0 * inputs.target_action_sound.value());

    let visual = if !inputs.detector_has_los || inputs.target_is_invisible {
        0.0
    } else {
        let night_eye = if inputs.detector_has_night_eye {
            3.0
        } else {
            1.0
        };
        let light = 1.4 * (inputs.light_level * night_eye).min(100.0);
        let visual_movement = match inputs.target_movement {
            MovementState::Stationary => 0.0,
            MovementState::Running => 0.21,
            MovementState::Walking | MovementState::SilentRunning => 0.01,
        };
        light * (1.0 + visual_movement)
    };

    let detector_skill =
        (10.0 + 8.0 * inputs.detector_perception) * inputs.detector_state.multiplier();

    let target_skill = if !inputs.target_is_sneaking {
        0.0
    } else {
        inputs.target_sneak_skill
            + 5.0 * (f32::from(inputs.target_level) - f32::from(inputs.detector_level))
            + (50.0 - 10.0 * f32::from(inputs.target_level)).max(0.0)
            - inputs.target_armor.penalty()
    };

    attenuation * (sound + visual + detector_skill / 2.0) - target_skill / 2.0 - 35.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> DetectionInputs {
        DetectionInputs {
            distance: 500.0,
            locale: Locale::Outdoor,
            detector_has_los: true,
            detector_perception: 5.0,
            detector_state: DetectorState::Normal,
            detector_level: 5,
            detector_has_night_eye: false,
            target_is_sneaking: true,
            target_is_invisible: false,
            target_sneak_skill: 50.0,
            target_level: 5,
            target_armor: ArmorClass::Light,
            target_equipped_weight: 20.0,
            target_movement: MovementState::Stationary,
            target_action_sound: ActionSound::None,
            light_level: 20.0,
        }
    }

    #[test]
    fn closer_distance_raises_detection() {
        let far = detection_score(&DetectionInputs {
            distance: 4000.0,
            ..baseline()
        });
        let near = detection_score(&DetectionInputs {
            distance: 100.0,
            ..baseline()
        });
        assert!(
            near > far,
            "standing closer to the detector must be easier to detect"
        );
    }

    #[test]
    fn indoor_max_distance_is_half_outdoor() {
        assert_eq!(Locale::Indoor.max_distance(), 2500.0);
        assert_eq!(Locale::Outdoor.max_distance(), 5000.0);
    }

    #[test]
    fn running_raises_detection_over_stationary() {
        let stationary = detection_score(&baseline());
        let running = detection_score(&DetectionInputs {
            target_movement: MovementState::Running,
            ..baseline()
        });
        assert!(running > stationary, "running is louder and more visible");
    }

    #[test]
    fn silent_running_matches_stationary_sound_but_not_visual() {
        // Silent Running zeroes movement *sound* like standing still, but
        // still counts as "moving" for the visual term (small +0.01 vs 0).
        let stationary = detection_score(&baseline());
        let silent_running = detection_score(&DetectionInputs {
            target_movement: MovementState::SilentRunning,
            ..baseline()
        });
        assert!(
            silent_running > stationary,
            "Silent Running still nudges the visual term up slightly"
        );
        // But dramatically less than actually running loudly.
        let running = detection_score(&DetectionInputs {
            target_movement: MovementState::Running,
            ..baseline()
        });
        assert!(silent_running < running);
    }

    #[test]
    fn loud_action_raises_detection() {
        let quiet = detection_score(&baseline());
        let loud = detection_score(&DetectionInputs {
            target_action_sound: ActionSound::Loud,
            ..baseline()
        });
        assert!(loud > quiet, "firing a loud weapon must be easier to hear");
    }

    #[test]
    fn no_line_of_sight_zeroes_visual_and_softens_sound() {
        let with_los = detection_score(&baseline());
        let without_los = detection_score(&DetectionInputs {
            detector_has_los: false,
            ..baseline()
        });
        assert!(
            without_los < with_los,
            "losing line of sight can only help the target hide"
        );
    }

    #[test]
    fn invisibility_zeroes_visual_even_with_los() {
        let visible = detection_score(&baseline());
        let invisible = detection_score(&DetectionInputs {
            target_is_invisible: true,
            ..baseline()
        });
        assert!(invisible < visible, "Chameleon/invisibility only helps");
    }

    #[test]
    fn heavier_armor_raises_detection() {
        let light = detection_score(&baseline());
        let heavy = detection_score(&DetectionInputs {
            target_armor: ArmorClass::Heavy,
            ..baseline()
        });
        assert!(heavy > light, "heavy armor is noisier, easier to detect");
    }

    #[test]
    fn higher_sneak_skill_lowers_detection() {
        let low_skill = detection_score(&DetectionInputs {
            target_sneak_skill: 10.0,
            ..baseline()
        });
        let high_skill = detection_score(&DetectionInputs {
            target_sneak_skill: 90.0,
            ..baseline()
        });
        assert!(high_skill < low_skill, "higher Sneak skill hides better");
    }

    #[test]
    fn not_sneaking_ignores_sneak_skill_entirely() {
        // TargetSkill collapses to 0 regardless of the skill value when the
        // target isn't in sneak mode at all.
        let a = detection_score(&DetectionInputs {
            target_is_sneaking: false,
            target_sneak_skill: 10.0,
            ..baseline()
        });
        let b = detection_score(&DetectionInputs {
            target_is_sneaking: false,
            target_sneak_skill: 90.0,
            ..baseline()
        });
        assert_eq!(a, b, "Sneak skill is irrelevant while not sneaking");
    }

    #[test]
    fn target_level_advantage_over_detector_lowers_detection() {
        // TargetSkill's `5·(TargetLevel − DetectorLevel)` term: a target
        // higher-level than its detector is harder to spot.
        let even = detection_score(&baseline());
        let higher_target_level = detection_score(&DetectionInputs {
            target_level: 15,
            ..baseline()
        });
        assert!(
            higher_target_level < even,
            "outleveling the detector should make the target harder to detect"
        );
    }

    #[test]
    fn higher_detector_perception_raises_detection() {
        let low_per = detection_score(&DetectionInputs {
            detector_perception: 1.0,
            ..baseline()
        });
        let high_per = detection_score(&DetectionInputs {
            detector_perception: 10.0,
            ..baseline()
        });
        assert!(high_per > low_per, "sharper-eyed detectors see better");
    }

    #[test]
    fn alert_detector_state_raises_detection_over_normal() {
        let normal = detection_score(&baseline());
        let alert = detection_score(&DetectionInputs {
            detector_state: DetectorState::AlertLostOrFightingOther,
            ..baseline()
        });
        let sleeping = detection_score(&DetectionInputs {
            detector_state: DetectorState::SleepingOrFightingThisTarget,
            ..baseline()
        });
        assert!(alert > normal, "an alert detector is more perceptive");
        assert!(sleeping < normal, "a sleeping detector is less perceptive");
    }

    #[test]
    fn night_eye_raises_detection_in_the_dark() {
        let without = detection_score(&baseline());
        let with_night_eye = detection_score(&DetectionInputs {
            detector_has_night_eye: true,
            ..baseline()
        });
        assert!(
            with_night_eye > without,
            "NightEye triples the effective light level"
        );
    }

    #[test]
    fn classify_matches_the_documented_bands() {
        assert_eq!(classify(-25.0), DetectionState::Undetected);
        assert_eq!(classify(-20.0), DetectionState::Suspicious, "boundary is inclusive");
        assert_eq!(classify(0.0), DetectionState::Suspicious, "boundary is inclusive");
        assert_eq!(classify(0.1), DetectionState::Detected);
    }

    #[test]
    fn light_level_is_capped_before_the_1_4_multiplier() {
        // `min(100, light*nighteye)` caps the pre-multiplier term, not the
        // final Light value — Light itself can exceed 100 (1.4 * 100 = 140).
        let capped = detection_score(&DetectionInputs {
            light_level: 200.0,
            detector_has_night_eye: true, // light*nighteye = 600, capped to 100
            ..baseline()
        });
        let at_cap = detection_score(&DetectionInputs {
            light_level: 100.0,
            detector_has_night_eye: false,
            ..baseline()
        });
        assert!(
            (capped - at_cap).abs() < 1e-4,
            "both saturate the same min(100, ...) term"
        );
    }
}
