//! Stealth detection: the per-game detection value, level and alert state
//! (M65, `docs/engine/sneak-detection.md` §4).
//!
//! Pure math, no ECS. The binary's detection tick gathers each detector's
//! inputs and calls [`StealthRules::value`]; the rules are a character-ruleset
//! row (`CharacterRulesProfile::stealth`), resolved against the load order's
//! game settings at install, so no caller branches on the game (SNK-DET-002).
//!
//! ## The Fallout 3 / New Vegas formula
//!
//! [`StealthKind::Fallout3`] is the formula `Fallout3.exe` computes at
//! `0x57e300` (design §4.2), which has the shape of the GECK's
//! (*Category:Detection*) with every coefficient a game setting:
//!
//! ```text
//! P      = fSneakPerceptionSkillMin + (Max − Min) · Perception / 10
//! mod    = 1 + alert·fSneakAlertMod + sleeping·fSneakSleepBonus + notCombatTarget·fSneakCombatMod
//! att    = max(0, (maxD − d) / maxD) ^ fSneakDistanceAttenuationExponent
//! sound  = max(0, att · [no LOS: fSneakSoundLosMult] · fSneakSoundsMult
//!            · (action · int(fSneakActionMult) + moving · (BootBase + BootMult·weight) · [running: fSneakRunningMult]))
//! visual = max(0, sees · att · (1 + moving·fSneakLightMoveMult + running·fSneakLightRunMult)
//!            · (light + fDetectionSneakLightMod) · fSneakLightMult · blindness · size) · [stealth boy: StealthBoyMult]
//! skill  = (att · P · mod − sneaking · targetSkill) · fSneakSkillMult
//! value  = fSneakBaseValue + sound + visual + skill
//! ```
//!
//! [`StealthKind::FalloutNv`] is the same shape plus New Vegas's target-skill
//! terms from the Fallout wiki capture (CHARAL FNV ruleset *Sneak Detection*):
//! `TargetSkill = Sneak + 5·(TargetLevel − DetectorLevel) + max(50 − 10·TargetLevel, 0) − Armor`.
//! The capture's other constants (base −35, sound 1.6 / 0.16, `12 + w/2`,
//! running 1.5, action ×2 with Silent 10 / Normal 50 / Loud 100, light 1.4,
//! visual movement 0.01 / 0.21, `10 + 8·Perception`, the halving) are exactly
//! the settings' values, so they are read from the load order rather than
//! written here. The capture's `DetectorState` grouping (0.8 / 1.2) is
//! superseded by the settings form the code shows.
//!
//! A game without a sourced formula ([`StealthKind::SightOnly`]: Oblivion and
//! Skyrim until Phase 5, Fallout 4 and Starfield per Q2) has no value; the
//! tick answers its levels from the sight test.

pub mod indicator;

pub use indicator::{IndicatorGate, IndicatorModel, IndicatorState, IndicatorStyle, StealthIndicator};

/// Which formula a game's detection runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealthKind {
    /// No sourced formula: levels come from the actor-to-actor sight test.
    SightOnly,
    /// The GECK / `Fallout3.exe` formula without level terms (SNK-DET-003).
    Fallout3,
    /// The same with New Vegas's target-skill level and armor terms (SNK-DET-004).
    FalloutNv,
}

/// A detector's awareness of the player (CS *GetDetectionLevel*: "0 = Lost :1 =
/// Unseen :2 = Noticed :3 = Seen"). FO3 / FNV store only Lost and Seen
/// (`Fallout3.exe` `0x874393`); the band between is the alert state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum DetectionLevel {
    #[default]
    Lost = 0,
    Unseen = 1,
    Noticed = 2,
    Seen = 3,
}

impl DetectionLevel {
    /// The number `GetDetectionLevel` answers.
    #[must_use]
    pub const fn as_number(self) -> u8 {
        self as u8
    }
}

/// The detector's own state: "Normal->Alert->Combat->Lost->Normal" (GECK and
/// CK *Category:Detection*). *Alert state Lost* (searching after combat) is not
/// *level Lost*.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AlertState {
    #[default]
    Normal,
    Alert,
    Combat,
    Lost,
}

