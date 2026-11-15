//! M65 (SNK-STN-005) — where each game's movement speed in the sneak stance
//! comes from (`docs/engine/sneak-detection.md` §2).
//!
//! A profile row names the *source*; the binary resolves it once per load
//! order into two plain multipliers ([`SneakMovement`]) the character
//! controller applies. No consumer branches on the game.
//!
//! * Oblivion, Fallout 3, New Vegas: the `fMoveSneakMult` game setting, with
//!   the executable's default where no master authors it (`Oblivion.exe` 0.75,
//!   `Fallout3.exe` 0.6; the masters author 0.6 and 0.57). `Fallout3.exe`'s
//!   base-speed routine (`0x6017c0`) multiplies by it while sneaking.
//! * Skyrim and Fallout 4: the playable races author no `SNMV`, so the speed
//!   is the ratio of the sneaking movement type's forward speeds to the
//!   default one's (`MOVT` `SPED`): Skyrim `NPC_Sneaking_MT` / `NPC_Default_MT`
//!   (47.2 / 80.1 walking, 222 / 370 running), Fallout 4 `NPC_Sneaking` /
//!   `NPC_Default_MT`. The `SPED` layouts differ (xEdit `wbDefinitionsTES5`:
//!   two floats per direction; `wbDefinitionsFO4`: four, the first and last
//!   unused), so the row carries the forward walk / run indices.
//! * Starfield and Fallout 76: no source read yet, so the stance does not
//!   change the speed ([`SneakSpeed::Unscaled`]).

/// Where a game's speed in the sneak stance comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SneakSpeed {
    /// `fMoveSneakMult`, or `default` (the executable's) when no plugin
    /// authors it. Walking and running take the same multiplier.
    Setting { default: f32 },
    /// The forward speeds of the `sneak` movement type over the `default`
    /// one's, read from `MOVT` `SPED` at the given float indices.
    MovementTypes {
        sneak: &'static str,
        default: &'static str,
        walk_index: usize,
        run_index: usize,
    },
    /// No source: the stance leaves the speed alone.
    Unscaled,
}

/// The per-game stance row of
/// [`CharacterRulesProfile`](super::CharacterRulesProfile).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StanceModel {
    pub sneak_speed: SneakSpeed,
}

impl StanceModel {
    pub const NONE: Self = Self { sneak_speed: SneakSpeed::Unscaled };
    /// `Oblivion.exe`'s `fMoveSneakMult` default (its initializer pushes 0.75).
    pub const OBLIVION: Self = Self { sneak_speed: SneakSpeed::Setting { default: 0.75 } };
    /// `Fallout3.exe`'s `fMoveSneakMult` default; New Vegas shares it (its
    /// executable is encrypted, and `FalloutNV.esm` authors the value anyway).
    pub const FALLOUT: Self = Self { sneak_speed: SneakSpeed::Setting { default: 0.6 } };
    pub const SKYRIM: Self = Self {
        sneak_speed: SneakSpeed::MovementTypes {
            sneak: "NPC_Sneaking_MT",
            default: "NPC_Default_MT",
            walk_index: 4,
            run_index: 5,
        },
    };
    pub const FALLOUT4: Self = Self {
        sneak_speed: SneakSpeed::MovementTypes {
            sneak: "NPC_Sneaking",
            default: "NPC_Default_MT",
            walk_index: 9,
            run_index: 10,
        },
    };

    /// Resolve the row against a load order: `gmst` reads a numeric game
    /// setting, `speeds` a movement type's `SPED` floats by EditorID. A
    /// missing record or a non-positive default speed leaves the speed
    /// unscaled rather than inventing one; [`SneakMovement::source`] says
    /// which happened, for the install log.
    #[must_use]
    pub fn resolve(
        self,
        gmst: impl Fn(&str) -> Option<f32>,
        speeds: impl Fn(&str) -> Option<Vec<f32>>,
    ) -> SneakMovement {
        match self.sneak_speed {
            SneakSpeed::Setting { default } => {
                let (mult, source) = match gmst("fMoveSneakMult") {
                    Some(value) => (value, SneakSpeedSource::Authored),
                    None => (default, SneakSpeedSource::ExecutableDefault),
                };
                SneakMovement { walk_mult: mult, run_mult: mult, source }
            }
            SneakSpeed::MovementTypes { sneak, default, walk_index, run_index } => {
                let ratio = |sneaking: &[f32], standing: &[f32], at: usize| {
                    let (s, d) = (*sneaking.get(at)?, *standing.get(at)?);
                    (d > 0.0 && s.is_finite()).then_some(s / d)
                };
                let resolved = speeds(sneak).zip(speeds(default)).and_then(|(s, d)| {
                    Some((ratio(&s, &d, walk_index)?, ratio(&s, &d, run_index)?))
                });
                match resolved {
                    Some((walk_mult, run_mult)) => SneakMovement {
                        walk_mult,
                        run_mult,
                        source: SneakSpeedSource::MovementTypes,
                    },
                    None => SneakMovement::UNSCALED_MISSING,
                }
            }
            SneakSpeed::Unscaled => SneakMovement::UNSCALED,
        }
    }
}

