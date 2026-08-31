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
//! One correction to "nothing feeds this yet" (#2979): a melee-combat
//! consumer shipped 2026-08-15/16 (`byroredux/src/combat.rs`), and the
//! canonical `HitEvent` it produces carries a `sneak_attack` field — set to a
//! hardcoded `false`, because no sneak/crouch input, alert state, or
//! detection tick exists to compute it from. That field is this module's
//! concrete future hook point, not evidence of a wired consumer; the
//! zero-caller state above still holds.
//!
//! ## No-guessing caveat
//!
//! Unlike most CHARAL formulas, the source page gives **no worked numeric
//! example** for the full `Detection` formula (only for its sub-terms, e.g.
//! Action Points elsewhere). [`detection_score`] is a direct algebraic
//! transcription of the cited formula, not a guess — so there is no
//! "matches wiki example" test for the whole formula like the rest of
//! CHARAL has. What there is instead, since #3482: the source's `Sound` and
//! `Visual` sub-expressions are now captured *verbatim* in
//! `docs/engine/charal-fnv-fo3-ruleset.md` (they previously survived only as
//! prose there, which made every coefficient below unfalsifiable from the
//! repository), and the `*_matches_the_source_table` /
//! `*_matches_the_captured_*` tests pin each one to its exact captured value
//! on top of the original monotonicity checks.
//!
//! One coefficient is a **disclosed modelling choice, not a transcription**:
//! see [`MovementState::SilentRunning`].

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
///
/// #3482 — this enum collapses two independent things the source keeps
/// apart: how the target is moving, and whether it has the Silent Running
/// perk. The source special-cases the perk in the **sound** branch only
/// ("stationary *or* has Silent Running" ⇒ `MovementMultiplier = 0`) and
/// never mentions it in `VisualMovement`, which discriminates on motion
/// alone (`0` / `0.01` / `0.21` for not moving / moving / running). So this
/// variant has to *choose* a visual coefficient, and it takes `0.01`
/// ("moving but not running"); a target sprinting with the perk is arguably
/// `0.21` under the source's own wording. That is the one number here the
/// capture document owns rather than the source — recorded in
/// `docs/engine/charal-fnv-fo3-ruleset.md` so it is visible instead of
/// looking sourced. Splitting the perk out of this enum is an M42-era
/// change (nothing consumes it today).
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
        assert_eq!(
            classify(-20.0),
            DetectionState::Suspicious,
            "boundary is inclusive"
        );
        assert_eq!(
            classify(0.0),
            DetectionState::Suspicious,
            "boundary is inclusive"
        );
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

    // ── #3482 (CHAR-2026-08-27b-D2-01) — exact per-coefficient pins.
    //
    //   The monotonicity tests above check that each input moves the score
    //   the direction the source's prose says. They cannot tell 1.6 from
    //   2.6, 0.21 from 0.12, or 12 + w/2 from 12 + w — which is what made
    //   the whole `Sound`/`Visual` coefficient set unfalsifiable while the
    //   capture document recorded only prose for it. Each test below asserts
    //   an exact value against the table now captured in
    //   `docs/engine/charal-fnv-fo3-ruleset.md` § Sneak Detection (FNV).

    /// Inputs that zero **every** term of `Detection` except the one under
    /// test, so a probe's score minus [`ISOLATED_BASE`] is that term alone:
    ///
    /// * `distance = 0` ⇒ `Attenuation = ((5000−0)/5000)² = 1`
    /// * stationary, no action, zero equipped weight ⇒ `Sound = 0`
    /// * `light_level = 0` ⇒ `Light = 0` ⇒ `Visual = 0`
    /// * `detector_perception = 0`, `Normal` state ⇒ `DetectorSkill = 10`
    /// * not sneaking ⇒ `TargetSkill = 0`
    ///
    /// leaving `1·(0 + 0 + 10/2) − 0/2 − 35 = −30`.
    fn isolated() -> DetectionInputs {
        DetectionInputs {
            distance: 0.0,
            locale: Locale::Outdoor,
            detector_has_los: true,
            detector_perception: 0.0,
            detector_state: DetectorState::Normal,
            detector_level: 1,
            detector_has_night_eye: false,
            target_is_sneaking: false,
            target_is_invisible: false,
            target_sneak_skill: 0.0,
            target_level: 1,
            target_armor: ArmorClass::Light,
            target_equipped_weight: 0.0,
            target_movement: MovementState::Stationary,
            target_action_sound: ActionSound::None,
            light_level: 0.0,
        }
    }

    const ISOLATED_BASE: f32 = -30.0;

    /// `Detection`'s own constants, and the premise every probe below rests
    /// on. `−35` is the flat term; `10 + 8·Perception` is `DetectorSkill`'s
    /// base; both are halved/subtracted exactly as the source writes them.
    #[test]
    fn isolated_baseline_is_the_documented_constant_term() {
        approx(detection_score(&isolated()), ISOLATED_BASE, "1·(10/2) − 35");
    }

    fn approx(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what}: expected {expected}, got {actual}"
        );
    }

    /// The term the probe under test contributes, isolated.
    fn term(inputs: &DetectionInputs) -> f32 {
        detection_score(inputs) - ISOLATED_BASE
    }

    /// `SoundMultiplier = 1.6 with LOS, 0.16 otherwise`. Probed through a
    /// walking target at zero equipped weight, whose `MovementSound` is the
    /// bare `12`, so the multiplier is the only unknown. Losing LOS also
    /// zeroes `Visual`, which is already zero here.
    #[test]
    fn sound_multiplier_matches_the_source_table() {
        let walking = DetectionInputs {
            target_movement: MovementState::Walking,
            ..isolated()
        };
        approx(term(&walking), 1.6 * 12.0, "1.6 × MovementSound with LOS");
        approx(
            term(&DetectionInputs {
                detector_has_los: false,
                ..walking
            }),
            0.16 * 12.0,
            "0.16 × MovementSound without LOS",
        );
    }

    /// `MovementSound = (12 + EquippedWeight/2) × MovementMultiplier`, with
    /// the multiplier `0 / 1.5 / 1` for stationary-or-silent-running /
    /// running / otherwise. 100 lb of equipment must add exactly 50, not 100.
    #[test]
    fn movement_sound_matches_the_source_table() {
        let heavy = DetectionInputs {
            target_equipped_weight: 100.0,
            ..isolated()
        };
        for (movement, multiplier) in [
            (MovementState::Stationary, 0.0),
            (MovementState::Walking, 1.0),
            (MovementState::Running, 1.5),
            (MovementState::SilentRunning, 0.0),
        ] {
            approx(
                term(&DetectionInputs {
                    target_movement: movement,
                    ..heavy
                }),
                1.6 * ((12.0 + 100.0 / 2.0) * multiplier),
                "MovementSound",
            );
        }
    }

    /// `ActionSound` is `100 / 50 / 10 / 0` for loud / normal / silent / no
    /// action, and enters `Sound` **doubled**.
    #[test]
    fn action_sound_matches_the_source_table() {
        for (action, value) in [
            (ActionSound::Loud, 100.0),
            (ActionSound::Normal, 50.0),
            (ActionSound::Silent, 10.0),
            (ActionSound::None, 0.0),
        ] {
            assert_eq!(action.value(), value, "{action:?}");
            approx(
                term(&DetectionInputs {
                    target_action_sound: action,
                    ..isolated()
                }),
                1.6 * (2.0 * value),
                "2 × ActionSound",
            );
        }
    }

    /// `Light = 1.4 × min(100, LightLevel × nighteye)`, `nighteye = 3` with
    /// the effect and `1` without. Stationary, so `VisualMovement = 0` and
    /// `Visual` is `Light` exactly.
    #[test]
    fn light_and_night_eye_match_the_source_table() {
        for (light_level, night_eye, expected) in [
            (50.0, false, 1.4 * 50.0),
            (50.0, true, 1.4 * 100.0), // 150 → capped to 100
            (30.0, true, 1.4 * 90.0),  // 90 → under the cap, so ×3 shows
            (100.0, false, 1.4 * 100.0),
        ] {
            approx(
                term(&DetectionInputs {
                    light_level,
                    detector_has_night_eye: night_eye,
                    ..isolated()
                }),
                expected,
                "1.4 × min(100, light × nighteye)",
            );
        }
    }

    /// `VisualMovement = 0 / 0.01 / 0.21` for not moving / moving but not
    /// running / running, applied as `Light × (1 + VisualMovement)`. The
    /// expected value carries the movement's `Sound` contribution too, since
    /// the two cannot be varied independently.
    #[test]
    fn visual_movement_matches_the_source_table() {
        // Light = 1.4 × min(100, 50) = 70.
        let lit = DetectionInputs {
            light_level: 50.0,
            ..isolated()
        };
        for (movement, sound_multiplier, visual_movement) in [
            (MovementState::Stationary, 0.0, 0.0),
            (MovementState::Walking, 1.0, 0.01),
            (MovementState::Running, 1.5, 0.21),
            // The disclosed choice, not the source — see `MovementState`'s
            // docs and the capture document. Pinned so a future perk/movement
            // split is a deliberate edit here rather than a silent drift.
            (MovementState::SilentRunning, 0.0, 0.01),
        ] {
            approx(
                term(&DetectionInputs {
                    target_movement: movement,
                    ..lit
                }),
                1.6 * (12.0 * sound_multiplier) + 70.0 * (1.0 + visual_movement),
                "Light × (1 + VisualMovement)",
            );
        }
    }

    /// `Armor = 20 / 10 / 0` for heavy / medium / otherwise, subtracted from
    /// `TargetSkill` — which is itself halved, so heavy armor costs the
    /// target exactly 10 points of `Detection`. (Source note: the penalty is
    /// for armor, not for helmets.)
    #[test]
    fn armor_penalty_matches_the_source_table() {
        // Sneak skill 0 and equal levels ⇒ TargetSkill = max(50 − 10·1, 0)
        // − Armor = 40 − Armor.
        let sneaking = DetectionInputs {
            target_is_sneaking: true,
            target_level: 5,
            detector_level: 5, // 5·(5−5) = 0
            ..isolated()
        };
        for (armor, penalty) in [
            (ArmorClass::Heavy, 20.0),
            (ArmorClass::Medium, 10.0),
            (ArmorClass::Light, 0.0),
        ] {
            assert_eq!(armor.penalty(), penalty, "{armor:?}");
            // max(50 − 10·5, 0) = 0, so TargetSkill is exactly −penalty.
            approx(
                term(&DetectionInputs {
                    target_armor: armor,
                    ..sneaking
                }),
                penalty / 2.0,
                "−TargetSkill/2",
            );
        }
    }

    /// `DetectorSkill = (10 + 8·Perception) × DetectorState`, the state
    /// multiplier being `0.8` (sleeping / already fighting this target),
    /// `1.2` (alert, lost, or fighting someone else) or `1`.
    #[test]
    fn detector_skill_and_state_match_the_source_table() {
        for (state, multiplier) in [
            (DetectorState::SleepingOrFightingThisTarget, 0.8),
            (DetectorState::AlertLostOrFightingOther, 1.2),
            (DetectorState::Normal, 1.0),
        ] {
            assert_eq!(state.multiplier(), multiplier, "{state:?}");
            // Perception 10 ⇒ base 90, entering Detection as 90·mult/2.
            approx(
                detection_score(&DetectionInputs {
                    detector_perception: 10.0,
                    detector_state: state,
                    ..isolated()
                }),
                (10.0 + 8.0 * 10.0) * multiplier / 2.0 - 35.0,
                "DetectorSkill/2 − 35",
            );
        }
    }

    /// `Attenuation = ((MaxDist − distance)/MaxDist)²`, `MaxDist` 2500
    /// indoors / 5000 outdoors — squared, so half the maximum distance
    /// leaves a quarter of the term, not a half.
    #[test]
    fn attenuation_matches_the_captured_expression() {
        for (locale, max_distance) in [(Locale::Indoor, 2500.0), (Locale::Outdoor, 5000.0)] {
            for fraction in [0.0f32, 0.5, 1.0] {
                let inputs = DetectionInputs {
                    locale,
                    distance: max_distance * fraction,
                    // Something for the attenuation to bite on: a loud
                    // action at 1.6 × 2 × 100 = 320 of Sound, plus the
                    // constant 10/2 of DetectorSkill.
                    target_action_sound: ActionSound::Loud,
                    ..isolated()
                };
                let expected = (1.0 - fraction).powi(2) * (1.6 * 2.0 * 100.0 + 5.0) - 35.0;
                approx(detection_score(&inputs), expected, "Attenuation");
            }
        }
    }

    /// `TargetSkill = SneakSkill + 5·(TargetLevel − DetectorLevel)
    /// + max(50 − 10·TargetLevel, 0) − Armor`, and `0` when not sneaking.
    /// The `max(…, 0)` low-level bonus and the both-actors level term are
    /// FNV's addition over FO3.
    #[test]
    fn target_skill_matches_the_captured_expression() {
        for (sneak_skill, target_level, detector_level) in
            [(0.0, 1, 1), (75.0, 3, 8), (25.0, 10, 2), (100.0, 6, 6)]
        {
            let expected_target_skill = sneak_skill
                + 5.0 * (f32::from(target_level) - f32::from(detector_level))
                + (50.0 - 10.0 * f32::from(target_level)).max(0.0);
            approx(
                term(&DetectionInputs {
                    target_is_sneaking: true,
                    target_sneak_skill: sneak_skill,
                    target_level,
                    detector_level,
                    ..isolated()
                }),
                -expected_target_skill / 2.0,
                "−TargetSkill/2",
            );
        }
    }
}