/// The target's worn-armor noise class (New Vegas only: medium and heavy
/// armor subtract 10 / 20 from the target skill, Fallout wiki capture).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArmorClass {
    #[default]
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

/// The action rating of a weapon's sound level (`WEAP` `VNAM`, SNK-DET-016):
/// the Fallout wiki capture's Silent 10, Normal 50, Loud 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSound {
    Silent,
    Normal,
    Loud,
}

impl ActionSound {
    #[must_use]
    pub const fn rating(self) -> f32 {
        match self {
            ActionSound::Silent => 10.0,
            ActionSound::Normal => 50.0,
            ActionSound::Loud => 100.0,
        }
    }
}

/// Everything one detection roll needs for one detector and the player. The
/// caller resolves the ECS state into these plain values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectionInputs {
    /// World-unit distance between the two.
    pub distance: f32,
    /// Whether the pair is in an exterior (the range is scaled there).
    pub exterior: bool,
    /// An unobstructed ray between them (the sound term's line of sight).
    pub line_of_sight: bool,
    /// The line of sight and the detector's view cone both hold (visual contact).
    pub sees: bool,
    pub detector_perception: f32,
    /// The detector's alert state is Alert or alert state Lost.
    pub detector_alert: bool,
    pub detector_sleeping: bool,
    /// The detector is in combat, but not with the target.
    pub detector_not_combat_target: bool,
    pub detector_level: u16,
    pub detector_night_eye: bool,
    /// The detector's Blindness, 0 to 100.
    pub detector_blindness: f32,
    pub target_sneaking: bool,
    pub target_sneak_skill: f32,
    pub target_level: u16,
    pub target_armor: ArmorClass,
    /// Equipped weight.
    pub target_weight: f32,
    pub target_moving: bool,
    pub target_running: bool,
    /// Chameleon / invisibility (a Stealth Boy): no visual contact.
    pub target_stealth_boy: bool,
    /// The size factor input (1 for a human).
    pub target_size: f32,
    /// The action rating heard this period (0 for none; [`ActionSound::rating`]
    /// or a detection event's sound level).
    pub action: f32,
    /// The light level at the target, 0 to 100 (design §6).
    pub light_level: f32,
}

impl Default for DetectionInputs {
    fn default() -> Self {
        Self {
            distance: 0.0,
            exterior: false,
            line_of_sight: true,
            sees: true,
            detector_perception: 5.0,
            detector_alert: false,
            detector_sleeping: false,
            detector_not_combat_target: false,
            detector_level: 1,
            detector_night_eye: false,
            detector_blindness: 0.0,
            target_sneaking: false,
            target_sneak_skill: 0.0,
            target_level: 1,
            target_armor: ArmorClass::Light,
            target_weight: 0.0,
            target_moving: false,
            target_running: false,
            target_stealth_boy: false,
            target_size: 1.0,
            action: 0.0,
            light_level: 0.0,
        }
    }
}

/// One evaluation's terms, kept for `stealth.status` (SNK-DX-001).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DetectionTerms {
    pub sound: f32,
    pub visual: f32,
    pub skill: f32,
    pub value: f32,
}

macro_rules! geck_settings {
    ($( $field:ident : $name:literal = $default:expr ),+ $(,)?) => {
        /// The FO3 / FNV detection settings. Defaults are `Fallout3.exe`'s
        /// (design §13.1; New Vegas shares them, its executable being
        /// encrypted); the load order's values replace them at install.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct GeckSettings { $( pub $field: f32, )+ }

        impl GeckSettings {
            /// `Fallout3.exe`'s defaults (the census's E column).
            pub const EXECUTABLE_DEFAULTS: Self = Self { $( $field: $default, )+ };

            /// Every setting's EditorID, in declaration order.
            pub const NAMES: &'static [&'static str] = &[ $( $name, )+ ];

            /// Replace each default by the load order's value where it
            /// authors one; report every setting and whether it was authored
            /// (SNK-DX-003).
            pub fn with_gmst(mut self, gmst: impl Fn(&str) -> Option<f32>) -> (Self, Vec<SettingReading>) {
                let mut readings = Vec::with_capacity(Self::NAMES.len());
                $(
                    let authored = gmst($name).filter(|value| value.is_finite());
                    if let Some(value) = authored {
                        self.$field = value;
                    }
                    readings.push(SettingReading { name: $name, value: self.$field, authored: authored.is_some() });
                )+
                (self, readings)
            }
        }
    };
}

