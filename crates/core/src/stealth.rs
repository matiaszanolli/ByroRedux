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
//! [`StealthKind::Oblivion`] is the CS formula (`oblivion`), [`StealthKind::Skyrim`]
//! `SkyrimSE.exe`'s (`skyrim`), with its stealth points, and [`StealthKind::Fallout4`]
//! `Fallout4.exe`'s split sound and visual values (`fallout4`, #6996). [`StealthKind::Starfield`] is its
//! metre-based descendant (`starfield`, #7046). A game without a sourced formula
//! ([`StealthKind::SightOnly`]) has no value; the tick answers its levels from the sight test.

mod fallout;
mod fallout4;
pub mod indicator;
pub mod oblivion;
mod settings;
pub mod skyrim;
mod starfield;

pub use indicator::{IndicatorGate, IndicatorModel, IndicatorState, IndicatorStyle, StealthIndicator};
pub use settings::{DetectionSettings, SettingReading};
pub use skyrim::{StealthPointInputs, StealthPoints};

/// Which formula a game's detection runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealthKind {
    /// No sourced formula: levels come from the actor-to-actor sight test.
    SightOnly,
    /// The GECK / `Fallout3.exe` formula without level terms (SNK-DET-003).
    Fallout3,
    /// The same with New Vegas's target-skill level and armor terms (SNK-DET-004).
    FalloutNv,
    /// The CS formula and `Oblivion.exe`'s levels (SNK-DET-005).
    Oblivion,
    /// `SkyrimSE.exe`'s formula with stealth points (SNK-DET-007, -015).
    Skyrim,
    /// `Fallout4.exe`'s sound and visual values with stealth points split by sense (#6996).
    Fallout4,
    /// `Starfield.exe`'s pair: Fallout 4's model re-based to metres, with an ambient-sound term and a sprint tier (#7046).
    Starfield,
}

