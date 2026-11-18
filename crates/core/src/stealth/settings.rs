//! The detection settings each formula reads, with each executable's default
//! (design §13.1). A row reads only the settings its game has; the load
//! order's authored values replace the defaults at install (SNK-DX-003).

use super::StealthKind;

/// "The game has no such setting": the row neither reads nor reports it.
const NA: f32 = f32::NAN;

/// Which executable's default table a row falls back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    /// `Fallout3.exe` (New Vegas's is encrypted and shares the code).
    Fallout3 = 0,
    Oblivion = 1,
    Skyrim = 2,
    Fallout4 = 3,
    /// `Starfield.exe`: each setting is built by its own initializer, read by `scripts/exe-setting-defaults.py` (#7046).
    Starfield = 4,
}

macro_rules! detection_settings {
    ($( $field:ident : $name:literal = [$fo3:expr, $obl:expr, $sky:expr, $fo4:expr, $sf:expr] ),+ $(,)?) => {
        /// Every detection setting a row reads. A field the row's game lacks
        /// stays 0 and is not reported.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct DetectionSettings { $( pub $field: f32, )+ }

        impl DetectionSettings {
            /// Every setting's EditorID, in declaration order.
            pub const NAMES: &'static [&'static str] = &[ $( $name, )+ ];

            const DEFAULTS: [Self; 5] = [
                Self { $( $field: $fo3, )+ },
                Self { $( $field: $obl, )+ },
                Self { $( $field: $sky, )+ },
                Self { $( $field: $fo4, )+ },
                Self { $( $field: $sf, )+ },
            ];

            /// The settings `kind` reads, each the load order's value where it
            /// authors one and the executable's default otherwise, with a
            /// reading per setting (SNK-DX-003).
            pub fn resolve(kind: StealthKind, gmst: impl Fn(&str) -> Option<f32>) -> (Self, Vec<SettingReading>) {
                let defaults = Self::DEFAULTS[column(kind) as usize];
                let reads = |name: &str| match kind {
                    // A sight-only row reads only the range it shares with the sight test.
                    StealthKind::SightOnly => matches!(name, "fSneakMaxDistance" | "fSneakExteriorDistanceMult"),
                    _ => true,
                };
                let mut out = Self { $( $field: 0.0, )+ };
                let mut readings = Vec::new();
                $(
                    if !defaults.$field.is_nan() && reads($name) {
                        let authored = gmst($name).filter(|value| value.is_finite());
                        out.$field = authored.unwrap_or(defaults.$field);
                        readings.push(SettingReading { name: $name, value: out.$field, authored: authored.is_some() });
                    }
                )+
                out.lift_units(kind);
                (out, readings)
            }
        }
    };
}

fn column(kind: StealthKind) -> Column {
    match kind {
        StealthKind::SightOnly | StealthKind::Fallout3 | StealthKind::FalloutNv => Column::Fallout3,
        StealthKind::Oblivion => Column::Oblivion,
        StealthKind::Skyrim => Column::Skyrim,
        StealthKind::Fallout4 => Column::Fallout4,
        StealthKind::Starfield => Column::Starfield,
    }
}

/// One setting the install read: its value, and whether the load order
/// authors it or it fell back to the executable default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SettingReading {
    pub name: &'static str,
    pub value: f32,
    pub authored: bool,
}