geck_settings! {
    base_value: "fSneakBaseValue" = -25.0,
    max_distance: "fSneakMaxDistance" = 1500.0,
    exterior_distance_mult: "fSneakExteriorDistanceMult" = 2.0,
    attenuation_exponent: "fSneakDistanceAttenuationExponent" = 2.0,
    sound_los_mult: "fSneakSoundLosMult" = 0.25,
    sounds_mult: "fSneakSoundsMult" = 1.6,
    boot_weight_base: "fSneakBootWeightBase" = 14.0,
    boot_weight_mult: "fSneakBootWeightMult" = 1.0,
    running_mult: "fSneakRunningMult" = 2.0,
    action_mult: "fSneakActionMult" = 1.0,
    light_move_mult: "fSneakLightMoveMult" = 0.01,
    light_run_mult: "fSneakLightRunMult" = 0.2,
    light_mod: "fDetectionSneakLightMod" = 0.0,
    light_mult: "fSneakLightMult" = 1.4,
    stealth_boy_mult: "fSneakStealthBoyMult" = 0.0,
    night_eye_bonus: "fDetectionNightEyeBonus" = 3.0,
    perception_skill_min: "fSneakPerceptionSkillMin" = 10.0,
    perception_skill_max: "fSneakPerceptionSkillMax" = 90.0,
    alert_mod: "fSneakAlertMod" = 0.2,
    sleep_bonus: "fSneakSleepBonus" = -0.2,
    combat_mod: "fSneakCombatMod" = -0.4,
    skill_mult: "fSneakSkillMult" = 0.5,
    noticed_min: "fSneakNoticedMin" = -20.0,
    seen_min: "fSneakSeenMin" = 0.0,
    timer: "fDetectionTimerSetting" = 0.3,
    view_cone: "fDetectionViewCone" = 190.0,
    event_expire_time: "fDetectionEventExpireTime" = 10.0,
    large_actor_threshold: "fDetectionLargeActorThreshold" = 135.0,
    large_actor_size_mult: "fDetectionLargeActorSizeMult" = 1.0,
    sneak_damage_mult: "fCombatDamageBonusSneakingMult" = 1.0,
    melee_sneak_damage_mult: "fCombatDamageBonusMeleeSneakingMult" = 5.0,
    unarmed_crit_damage_mult: "fCombatUnarmedCritDamageMult" = 0.5,
}

/// What a sneak attack needs to know about the weapon (SNK-ATK-003): its `CRDT`
/// Critical Damage and Crit % Mult, and whether it is a melee weapon (FO3 / FNV
/// animation types 0–2: hand-to-hand, one- and two-handed melee).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SneakWeapon {
    pub critical_damage: f32,
    pub critical_chance_mult: f32,
    pub melee: bool,
}

/// One setting the install read: its value, and whether the load order
/// authors it or it fell back to the executable default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SettingReading {
    pub name: &'static str,
    pub value: f32,
    pub authored: bool,
}

/// A load order's resolved detection rules: the profile row's kind over its
/// settings. Installed as a resource by the binary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StealthRules {
    pub kind: StealthKind,
    pub settings: GeckSettings,
}

impl crate::ecs::resource::Resource for StealthRules {}

impl StealthRules {
    /// Resolve `kind` against the load order's settings. A [`StealthKind::SightOnly`]
    /// row reads only the range settings it shares with the sight test.
    #[must_use]
    pub fn resolve(kind: StealthKind, gmst: impl Fn(&str) -> Option<f32>) -> (Self, Vec<SettingReading>) {
        let (settings, readings) = GeckSettings::EXECUTABLE_DEFAULTS.with_gmst(gmst);
        let readings = match kind {
            StealthKind::SightOnly => readings
                .into_iter()
                .filter(|reading| matches!(reading.name, "fSneakMaxDistance" | "fSneakExteriorDistanceMult"))
                .collect(),
            _ => readings,
        };
        (Self { kind, settings }, readings)
    }