/// A detector's awareness of the player (CS *GetDetectionLevel*: "0 = Lost :1 =
/// Unseen :2 = Noticed :3 = Seen"). FO3 / FNV store only Lost and Seen
/// (`Fallout3.exe` `0x874393`); the band between is the alert state. Oblivion
/// stores Lost, Noticed and Seen (`Oblivion.exe` `0x640900`); Skyrim's Noticed
/// is a hostile detector held off by the player's stealth points.
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
    /// The detector's Sneak skill (Oblivion, Skyrim: the detector term).
    pub detector_sneak_skill: f32,
    pub detector_night_eye: bool,
    /// The detector's Blindness, 0 to 100.
    pub detector_blindness: f32,
    pub target_sneaking: bool,
    /// The target's Sneak skill. Skyrim transforms it (`fPlayerDetectionSneak*`)
    /// and runs entry 57 over it inside the formula.
    pub target_sneak_skill: f32,
    pub target_level: u16,
    pub target_armor: ArmorClass,
    /// Equipped weight.
    pub target_weight: f32,
    pub target_moving: bool,
    pub target_running: bool,
    /// Starfield: the target is sprinting (`fDetectionSprintingMult`).
    pub target_sprinting: bool,
    /// Chameleon / invisibility (a Stealth Boy): no visual contact (FO3 / FNV).
    pub target_stealth_boy: bool,
    /// Invisibility alone (Oblivion: −100; Skyrim: no visual contact).
    pub target_invisible: bool,
    /// The size factor input (1 for a human).
    pub target_size: f32,
    /// Oblivion's Chameleon magnitude, 0 to 100.
    pub target_chameleon: f32,
    /// Oblivion: the target's boots' weight (the movement noise).
    pub target_boot_weight: f32,
    /// Oblivion: the target is in combat (sound bonus, no sneak subtraction).
    pub target_in_combat: bool,
    /// Oblivion: the target is swimming (no sound, sight scaled).
    pub target_swimming: bool,
    /// Oblivion: the target attacked this detector since its last evaluation.
    pub target_attacked_detector: bool,
    /// Skyrim: the movement-noise factor (1; Muffle lowers it).
    pub target_noise_mult: f32,
    /// Skyrim: the target race's size class, 0 small to 3 very large.
    pub target_size_class: u8,
    /// The action rating heard this period (0 for none; [`ActionSound::rating`]
    /// or a detection event's sound level).
    pub action: f32,
    /// The light level at the target, 0 to 100 (design §6).
    pub light_level: f32,
    /// Fallout 4: the cell's ambient share of [`Self::light_level`], 0 to 100 (an interior's light-level mod
    /// takes at most this much off).
    pub light_ambient: f32,
    /// Fallout 4: the detector's Deafness, 0 to 100 (100 hears nothing).
    pub detector_deafness: f32,
    /// Fallout 4: the target is the detector's combat target.
    pub detector_fighting_target: bool,
    /// Fallout 4: the target's Agility (the sneaking agility term).
    pub target_agility: f32,
    /// Fallout 4: the target's Invisibility, 0 to 100 (100 is a Stealth Boy: no visual term).
    pub target_invisibility: f32,
    /// Fallout 4: entry 15 (*Ignore Running During Detection*) holds for the target, which also silences
    /// its movement while sneaking.
    pub target_silent_movement: bool,
    /// Fallout 4: objects between the pair that hide the target (none modelled yet).
    pub target_occluders: u8,
    /// Fallout 4: the sneaking target is in cover (no cover rays yet).
    pub target_in_cover: bool,
    /// Fallout 4: the horizontal angle between the detector's facing and the target, radians.
    pub view_angle: f32,
    /// Fallout 4: the target's elevation seen from the detector, radians, positive above.
    pub view_pitch: f32,
    /// Starfield: the detector's ambient sound class 1 (silent) to 5 (very loud), 0 for none. The cell field it
    /// comes from is not identified (#7046), so the engine leaves it 0 and the ambient term off.
    pub ambient_sound_class: u8,
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
            detector_sneak_skill: 0.0,
            detector_night_eye: false,
            detector_blindness: 0.0,
            target_sneaking: false,
            target_sneak_skill: 0.0,
            target_level: 1,
            target_armor: ArmorClass::Light,
            target_weight: 0.0,
            target_moving: false,
            target_running: false,
            target_sprinting: false,
            target_stealth_boy: false,
            target_invisible: false,
            target_size: 1.0,
            target_chameleon: 0.0,
            target_boot_weight: 0.0,
            target_in_combat: false,
            target_swimming: false,
            target_attacked_detector: false,
            target_noise_mult: 1.0,
            target_size_class: 1,
            action: 0.0,
            light_level: 0.0,
            light_ambient: 0.0,
            detector_deafness: 0.0,
            detector_fighting_target: false,
            target_agility: 0.0,
            target_invisibility: 0.0,
            target_silent_movement: false,
            target_occluders: 0,
            target_in_cover: false,
            view_angle: 0.0,
            view_pitch: 0.0,
            ambient_sound_class: 0,
        }
    }
}

/// One evaluation's terms, kept for `stealth.status` (SNK-DX-001). Fallout 4's `sound` and `visual` are its
/// two rounded levels, `skill` the target's agility term (negated), `value` the higher level.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DetectionTerms {
    pub sound: f32,
    pub visual: f32,
    pub skill: f32,
    pub value: f32,
}

/// A Skyrim perk entry point the formula runs on the target's perks (SNK-PERK-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SneakPerkHook {
    /// 47 *Mod Detection Light*: the light level, before Sneak and Invisibility apply.
    DetectionLight,
    /// 48 *Mod Detection Movement*: the visual movement multiplier (`0x14041d0af`).
    DetectionMovement,
    /// 57 *Mod Detection Sneak Skill*: the target's sneak skill (`0x14041ce35`).
    DetectionSneakSkill,
}

/// What a sneak attack needs to know about the weapon: its `CRDT` Critical
/// Damage and Crit % Mult (FO3 / FNV, SNK-ATK-003), its animation type (FO3 /
/// FNV and Skyrim `DNAM`), and Oblivion's `DATA` weapon type.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SneakWeapon {
    pub critical_damage: f32,
    pub critical_chance_mult: f32,
    pub anim_type: u8,
    pub oblivion_type: u8,
}

/// What the level of one detector depends on besides its value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelContext {
    /// The detector would attack the player.
    pub hostile: bool,
    pub target_sneaking: bool,
    /// The player's stealth points (Skyrim, Fallout 4).
    pub stealth_points: f32,
    /// The evaluation's terms: Fallout 4 gates on its sound and visual levels apart.
    pub terms: DetectionTerms,
    /// The pair distance (Fallout 4's sound threshold applies within a radius).
    pub distance: f32,
}