/// Where a resolved [`SneakMovement`] came from, for the install log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SneakSpeedSource {
    /// A plugin authors `fMoveSneakMult`.
    Authored,
    /// No plugin authors it: the executable's default.
    ExecutableDefault,
    /// The two movement types' `SPED`.
    MovementTypes,
    /// The row names movement types the load order lacks.
    MissingMovementTypes,
    /// The row has no source.
    Unsourced,
}

/// The resolved stance multipliers the character controller applies on top of
/// its walk and sprint speeds. Installed as a resource; absent means unscaled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SneakMovement {
    pub walk_mult: f32,
    pub run_mult: f32,
    pub source: SneakSpeedSource,
}

impl SneakMovement {
    pub const UNSCALED: Self = Self { walk_mult: 1.0, run_mult: 1.0, source: SneakSpeedSource::Unsourced };
    const UNSCALED_MISSING: Self =
        Self { walk_mult: 1.0, run_mult: 1.0, source: SneakSpeedSource::MissingMovementTypes };

    /// The multiplier for a player moving in the stance, running or not.
    #[must_use]
    pub fn mult(&self, running: bool) -> f32 {
        if running { self.run_mult } else { self.walk_mult }
    }
}

impl crate::ecs::resource::Resource for SneakMovement {}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Skyrim.esm`'s `SPED` for the two types (§13.3 of the design, V2).
    fn skyrim_speeds(editor_id: &str) -> Option<Vec<f32>> {
        match editor_id {
            "NPC_Sneaking_MT" => Some(vec![41.44, 200.0, 41.44, 200.0, 47.2, 222.0, 43.38, 150.0, 1.57, 3.14, 3.14]),
            "NPC_Default_MT" => Some(vec![80.1, 370.0, 79.8, 370.0, 80.1, 370.0, 71.9, 205.2, 1.57, 3.14, 3.14]),
            _ => None,
        }
    }

    #[test]
    fn the_fallout_row_reads_the_authored_setting_then_the_executable_default() {
        let authored = StanceModel::FALLOUT.resolve(|name| (name == "fMoveSneakMult").then_some(0.57), |_| None);
        assert_eq!((authored.walk_mult, authored.run_mult), (0.57, 0.57));
        assert_eq!(authored.source, SneakSpeedSource::Authored);
        let unauthored = StanceModel::FALLOUT.resolve(|_| None, |_| None);
        assert_eq!(unauthored.walk_mult, 0.6);
        assert_eq!(unauthored.source, SneakSpeedSource::ExecutableDefault);
        assert_eq!(StanceModel::OBLIVION.resolve(|_| None, |_| None).walk_mult, 0.75);
    }

    #[test]
    fn the_skyrim_row_is_the_ratio_of_the_two_movement_types() {
        let movement = StanceModel::SKYRIM.resolve(|_| None, skyrim_speeds);
        assert!((movement.walk_mult - 47.2 / 80.1).abs() < 1e-6);
        assert!((movement.run_mult - 222.0 / 370.0).abs() < 1e-6);
        assert_eq!(movement.source, SneakSpeedSource::MovementTypes);
    }

    #[test]
    fn a_missing_movement_type_leaves_the_speed_unscaled() {
        let movement = StanceModel::SKYRIM.resolve(|_| None, |_| None);
        assert_eq!((movement.walk_mult, movement.run_mult), (1.0, 1.0));
        assert_eq!(movement.source, SneakSpeedSource::MissingMovementTypes);
        assert_eq!(StanceModel::NONE.resolve(|_| Some(0.5), skyrim_speeds), SneakMovement::UNSCALED);
    }
}