    /// The detection range: `fSneakMaxDistance`, scaled in an exterior.
    #[must_use]
    pub fn range(&self, exterior: bool) -> f32 {
        let s = &self.settings;
        if exterior { s.max_distance * s.exterior_distance_mult } else { s.max_distance }
    }

    /// Seconds between one detector's re-evaluations (SNK-DET-006).
    #[must_use]
    pub fn period(&self) -> f32 {
        self.settings.timer.max(0.01)
    }

    /// The full view-cone angle in degrees, where the game authors one (SNK-DET-008).
    #[must_use]
    pub fn view_cone_degrees(&self) -> Option<f32> {
        match self.kind {
            StealthKind::Fallout3 | StealthKind::FalloutNv => Some(self.settings.view_cone),
            StealthKind::SightOnly => None,
        }
    }

    /// The detection value and its terms, or `None` for a game without a
    /// sourced formula.
    #[must_use]
    pub fn value(&self, inputs: &DetectionInputs) -> Option<DetectionTerms> {
        let level_terms = match self.kind {
            StealthKind::SightOnly => return None,
            StealthKind::Fallout3 => false,
            StealthKind::FalloutNv => true,
        };
        Some(geck_value(&self.settings, inputs, level_terms))
    }

    /// The level a value means (SNK-DET-012): FO3 / FNV store Seen above
    /// `fSneakSeenMin` and Lost otherwise (`Fallout3.exe` `0x874393`).
    #[must_use]
    pub fn level(&self, value: f32) -> DetectionLevel {
        if value > self.settings.seen_min { DetectionLevel::Seen } else { DetectionLevel::Lost }
    }

    /// Whether a value crosses the alert threshold (`fSneakNoticedMin`, GECK
    /// *Category:Detection*: "Alert Threshold").
    #[must_use]
    pub fn noticed(&self, value: f32) -> bool {
        value > self.settings.noticed_min
    }

    /// Seconds a detector stays Alert (or alert state Lost) after the last
    /// thing it noticed (`fDetectionEventExpireTime`).
    #[must_use]
    pub fn alert_time(&self) -> f32 {
        self.settings.event_expire_time.max(0.0)
    }
}

impl StealthRules {
    /// The damage of a sneak attack (SNK-ATK-003), or `None` where the row has no
    /// sourced multipliers (a sight-only row; Oblivion and Skyrim are Phase 5).
    ///
    /// `Fallout3.exe` (design §8): a sneak attack multiplies the critical chance
    /// by `fCombatSneakAttackBonusMult` (100 in both masters, so it is a
    /// critical) unless the weapon's Crit % Mult is 0 ("If set to zero, the
    /// weapon will not cause any critical hits, even for sneak attacks", GECK
    /// *Weapons*). A critical adds the weapon's Critical Damage, or for an
    /// unarmed strike the damage × `fCombatUnarmedCritDamageMult` (`0x845753`).
    /// Then the damage is multiplied by `fCombatDamageBonusMeleeSneakingMult`
    /// for unarmed and melee weapons, or `fCombatDamageBonusSneakingMult`
    /// otherwise (`0x845980`). `weapon` is `None` for an unarmed strike.
    #[must_use]
    pub fn sneak_attack_damage(&self, damage: f32, weapon: Option<SneakWeapon>) -> Option<f32> {
        if self.kind == StealthKind::SightOnly {
            return None;
        }
        let s = &self.settings;
        let critical = match weapon {
            Some(weapon) if weapon.critical_chance_mult > 0.0 => weapon.critical_damage,
            Some(_) => 0.0,
            None => damage * s.unarmed_crit_damage_mult,
        };
        let melee = weapon.is_none_or(|weapon| weapon.melee);
        let mult = if melee { s.melee_sneak_damage_mult } else { s.sneak_damage_mult };
        Some((damage + critical) * mult)
    }
}