impl Default for LevelContext {
    fn default() -> Self {
        Self { hostile: false, target_sneaking: false, stealth_points: f32::MAX, terms: DetectionTerms::default(), distance: 0.0 }
    }
}

/// Where a strike's unaware test reads the target's value from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrikeValue {
    /// Re-run the formula at the strike (`Fallout3.exe` `0x845580`).
    Fresh,
    /// The target's stored value toward the attacker (`Oblivion.exe` `0x5ff23a`,
    /// `SkyrimSE.exe` `0x1406a2520`).
    Stored,
}

/// A load order's resolved detection rules: the profile row's kind over its
/// settings. Installed as a resource by the binary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StealthRules {
    pub kind: StealthKind,
    pub settings: DetectionSettings,
}

impl crate::ecs::resource::Resource for StealthRules {}

impl StealthRules {
    /// Resolve `kind` against the load order's settings (`DetectionSettings::resolve`).
    #[must_use]
    pub fn resolve(kind: StealthKind, gmst: impl Fn(&str) -> Option<f32>) -> (Self, Vec<SettingReading>) {
        let (settings, readings) = DetectionSettings::resolve(kind, gmst);
        (Self { kind, settings }, readings)
    }

    /// The detection range: `fSneakMaxDistance`, scaled in an exterior.
    #[must_use]
    pub fn range(&self, exterior: bool) -> f32 {
        let s = &self.settings;
        if exterior { s.max_distance * s.exterior_distance_mult } else { s.max_distance }
    }

    /// The distance between a detector and its target from its horizontal and vertical parts: Fallout 4 weights
    /// the height by `fDetectionDistanceHeightMult` (`Fallout4.exe` `0x140cc0379`); the others take it whole.
    #[must_use]
    pub fn pair_distance(&self, horizontal: f32, vertical: f32) -> f32 {
        match self.kind {
            StealthKind::Fallout4 | StealthKind::Starfield => fallout4::pair_distance(&self.settings, horizontal, vertical),
            _ => horizontal.hypot(vertical),
        }
    }

    /// Seconds until one detector's next evaluation (SNK-DET-006): the fixed
    /// `fDetectionTimerSetting`, or on Skyrim a draw from
    /// [`fDetectionUpdateTimeMin`, `Max`] by `roll` in 0..1.
    #[must_use]
    pub fn period(&self, roll: f32) -> f32 {
        let s = &self.settings;
        match self.kind {
            StealthKind::Skyrim | StealthKind::Fallout4 | StealthKind::Starfield => {
                s.update_time_min + (s.update_time_max - s.update_time_min) * roll.clamp(0.0, 1.0)
            }
            _ => s.timer,
        }
        .max(0.01)
    }

    /// The full view-cone angle in degrees, where the game authors one
    /// (SNK-DET-008). Oblivion has no such setting and the CS formula no cone
    /// term, so it sees along any unobstructed ray.
    #[must_use]
    pub fn view_cone_degrees(&self) -> Option<f32> {
        match self.kind {
            StealthKind::Fallout3 | StealthKind::FalloutNv | StealthKind::Skyrim | StealthKind::Fallout4 | StealthKind::Starfield => {
                Some(self.settings.view_cone)
            },
            StealthKind::Oblivion | StealthKind::SightOnly => None,
        }
    }

    /// The detection value and its terms, or `None` for a game without a
    /// sourced formula. Skyrim's perk entry points take their unmodified value.
    #[must_use]
    pub fn value(&self, inputs: &DetectionInputs) -> Option<DetectionTerms> {
        self.value_with(inputs, &|_, value| value)
    }

    /// [`Self::value`] with Skyrim's perk entry points composed by `perks`.
    #[must_use]
    pub fn value_with(&self, inputs: &DetectionInputs, perks: &dyn Fn(SneakPerkHook, f32) -> f32) -> Option<DetectionTerms> {
        let s = &self.settings;
        Some(match self.kind {
            StealthKind::SightOnly => return None,
            StealthKind::Fallout3 => fallout::value(s, inputs, false),
            StealthKind::FalloutNv => fallout::value(s, inputs, true),
            StealthKind::Oblivion => oblivion::value(s, inputs),
            StealthKind::Skyrim => skyrim::value(s, inputs, perks),
            StealthKind::Fallout4 => fallout4::value(s, inputs, perks),
            StealthKind::Starfield => starfield::value(s, inputs, perks),
        })
    }