// [Fallout3.exe, Oblivion.exe, SkyrimSE.exe, Fallout4.exe, Starfield.exe] defaults, read statically (design
// §13.1). NA where the game has no such setting, or where the row's formula does
// not read it (the `Fallout3.exe` Oblivion leftovers without a reader; Skyrim's
// light and night-eye inputs, applied before its formula runs).
detection_settings! {
    // Range and cadence.
    max_distance: "fSneakMaxDistance" = [1500.0, 1500.0, 1500.0, 4096.0, 64.0],
    exterior_distance_mult: "fSneakExteriorDistanceMult" = [2.0, 2.0, 2.0, 1.25, 1.25],
    attenuation_exponent: "fSneakDistanceAttenuationExponent" = [2.0, NA, 2.0, 2.0, 2.0],
    timer: "fDetectionTimerSetting" = [0.3, 0.3, NA, NA, NA],
    update_time_min: "fDetectionUpdateTimeMin" = [NA, NA, 1.0, 1.0, 1.0],
    update_time_max: "fDetectionUpdateTimeMax" = [NA, NA, 2.0, 2.0, 2.0],
    view_cone: "fDetectionViewCone" = [190.0, NA, 190.0, 190.0, 190.0],
    event_expire_time: "fDetectionEventExpireTime" = [10.0, NA, 5.0, 2.5, 2.5],
    // Base value and thresholds.
    base_value: "fSneakBaseValue" = [-25.0, -25.0, -25.0, NA, NA],
    seen_min: "fSneakSeenMin" = [0.0, 0.0, NA, NA, NA],
    noticed_min: "fSneakNoticedMin" = [-20.0, -20.0, NA, NA, NA],
    ai_combat_min_detection: "iAICombatMinDetection" = [NA, -50.0, NA, NA, NA],
    // Sound.
    sounds_mult: "fSneakSoundsMult" = [1.6, 1.6, 1.6, 1.0, 1.0],
    sound_los_mult: "fSneakSoundLosMult" = [0.25, 0.5, 0.25, 0.3, 0.3],
    boot_weight_base: "fSneakBootWeightBase" = [14.0, 7.0, NA, NA, NA],
    boot_weight_mult: "fSneakBootWeightMult" = [1.0, 1.0, NA, NA, NA],
    equipped_weight_base: "fSneakEquippedWeightBase" = [NA, NA, 12.0, 12.0, 12.0],
    equipped_weight_mult: "fSneakEquippedWeightMult" = [NA, NA, 0.5, 0.5, 0.5],
    running_mult: "fSneakRunningMult" = [2.0, 1.5, 2.0, 2.0, 2.0],
    action_mult: "fSneakActionMult" = [1.0, NA, 1.0, 1.0, 1.0],
    target_in_combat_bonus: "fSneakTargetInCombatBonus" = [NA, 25.0, NA, NA, NA],
    target_attack_bonus: "fSneakTargetAttackBonus" = [NA, 100.0, NA, NA, NA],
    // Sight.
    light_mult: "fSneakLightMult" = [1.4, 1.2, 1.4, 0.33, 0.33],
    light_exterior_mult: "fSneakLightExteriorMult" = [NA, NA, 1.0, 0.5, 0.5],
    light_mod: "fDetectionSneakLightMod" = [0.0, 0.0, 0.0, 15.0, 15.0],
    light_move_mult: "fSneakLightMoveMult" = [0.01, NA, 0.01, 0.2, 0.2],
    light_run_mult: "fSneakLightRunMult" = [0.2, NA, 0.2, 0.4, 0.4],
    light_level_max: "iLightLevelMax" = [NA, NA, 100.0, 300.0, 150.0],
    light_level_interior_mod: "iLightLevelInteriorMod" = [NA, NA, 50.0, 50.0, NA],
    light_level_exterior_mod: "iLightLevelExteriorMod" = [NA, NA, 0.0, 0.0, NA],
    swimming_light_mult: "fSneakSwimmingLightMult" = [NA, 0.5, NA, NA, NA],
    stealth_boy_mult: "fSneakStealthBoyMult" = [0.0, NA, NA, 0.0, 0.0],
    night_eye_bonus: "fDetectionNightEyeBonus" = [3.0, 3.0, NA, 3.0, 3.0],
    large_actor_threshold: "fDetectionLargeActorThreshold" = [135.0, NA, NA, NA, NA],
    large_actor_size_mult: "fDetectionLargeActorSizeMult" = [1.0, NA, 1.0, NA, NA],
    size_small: "fSneakDetectionSizeSmall" = [NA, NA, 0.5, 0.5, 0.5],
    size_normal: "fSneakDetectionSizeNormal" = [NA, NA, 1.0, 1.0, 1.0],
    size_large: "fSneakDetectionSizeLarge" = [NA, NA, 1.5, 1.5, 1.5],
    size_very_large: "fSneakDetectionSizeVeryLarge" = [NA, NA, 2.0, 2.0, 2.0],
    // Detector skill.
    skill_mult: "fSneakSkillMult" = [0.5, 0.75, 0.5, 0.5, 0.5],
    perception_skill_min: "fSneakPerceptionSkillMin" = [10.0, NA, 10.0, 10.0, 10.0],
    perception_skill_max: "fSneakPerceptionSkillMax" = [90.0, NA, 90.0, 90.0, 90.0],
    player_detection_sneak_base: "fPlayerDetectionSneakBase" = [NA, NA, 0.0, NA, NA],
    player_detection_sneak_mult: "fPlayerDetectionSneakMult" = [NA, NA, 1.0, NA, NA],
    alert_mod: "fSneakAlertMod" = [0.2, NA, 0.2, 0.0, 0.0],
    sleep_bonus: "fSneakSleepBonus" = [-0.2, -25.0, -0.5, 0.0, 0.0],
    sleep_mod: "fSneakSleepMod" = [NA, NA, -0.2, 0.0, 0.0],
    combat_mod: "fSneakCombatMod" = [-0.4, NA, -0.4, -0.4, -0.4],
    // Stealth points (Skyrim; `SkyrimSE.exe` `0x140818570`).
    stealth_point_detection_threshold: "iCombatStealthPointDetectionThreshold" = [NA, NA, 25.0, NA, NA],
    stealth_point_sneak_detection_threshold: "iCombatStealthPointSneakDetectionThreshold" = [NA, NA, 70.0, NA, NA],
    stealth_point_max: "fCombatStealthPointMax" = [NA, NA, 100.0, 100.0, 100.0],
    stealth_point_drain_mult: "fCombatStealthPointDrainMult" = [NA, NA, 0.5, NA, NA],
    stealth_point_regen_min: "fCombatStealthPointRegenMin" = [NA, NA, 3.0, 1.0, 2.0],
    stealth_point_regen_mult: "fCombatStealthPointRegenMult" = [NA, NA, 0.2, NA, NA],
    stealth_point_regen_alert_wait: "fCombatStealthPointRegenAlertWaitTime" = [NA, NA, 10.0, NA, NA],
    stealth_point_regen_lost_wait: "fCombatStealthPointRegenLostWaitTime" = [NA, NA, 10.0, NA, NA],
    stealth_point_regen_attacked_wait: "fCombatStealthPointRegenAttackedWaitTime" = [NA, NA, 25.0, NA, NA],
    stealth_point_regen_event_wait: "fCombatStealthPointRegenDetectedEventWaitTime" = [NA, NA, 20.0, NA, NA],
    stealth_point_attacked_max: "fCombatStealthPointAttackedMaxValue" = [NA, NA, 50.0, 65.0, 25.0],
    stealth_point_event_max: "fCombatStealthPointDetectedEventMaxValue" = [NA, NA, 75.0, 80.0, 80.0],
    // Sneak attacks: Fallout 3 (`Fallout3.exe` `0x845580`, `0x845980`).
    sneak_damage_mult: "fCombatDamageBonusSneakingMult" = [1.0, NA, NA, NA, NA],
    melee_sneak_damage_mult: "fCombatDamageBonusMeleeSneakingMult" = [5.0, NA, NA, NA, NA],
    unarmed_crit_damage_mult: "fCombatUnarmedCritDamageMult" = [0.5, NA, NA, NA, NA],
    // Sneak attacks: Oblivion (`Oblivion.exe` `0x5477f0`), by Sneak mastery.
    sneak_melee_novice: "fPerkSneakAttackMeleeNoviceMult" = [NA, 4.0, NA, NA, NA],
    sneak_melee_apprentice: "fPerkSneakAttackMeleeApprenticeMult" = [NA, 6.0, NA, NA, NA],
    sneak_melee_journeyman: "fPerkSneakAttackMeleeJourneymanMult" = [NA, 6.0, NA, NA, NA],
    sneak_melee_expert: "fPerkSneakAttackMeleeExpertMult" = [NA, 6.0, NA, NA, NA],
    sneak_melee_master: "fPerkSneakAttackMeleeMasterMult" = [NA, 6.0, NA, NA, NA],
    sneak_marksman_novice: "fPerkSneakAttackMarksmanNoviceMult" = [NA, 6.0, NA, NA, NA],
    sneak_marksman_apprentice: "fPerkSneakAttackMarksmanApprenticeMult" = [NA, 8.0, NA, NA, NA],
    sneak_marksman_journeyman: "fPerkSneakAttackMarksmanJourneymanMult" = [NA, 8.0, NA, NA, NA],
    sneak_marksman_expert: "fPerkSneakAttackMarksmanExpertMult" = [NA, 8.0, NA, NA, NA],
    sneak_marksman_master: "fPerkSneakAttackMarksmanMasterMult" = [NA, 8.0, NA, NA, NA],
    // Sneak attacks: Skyrim (`SkyrimSE.exe` `0x14028c410`), by animation type 0-9.
    sneak_hand: "fCombatSneakHandMult" = [NA, NA, 1.0, 1.0, 1.0],
    sneak_one_hand_sword: "fCombatSneak1HSwordMult" = [NA, NA, 2.0, 2.0, 2.0],
    sneak_dagger: "fCombatSneak1HDaggerMult" = [NA, NA, 2.0, 2.0, 2.0],
    sneak_one_hand_axe: "fCombatSneak1HAxeMult" = [NA, NA, 2.0, 2.0, 2.0],
    sneak_mace: "fCombatSneak1HMaceMult" = [NA, NA, 2.0, 2.0, 2.0],
    sneak_two_hand_sword: "fCombatSneak2HSwordMult" = [NA, NA, 1.0, 1.0, 1.0],
    sneak_two_hand_axe: "fCombatSneak2HAxeMult" = [NA, NA, 1.0, 1.0, 1.0],
    sneak_bow: "fCombatSneakBowMult" = [NA, NA, 2.0, 2.0, 2.0],
    sneak_staff: "fCombatSneakStaffMult" = [NA, NA, 1.0, 1.0, 1.0],
    sneak_crossbow: "fCombatSneakCrossbowMult" = [NA, NA, 2.0, NA, NA],
    // Fallout 4 (`Fallout4.exe`, #6996): the split sound and visual values (`0x1406c8770`).
    sound_base_value: "fSneakSoundBaseValue" = [NA, NA, NA, -20.0, -20.0],
    visual_base_value: "fSneakVisualBaseValue" = [NA, NA, NA, -20.0, -20.0],
    skill_movement_mult: "fSneakSkillMovementMult" = [NA, NA, NA, 0.25, 0.25],
    agility_skill_min: "fSneakAgilitySkillMin" = [NA, NA, NA, 0.0, 0.0],
    agility_skill_max: "fSneakAgilitySkillMax" = [NA, NA, NA, 50.0, 50.0],
    large_actor_distance_mult: "fSneakLargeActorDistanceMult" = [NA, NA, NA, 1.0, 1.0],
    visual_min_value: "fSneakVisualMinValue" = [NA, NA, NA, 25.0, 25.0],
    visual_min_distance_min: "fSneakVisualMinDistanceMin" = [NA, NA, NA, 150.0, 2.0],
    visual_min_distance_max: "fSneakVisualMinDistanceMax" = [NA, NA, NA, 300.0, 4.0],
    visual_min_angle: "fSneakVisualMinAngle" = [NA, NA, NA, 45.0, 45.0],
    // Fallout 4: the pair distance and the visual range (`0x140cc00c0`).
    distance_height_mult: "fDetectionDistanceHeightMult" = [NA, NA, NA, 0.25, 0.25],
    los_distance_angle: "fDetectionLOSDistanceAngle" = [NA, NA, NA, 45.0, 45.0],
    los_distance_mult_exterior: "fDetectionLOSDistanceMultExterior" = [NA, NA, NA, 0.5, 0.5],
    los_distance_mult_interior: "fDetectionLOSDistanceMultInterior" = [NA, NA, NA, 0.5, 0.5],
    visual_distance_angle: "fDetectionVisualDistanceAngle" = [NA, NA, NA, 30.0, NA],
    visual_distance_mult_exterior: "fDetectionVisualDistanceMultExterior" = [NA, NA, NA, 0.33, 0.2],
    visual_distance_mult_interior: "fDetectionVisualDistanceMultInterior" = [NA, NA, NA, 0.33, 0.2],
    visual_distance_angle_up_min: "fDetectionVisualDistanceAngleUpMin" = [NA, NA, NA, 15.0, 5.0],
    visual_distance_angle_up_max: "fDetectionVisualDistanceAngleUpMax" = [NA, NA, NA, 45.0, 30.0],
    visual_distance_angle_down_min: "fDetectionVisualDistanceAngleDownMin" = [NA, NA, NA, 15.0, 10.0],
    visual_distance_angle_down_max: "fDetectionVisualDistanceAngleDownMax" = [NA, NA, NA, 60.0, 45.0],
    visual_distance_up_mult: "fDetectionVisualDistanceUpMult" = [NA, NA, NA, 0.1, 0.1],
    visual_distance_down_mult: "fDetectionVisualDistanceDownMult" = [NA, NA, NA, 0.1, 0.1],
    visual_cover_mod: "fSneakVisualCoverMod" = [NA, NA, NA, 0.5, 0.5],
    object_visibility_mult: "fDetectionObjectVisibilityMult" = [NA, NA, NA, 0.4, 0.4],
    object_visibility_mult_offset: "fDetectionObjectVisibilityMultOffset" = [NA, NA, NA, 0.15, 0.15],
    object_visibility_mult_min: "fDetectionObjectVisibilityMultMin" = [NA, NA, NA, 0.15, 0.15],
    // Fallout 4: stealth points, split by sense (`0x140e94420`, `0x140e9f170`).
    stealth_point_visual_threshold: "iCombatStealthPointDetectionVisualThreshold" = [NA, NA, NA, 25.0, 25.0],
    stealth_point_sneak_visual_threshold: "iCombatStealthPointSneakDetectionVisualThreshold" = [NA, NA, NA, 70.0, 70.0],
    stealth_point_sound_threshold: "iCombatStealthPointDetectionSoundThreshold" = [NA, NA, NA, 30.0, 30.0],
    stealth_point_sneak_sound_threshold: "iCombatStealthPointSneakDetectionSoundThreshold" = [NA, NA, NA, 40.0, 40.0],
    stealth_point_sound_threshold_radius: "fCombatStealthPointDetectionSoundThresholdRadius" = [NA, NA, NA, 512.0, 8.0],
    stealth_point_drain_sound_mult: "fCombatStealthPointDrainSoundMult" = [NA, NA, NA, 1.5, 1.5],
    stealth_point_drain_visual_mult: "fCombatStealthPointDrainVisualMult" = [NA, NA, NA, 3.0, 3.0],
    stealth_point_sound_floor: "fCombatStealthPointSoundThreshold" = [NA, NA, NA, 20.0, 25.0],
    stealth_point_los_sound_value_threshold: "fCombatStealthPointLOSSoundValueThreshold" = [NA, NA, NA, 50.0, 50.0],
    stealth_point_regen_alert_mult: "fCombatStealthPointRegenAlertMult" = [NA, NA, NA, 20.0, 50.0],
    stealth_point_regen_hostile_mult: "fCombatStealthPointRegenHostileMult" = [NA, NA, NA, 10.0, 15.0],
    stealth_point_regen_lost_mult: "fCombatStealthPointRegenLostMult" = [NA, NA, NA, 10.0, 20.0],
    stealth_point_regen_alert_delay: "fCombatStealthPointRegenAlertDelayTime" = [NA, NA, NA, 10.0, 10.0],
    stealth_point_regen_lost_delay: "fCombatStealthPointRegenLostDelayTime" = [NA, NA, NA, 1.0, 1.0],
    stealth_point_regen_attacked_delay: "fCombatStealthPointRegenAttackedDelayTime" = [NA, NA, NA, 20.0, 20.0],
    stealth_point_regen_event_delay: "fCombatStealthPointRegenDetectedEventDelayTime" = [NA, NA, NA, 12.5, 12.5],
    // Sneak attacks: Fallout 4, animation types 9-11 after Skyrim's 0-8 (`0x140479660`).
    sneak_gun: "fCombatSneakGunMult" = [NA, NA, NA, 2.0, 2.0],
    sneak_grenade: "fCombatSneakGrenadeMult" = [NA, NA, NA, 2.0, 2.0],
    sneak_mine: "fCombatSneakMineMult" = [NA, NA, NA, 2.0, 2.0],
    // Starfield (`Starfield.exe`, #7046; distances in metres): what the pair formula `0x140d7e2a0` reads beyond Fallout 4's.
    max_distance_visual_mult: "fSneakMaxDistanceVisualMult" = [NA, NA, NA, NA, 1.5],
    ambient_sound_mult: "fSneakAmbientSoundMult" = [NA, NA, NA, NA, 1.0],
    sound_level_silent: "iSoundLevelSilent" = [NA, NA, NA, NA, 10.0],
    sound_level_quiet: "iSoundLevelQuiet" = [NA, NA, NA, NA, 25.0],
    sound_level_normal: "iSoundLevelNormal" = [NA, NA, NA, NA, 50.0],
    sound_level_loud: "iSoundLevelLoud" = [NA, NA, NA, NA, 100.0],
    sound_level_very_loud: "iSoundLevelVeryLoud" = [NA, NA, NA, NA, 200.0],
    detection_running_mult: "fDetectionRunningMult" = [NA, NA, NA, NA, 2.0],
    detection_sprinting_mult: "fDetectionSprintingMult" = [NA, NA, NA, NA, 2.0],
    visual_distance_angle_min: "fDetectionVisualDistanceAngleMin" = [NA, NA, NA, NA, 30.0],
    visual_distance_angle_max: "fDetectionVisualDistanceAngleMax" = [NA, NA, NA, NA, 70.0],
}

impl DetectionSettings {
    /// Starfield authors its distances in metres, and its scene data is lifted to Bethesda units at load
    /// (`spatial_units::normalize`), so the distance settings are lifted the same way after they are read; the
    /// readings keep the authored metres. No other game's distances move.
    fn lift_units(&mut self, kind: StealthKind) {
        if kind != StealthKind::Starfield {
            return;
        }
        let units = crate::lighting::BETHESDA_UNITS_PER_METER;
        for distance in [
            &mut self.max_distance,
            &mut self.visual_min_distance_min,
            &mut self.visual_min_distance_max,
            &mut self.stealth_point_sound_threshold_radius,
        ] {
            *distance *= units;
        }
    }

    /// `Fallout3.exe`'s defaults for every setting the Fallout rows read.
    #[must_use]
    pub fn fallout3_defaults() -> Self {
        Self::resolve(StealthKind::Fallout3, |_| None).0
    }
}