/// Step one detector's alert state toward the player (SNK-DET-013; GECK
/// *Category:Detection* transitions). `hostile`: the detector would attack the
/// player. `noticed`: the value crossed the alert threshold, or a detection
/// event or damage from the player arrived. `expired`: nothing was noticed for
/// the alert time. A detector not hostile to the player stays Normal.
#[must_use]
pub fn step_alert(previous: AlertState, level: DetectionLevel, hostile: bool, noticed: bool, expired: bool) -> AlertState {
    if !hostile {
        return AlertState::Normal;
    }
    let seen = level == DetectionLevel::Seen;
    match previous {
        _ if seen => AlertState::Combat,
        AlertState::Combat => AlertState::Lost,
        AlertState::Normal if noticed => AlertState::Alert,
        AlertState::Normal => AlertState::Normal,
        AlertState::Alert | AlertState::Lost if expired && !noticed => AlertState::Normal,
        state => state,
    }
}

fn geck_value(s: &GeckSettings, i: &DetectionInputs, level_terms: bool) -> DetectionTerms {
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    let max_distance = if i.exterior { s.max_distance * s.exterior_distance_mult } else { s.max_distance };
    let attenuation = if max_distance > 0.0 {
        ((max_distance - i.distance) / max_distance).max(0.0).powf(s.attenuation_exponent)
    } else {
        0.0
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `FalloutNV.esm`'s authored values over the executable defaults (design
    /// §13.1): the settings the FNV capture's constants come from.
    fn fnv() -> StealthRules {
        let authored = |name: &str| match name {
            "fSneakBaseValue" => Some(-35.0),
            "fSneakMaxDistance" => Some(2500.0),
            "fSneakExteriorDistanceMult" => Some(2.0),
            "fSneakSoundLosMult" => Some(0.1),
            "fSneakBootWeightBase" => Some(12.0),
            "fSneakBootWeightMult" => Some(0.5),
            "fSneakRunningMult" => Some(1.5),
            "fSneakActionMult" => Some(2.0),
            "fSneakLightMult" => Some(1.4),
            "fSneakSkillMult" => Some(0.5),
            _ => None,
        };
        StealthRules::resolve(StealthKind::FalloutNv, authored).0
    }

    fn fo3() -> StealthRules {
        StealthRules { kind: StealthKind::Fallout3, ..fnv() }
    }

    fn approx(actual: f32, expected: f32, what: &str) {
        assert!((actual - expected).abs() < 1e-3, "{what}: expected {expected}, got {actual}");
    }

    /// The Fallout wiki capture, written out with its own constants (CHARAL FNV
    /// ruleset *Sneak Detection*), as the transcription oracle (PRD §7.2
    /// scenario 2). Its `DetectorState` is taken as the settings form.
    fn capture(i: &DetectionInputs) -> f32 {
        let max = if i.exterior { 5000.0 } else { 2500.0 };
        let att = ((max - i.distance) / max).max(0.0).powi(2);
        let sound_mult = if i.line_of_sight { 1.6 } else { 0.16 };
        let movement = if i.target_moving { (12.0 + i.target_weight / 2.0) * if i.target_running { 1.5 } else { 1.0 } } else { 0.0 };
        let sound = sound_mult * (movement + 2.0 * i.action);
        let visual = if i.sees && !i.target_stealth_boy {
            let light = 1.4 * (i.light_level * if i.detector_night_eye { 3.0 } else { 1.0 }).min(100.0);
            let motion = if i.target_running { 0.21 } else if i.target_moving { 0.01 } else { 0.0 };
            light * (1.0 + motion)
        } else {
            0.0
        };
        let state = 1.0 + if i.detector_alert { 0.2 } else { 0.0 } + if i.detector_sleeping { -0.2 } else { 0.0 }
            + if i.detector_not_combat_target { -0.4 } else { 0.0 };
        let detector = (10.0 + 8.0 * i.detector_perception) * state;
        let target = if i.target_sneaking {
            i.target_sneak_skill + 5.0 * (f32::from(i.target_level) - f32::from(i.detector_level))
                + (50.0 - 10.0 * f32::from(i.target_level)).max(0.0)
                - i.target_armor.penalty()
        } else {
            0.0
        };
        att * (sound + visual + detector / 2.0) - target / 2.0 - 35.0
    }

    fn cases() -> Vec<DetectionInputs> {
        let base = DetectionInputs::default();
        vec![
            base,
            DetectionInputs { distance: 800.0, light_level: 40.0, ..base },
            DetectionInputs { distance: 300.0, exterior: true, target_moving: true, target_weight: 30.0, ..base },
            DetectionInputs { target_moving: true, target_running: true, target_weight: 12.0, light_level: 70.0, ..base },
            DetectionInputs { line_of_sight: false, sees: false, action: ActionSound::Loud.rating(), ..base },
            DetectionInputs { target_sneaking: true, target_sneak_skill: 75.0, target_level: 8, detector_level: 3, ..base },
            DetectionInputs { target_sneaking: true, target_armor: ArmorClass::Heavy, target_level: 2, ..base },
            DetectionInputs { detector_alert: true, detector_perception: 8.0, light_level: 10.0, ..base },
            DetectionInputs { detector_sleeping: true, detector_night_eye: true, light_level: 50.0, ..base },
            DetectionInputs { target_stealth_boy: true, light_level: 90.0, action: ActionSound::Silent.rating(), ..base },
        ]
    }

    /// PRD §7.2 scenario 2 (FNV): the settings-driven formula over
    /// `FalloutNV.esm`'s values matches the captured transcription.
    #[test]
    fn the_fnv_row_matches_the_captured_formula() {
        let rules = fnv();
        for inputs in cases() {
            approx(rules.value(&inputs).unwrap().value, capture(&inputs), &format!("{inputs:?}"));
        }
    }

    /// PRD §7.2 scenario 2 (FO3): the same shape without New Vegas's level and
    /// armor terms: the target skill is the Sneak skill alone.
    #[test]
    fn the_fo3_row_drops_the_level_and_armor_terms() {
        let rules = fo3();
        for inputs in cases() {
            let fo3_target = if inputs.target_sneaking { inputs.target_sneak_skill } else { 0.0 };
            let fnv_target = if inputs.target_sneaking {
                inputs.target_sneak_skill + 5.0 * (f32::from(inputs.target_level) - f32::from(inputs.detector_level))
                    + (50.0 - 10.0 * f32::from(inputs.target_level)).max(0.0)
                    - inputs.target_armor.penalty()
            } else {
                0.0
            };
            approx(
                rules.value(&inputs).unwrap().value,
                capture(&inputs) + (fnv_target - fo3_target) * 0.5,
                &format!("{inputs:?}"),
            );
        }
    }

    #[test]
    fn executable_defaults_apply_where_the_load_order_authors_nothing() {
        let (rules, readings) = StealthRules::resolve(StealthKind::Fallout3, |_| None);
        assert_eq!(rules.settings, GeckSettings::EXECUTABLE_DEFAULTS);
        assert_eq!(readings.len(), GeckSettings::NAMES.len());
        assert!(readings.iter().all(|reading| !reading.authored));
        assert_eq!(rules.range(false), 1500.0);
        assert_eq!(rules.range(true), 3000.0);
        approx(rules.period(), 0.3, "fDetectionTimerSetting");
        assert_eq!(rules.view_cone_degrees(), Some(190.0));
    }

    #[test]
    fn sight_only_rows_have_no_value_and_read_only_the_range() {
        let (rules, readings) = StealthRules::resolve(StealthKind::SightOnly, |name| (name == "fSneakMaxDistance").then_some(2500.0));
        assert!(rules.value(&DetectionInputs::default()).is_none());
        assert_eq!(rules.view_cone_degrees(), None);
        assert_eq!(readings.iter().map(|r| r.name).collect::<Vec<_>>(), ["fSneakMaxDistance", "fSneakExteriorDistanceMult"]);
        assert!(readings[0].authored);
    }

    /// SNK-DET-012: FO3 / FNV store Seen above `fSneakSeenMin` (0) and Lost
    /// otherwise; `fSneakNoticedMin` (−20) is the alert threshold.
    #[test]
    fn the_level_and_the_alert_threshold_follow_the_settings() {
        let rules = fnv();
        assert_eq!(rules.level(0.5), DetectionLevel::Seen);
        assert_eq!(rules.level(0.0), DetectionLevel::Lost);
        assert_eq!(rules.level(-19.0), DetectionLevel::Lost);
        assert!(rules.noticed(-19.0));
        assert!(!rules.noticed(-20.0));
        assert_eq!(DetectionLevel::Seen.as_number(), 3);
    }

    #[test]
    fn distance_light_and_movement_raise_the_value_and_cover_lowers_it() {
        let rules = fnv();
        let value = |inputs: DetectionInputs| rules.value(&inputs).unwrap().value;
        let base = DetectionInputs { distance: 600.0, light_level: 30.0, ..DetectionInputs::default() };
        assert!(value(DetectionInputs { distance: 100.0, ..base }) > value(base));
        assert!(value(DetectionInputs { light_level: 80.0, ..base }) > value(base));
        assert!(value(DetectionInputs { target_moving: true, target_running: true, ..base }) > value(base));
        assert!(value(DetectionInputs { line_of_sight: false, sees: false, ..base }) < value(base));
        assert!(value(DetectionInputs { target_sneaking: true, target_sneak_skill: 80.0, ..base }) < value(base));
        assert!(value(DetectionInputs { detector_blindness: 1.0, ..base }) < value(base), "integer division");
        let far = rules.value(&DetectionInputs { distance: 9000.0, ..base }).unwrap();
        assert_eq!((far.sound, far.visual), (0.0, 0.0), "past the range the attenuation clamps to 0");
    }

    /// PRD §7.2 scenario 8 (SNK-ATK-003), the numbers: FNV's masters author the ranged ×2 and the unarmed crit
    /// ×1; melee ×5 is the executable default. A critical adds the weapon's Critical Damage unless its Crit % Mult
    /// is 0; a sight-only row has no multiplier yet.
    #[test]
    fn a_sneak_attack_is_a_critical_times_the_sneak_multiplier() {
        let rules = StealthRules::resolve(StealthKind::FalloutNv, |name| match name {
            "fCombatDamageBonusSneakingMult" => Some(2.0),
            "fCombatUnarmedCritDamageMult" => Some(1.0),
            _ => None,
        })
        .0;
        let knife = SneakWeapon { critical_damage: 10.0, critical_chance_mult: 1.0, melee: true };
        let pistol = SneakWeapon { critical_damage: 12.0, critical_chance_mult: 1.0, melee: false };
        let no_crits = SneakWeapon { critical_chance_mult: 0.0, ..pistol };
        approx(rules.sneak_attack_damage(20.0, Some(knife)).unwrap(), (20.0 + 10.0) * 5.0, "melee ×5");
        approx(rules.sneak_attack_damage(20.0, Some(pistol)).unwrap(), (20.0 + 12.0) * 2.0, "ranged ×2");
        approx(rules.sneak_attack_damage(20.0, Some(no_crits)).unwrap(), 20.0 * 2.0, "no critical, still ×2");
        approx(rules.sneak_attack_damage(8.0, None).unwrap(), (8.0 + 8.0) * 5.0, "unarmed: crit = damage × 1, ×5");
        let sight_only = StealthRules { kind: StealthKind::SightOnly, ..rules };
        assert_eq!(sight_only.sneak_attack_damage(20.0, Some(knife)), None);
    }

    /// SNK-DET-013: the GECK transitions toward a hostile player.
    #[test]
    fn the_alert_state_steps_through_the_geck_transitions() {
        use AlertState::*;
        use DetectionLevel::{Lost as Unaware, Seen};
        assert_eq!(step_alert(Normal, Unaware, true, true, false), Alert, "noticed");
        assert_eq!(step_alert(Normal, Seen, true, false, false), Combat, "seen without alert");
        assert_eq!(step_alert(Alert, Seen, true, true, false), Combat);
        assert_eq!(step_alert(Combat, Unaware, true, false, false), Lost);
        assert_eq!(step_alert(Lost, Seen, true, false, false), Combat);
        assert_eq!(step_alert(Lost, Unaware, true, false, true), Normal, "the alert timer ran out");
        assert_eq!(step_alert(Alert, Unaware, true, true, true), Alert, "still noticing");
        assert_eq!(step_alert(Combat, Seen, false, true, false), Normal, "not hostile: stays Normal");
    }
}