    /// The level a value means (SNK-DET-012): FO3 / FNV store Seen above
    /// `fSneakSeenMin` and Lost otherwise (`Fallout3.exe` `0x874393`); Oblivion
    /// adds Noticed above `fSneakNoticedMin` (`0x640900`); Skyrim gates a hostile
    /// detector on the stealth points ([`skyrim`]), Fallout 4 on them and its two
    /// levels' thresholds (`fallout4`).
    #[must_use]
    pub fn level(&self, value: f32, context: LevelContext) -> DetectionLevel {
        let s = &self.settings;
        match self.kind {
            StealthKind::Oblivion => oblivion::level(s, value),
            StealthKind::Skyrim => {
                skyrim::level(s, value, context.hostile, context.target_sneaking, context.stealth_points)
            }
            StealthKind::Fallout4 | StealthKind::Starfield => fallout4::level(
                s,
                &DetectionTerms { value, ..context.terms },
                context.distance,
                context.hostile,
                context.target_sneaking,
                context.stealth_points,
            ),
            _ => {
                if value > s.seen_min { DetectionLevel::Seen } else { DetectionLevel::Lost }
            }
        }
    }

    /// Whether a value puts a hostile detector on alert: above `fSneakNoticedMin`
    /// (GECK *Category:Detection*: "Alert Threshold"), or on Skyrim above 0 ("Comes
    /// within my detection threshold (detection > 0)", CK). Fallout 4 has no noticed
    /// threshold in its code: a level above 0 is one its stealth points hold off.
    #[must_use]
    pub fn noticed(&self, value: f32) -> bool {
        match self.kind {
            StealthKind::Skyrim | StealthKind::Fallout4 | StealthKind::Starfield => value > 0.0,
            _ => value > self.settings.noticed_min,
        }
    }

    /// Whether an Alert (or searching) detector returns to Normal: the alert
    /// time (`fDetectionEventExpireTime`) has passed since it last noticed
    /// anything, or on Skyrim the player's stealth points are full again ("all
    /// the targets have Stealth Points = 100", CK). Oblivion has no alert time
    /// (no such setting), so its alert lasts while the detector notices.
    #[must_use]
    pub fn alert_expired(&self, since_noticed: f32, stealth_points: f32) -> bool {
        match self.kind {
            StealthKind::Skyrim => stealth_points >= self.settings.stealth_point_max,
            StealthKind::Oblivion => true,
            _ => since_noticed >= self.settings.event_expire_time.max(0.0),
        }
    }

    /// Where a strike reads the target's value from (SNK-ATK-001).
    #[must_use]
    pub fn strike_value(&self) -> StrikeValue {
        match self.kind {
            StealthKind::Oblivion | StealthKind::Skyrim | StealthKind::Fallout4 | StealthKind::Starfield => StrikeValue::Stored,
            _ => StrikeValue::Fresh,
        }
    }

    /// Whether a target with `value` toward the attacker is unaware of it
    /// (SNK-ATK-001, V12): at most 0, and on Oblivion at most
    /// `iAICombatMinDetection` while the target fights the attacker.
    #[must_use]
    pub fn unaware(&self, value: f32, fighting_attacker: bool) -> bool {
        match self.kind {
            StealthKind::Oblivion => oblivion::unaware(&self.settings, value, fighting_attacker),
            _ => value <= 0.0,
        }
    }

    /// A sneak attack's damage and whether the hit carries the sneak flag
    /// (SNK-ATK-002 to -004), or `None` where the row has no sourced multipliers.
    /// `weapon` is `None` for an unarmed strike; `sneak_skill` is the attacker's
    /// Sneak (Oblivion's mastery); `mod_sneak_attack_mult` composes Skyrim's entry 18.
    ///
    /// - FO3 / FNV: a critical times the sneak multiplier, always flagged
    ///   (`fallout::sneak_attack_damage`).
    /// - Oblivion: × the mastery ladder for the weapon class, flagged even at 1
    ///   (two-handed weapons and staves).
    /// - Skyrim: × entry 18 of the class multiplier; flagged when above 1
    ///   (`0x1407f1b86`).
    /// - Fallout 4: the same over its twelve classes, Gun, Grenade and Mine
    ///   added (`0x140e3a1ae`).
    #[must_use]
    pub fn sneak_attack(
        &self,
        damage: f32,
        weapon: Option<SneakWeapon>,
        sneak_skill: f32,
        mod_sneak_attack_mult: &dyn Fn(f32) -> f32,
    ) -> Option<(f32, bool)> {
        let s = &self.settings;
        match self.kind {
            StealthKind::SightOnly => None,
            StealthKind::Fallout3 | StealthKind::FalloutNv => Some((fallout::sneak_attack_damage(s, damage, weapon), true)),
            StealthKind::Oblivion => Some((damage * oblivion::sneak_attack_mult(s, weapon, sneak_skill), true)),
            StealthKind::Skyrim => {
                let mult = mod_sneak_attack_mult(skyrim::sneak_attack_mult(s, weapon));
                Some((damage * mult, mult > 1.0))
            }
            StealthKind::Fallout4 | StealthKind::Starfield => {
                let mult = mod_sneak_attack_mult(fallout4::sneak_attack_mult(s, weapon));
                Some((damage * mult, mult > 1.0))
            }
        }
    }

    /// Step the player's stealth points one frame (Skyrim, Fallout 4; SNK-DET-015).
    pub fn step_stealth_points(&self, points: &mut StealthPoints, inputs: StealthPointInputs, dt: f32) {
        match self.kind {
            StealthKind::Skyrim => skyrim::step_points(&self.settings, points, inputs, dt),
            StealthKind::Fallout4 | StealthKind::Starfield => fallout4::step_points(&self.settings, points, inputs, dt),
            _ => {}
        }
    }

    /// Whether a hostile detector's evaluation passes Fallout 4's stealth-point thresholds, which empties the
    /// points (`Fallout4.exe` `0x140e9465d`); never on the other rows.
    #[must_use]
    pub fn exceeds_stealth_thresholds(&self, terms: &DetectionTerms, distance: f32, target_sneaking: bool) -> bool {
        matches!(self.kind, StealthKind::Fallout4 | StealthKind::Starfield) && fallout4::exceeds_thresholds(&self.settings, terms, distance, target_sneaking)
    }
}

/// The distance attenuation every formula shares: `max(0, (maxD − d) / maxD) ^
/// exponent` over the range (Oblivion's linear Distance Factor is exponent 1).
fn attenuation(s: &DetectionSettings, i: &DetectionInputs, exponent: f32) -> f32 {
    let max_distance = if i.exterior { s.max_distance * s.exterior_distance_mult } else { s.max_distance };
    if max_distance > 0.0 {
        ((max_distance - i.distance) / max_distance).max(0.0).powf(exponent)
    } else {
        0.0
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
        assert_eq!(rules.settings, DetectionSettings::fallout3_defaults());
        assert_eq!(readings.len(), 32, "the Fallout rows read 32 settings");
        assert!(readings.iter().all(|reading| !reading.authored));
        assert_eq!(rules.range(false), 1500.0);
        assert_eq!(rules.range(true), 3000.0);
        approx(rules.period(0.7), 0.3, "fDetectionTimerSetting");
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
        let level = |value| rules.level(value, LevelContext::default());
        assert_eq!(level(0.5), DetectionLevel::Seen);
        assert_eq!(level(0.0), DetectionLevel::Lost);
        assert_eq!(level(-19.0), DetectionLevel::Lost);
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
        let knife = SneakWeapon { critical_damage: 10.0, critical_chance_mult: 1.0, anim_type: 1, ..Default::default() };
        let pistol = SneakWeapon { critical_damage: 12.0, critical_chance_mult: 1.0, anim_type: 3, ..Default::default() };
        let no_crits = SneakWeapon { critical_chance_mult: 0.0, ..pistol };
        let damage = |weapon, base| {
            let (damage, flagged) = rules.sneak_attack(base, weapon, 0.0, &|mult| mult).unwrap();
            assert!(flagged);
            damage
        };
        approx(damage(Some(knife), 20.0), (20.0 + 10.0) * 5.0, "melee ×5");
        approx(damage(Some(pistol), 20.0), (20.0 + 12.0) * 2.0, "ranged ×2");
        approx(damage(Some(no_crits), 20.0), 20.0 * 2.0, "no critical, still ×2");
        approx(damage(None, 8.0), (8.0 + 8.0) * 5.0, "unarmed: crit = damage × 1, ×5");
        let sight_only = StealthRules { kind: StealthKind::SightOnly, ..rules };
        assert_eq!(sight_only.sneak_attack(20.0, Some(knife), 0.0, &|mult| mult), None);
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
