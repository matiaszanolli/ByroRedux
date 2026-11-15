//! M65 (SNK-STN-001…005, SNK-DX-002) — the player's sneak stance on the
//! engine side (`docs/engine/sneak-detection.md` §2).
//!
//! The stance is the `Sneaking` marker; `byroredux_scripting::stance` owns the
//! gate (`DisablePlayerControls` / `Control::Sneaking`) and the write. This
//! module adds what only the binary knows (a player body to crouch, a player
//! able to act), the Left Ctrl edge, and the per-load speed multipliers.
//!
//! Runs in `Stage::Early` after `player_controller_system`, like VATS, so this
//! frame's `ActionState` edges are fresh; the controller reads the stance on
//! the next frame. The same system keeps the player body's locomotion clips on
//! the stance's (SNK-STN-008), whichever path changed it.

use crate::components::WalkAnimation;
use crate::interaction::{ActionState, InputAction};
use crate::systems::{player_can_act, PlayerEntity, PlayerMode};
use byroredux_core::animation::AnimationPlayer;
use byroredux_core::character::SneakMovement;
use byroredux_core::ecs::{Component, SparseSetStorage, World};
use byroredux_plugin::esm::records::EsmIndex;

/// Toggle the stance on the Sneak key's press edge (SNK-STN-001), then put the
/// body's locomotion clips on the stance's (SNK-STN-008).
pub(crate) fn sneak_stance_system(world: &World, _dt: f32) {
    let pressed = world
        .try_resource::<ActionState>()
        .is_some_and(|actions| actions.was_pressed(InputAction::Sneak));
    if pressed {
        if let Err(reason) = set_player_sneaking(world, None) {
            log::debug!("sneak key refused: {reason}");
        }
    }
    sync_stance_locomotion(world);
}

/// M65 (SNK-STN-008) — the player body's standing and sneak locomotion clips
/// (`npc_spawn::sneak_locomotion_paths`), attached with its walk animation.
/// `NOT_SAVED_BY_DESIGN`: rebuilt with the body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StanceLocomotion {
    pub(crate) standing_walk: u32,
    pub(crate) standing_idle: Option<u32>,
    pub(crate) sneak_walk: Option<u32>,
    pub(crate) sneak_idle: Option<u32>,
    /// The stance the clips were last put on.
    pub(crate) sneaking: bool,
}

impl Component for StanceLocomotion {
    type Storage = SparseSetStorage<Self>;
}

impl StanceLocomotion {
    /// `(walk, idle)` for a stance; a sneak clip the game lacks keeps the standing one.
    fn clips(&self, sneaking: bool) -> (u32, Option<u32>) {
        if sneaking {
            (self.sneak_walk.unwrap_or(self.standing_walk), self.sneak_idle.or(self.standing_idle))
        } else {
            (self.standing_walk, self.standing_idle)
        }
    }
}

/// Put the player body on the stance's walk and idle clips when the stance
/// changed. The walk system's take / restore stays consistent: the walk handle
/// it compares against, the playing clip, and the idle it will restore all move
/// together.
fn sync_stance_locomotion(world: &World) {
    let Some(player) = PlayerEntity::get(world) else {
        return;
    };
    let sneaking = byroredux_scripting::stance::is_sneaking(world, player);
    let (from, to) = {
        let Some(mut stances) = world.query_mut::<StanceLocomotion>() else {
            return;
        };
        let Some(stance) = stances.get_mut(player) else {
            return;
        };
        if stance.sneaking == sneaking {
            return;
        }
        stance.sneaking = sneaking;
        (stance.clips(!sneaking), stance.clips(sneaking))
    };
    let walking = {
        let Some(mut walks) = world.query_mut::<WalkAnimation>() else {
            return;
        };
        let Some(walk) = walks.get_mut(player) else {
            return;
        };
        walk.walk_handle = to.0;
        if let (Some(snapshot), Some(idle)) = (walk.captured.as_mut(), to.1) {
            if Some(snapshot.clip_handle) == from.1 {
                snapshot.clip_handle = idle;
            }
        }
        walk.walking
    };
    let Some(mut players) = world.query_mut::<AnimationPlayer>() else {
        return;
    };
    if let Some(animation) = players.get_mut(player) {
        let (old, new) = if walking { (Some(from.0), Some(to.0)) } else { (from.1, to.1) };
        if let Some(new) = new.filter(|_| old == Some(animation.clip_handle)) {
            animation.clip_handle = new;
            animation.local_time = 0.0;
            animation.prev_time = 0.0;
        }
    }
}

/// Put the player in the stance (`Some(true)`), take them out (`Some(false)`)
/// or flip it (`None`), and return whether they are sneaking afterwards. The
/// one path the key and `stealth.sneak` share (SNK-DX-002); Papyrus
/// `StartSneaking` reaches the same scripting gate without the body checks.
///
/// Entering needs a player body (not the fly camera) and a player able to act
/// (alive, not in VATS), then the scripting gate. Leaving is never refused.
pub(crate) fn set_player_sneaking(world: &World, sneak: Option<bool>) -> Result<bool, &'static str> {
    let Some(player) = PlayerEntity::get(world) else {
        return Err("no player");
    };
    let sneak = sneak.unwrap_or_else(|| !byroredux_scripting::stance::is_sneaking(world, player));
    if sneak {
        if world
            .try_resource::<PlayerMode>()
            .is_none_or(|mode| *mode != PlayerMode::Character)
        {
            return Err("no player body (fly-cam)");
        }
        if !player_can_act(world) {
            return Err("player cannot act");
        }
    }
    byroredux_scripting::stance::set_sneaking(world, player, sneak).map_err(|refusal| refusal.reason())
}

/// Install the load order's stance speed (SNK-STN-005): the profile row's
/// source resolved against the game settings and movement types, logged with
/// where it came from (SNK-DX-003's install line for the stance).
pub(crate) fn install_sneak_movement(world: &mut World, index: &EsmIndex) {
    let movement = index.character_rules.stance().resolve(
        |editor_id| index.game_setting_float(editor_id),
        |editor_id| index.movement_type_speeds(editor_id),
    );
    log::info!(
        "Sneak stance speed: walk ×{:.3}, run ×{:.3} ({:?})",
        movement.walk_mult,
        movement.run_mult,
        movement.source
    );
    world.insert_resource(movement);
}

/// The stance's factor on the controller's speed: 1 when standing; the walk or
/// run multiplier when sneaking, by whether Sprint is held. On every row Sprint
/// works in the stance and moves at the running speed (SNK-STN-006, on FO3 /
/// FNV provisional until V3, #6935); the detection value reads the same
/// Sprint state as running (Phase 2).
pub(crate) fn stance_speed_mult(sneaking: bool, running: bool, movement: Option<&SneakMovement>) -> f32 {
    if !sneaking {
        return 1.0;
    }
    movement.map_or(1.0, |movement| movement.mult(running))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::InputState;
    use crate::interaction::{refresh_action_state, ActionBindings};
    use byroredux_core::character::{CharacterRulesProfile, SneakSpeedSource};
    use byroredux_core::ecs::components::{Dead, Sneaking};
    use byroredux_plugin::esm::records::{GameSetting, MovementType, SettingValue};
    use byroredux_scripting::input_layers::{Control, InputLayers};
    use winit::keyboard::KeyCode;

    fn world() -> (World, byroredux_core::ecs::EntityId) {
        let mut world = World::new();
        world.insert_resource(InputState::default());
        world.insert_resource(ActionBindings::default());
        world.insert_resource(ActionState::default());
        world.insert_resource(PlayerMode::Character);
        byroredux_scripting::stance::register(&mut world);
        world.register::<Dead>();
        let player = world.spawn();
        world.insert_resource(PlayerEntity(Some(player)));
        (world, player)
    }

    fn tap_sneak(world: &World) {
        world.resource_mut::<InputState>().keys_held.insert(KeyCode::ControlLeft);
        refresh_action_state(world);
        sneak_stance_system(world, 0.0);
        world.resource_mut::<InputState>().keys_held.clear();
        refresh_action_state(world);
        sneak_stance_system(world, 0.0);
    }

    /// SNK-STN-008: entering the stance puts the standing body on the sneak idle and the walk system on the
    /// sneak walk; mid-stride it swaps the playing walk and the idle the walk will restore; leaving goes back.
    #[test]
    fn the_stance_swaps_the_body_onto_the_sneak_clips() {
        let (mut world, player) = world();
        world.insert(
            player,
            StanceLocomotion { standing_walk: 1, standing_idle: Some(2), sneak_walk: Some(3), sneak_idle: Some(4), sneaking: false },
        );
        world.insert(
            player,
            WalkAnimation { walk_handle: 1, walking: false, last_pos: Default::default(), captured: None, transition_secs: 0.0 },
        );
        world.insert(player, AnimationPlayer::new(2));
        let clip = |world: &World| world.get::<AnimationPlayer>(player).unwrap().clip_handle;
        let walk = |world: &World| world.get::<WalkAnimation>(player).unwrap().walk_handle;

        tap_sneak(&world);
        assert_eq!((clip(&world), walk(&world)), (4, 3), "standing: the sneak idle, the sneak walk armed");

        // Mid-stride, the walk system holds the idle it will restore.
        {
            let mut walks = world.query_mut::<WalkAnimation>().unwrap();
            let state = walks.get_mut(player).unwrap();
            state.walking = true;
            state.captured = Some(crate::components::WalkAnimSnapshot {
                clip_handle: 4,
                local_time: 0.0,
                prev_time: 0.0,
                speed: 1.0,
                playing: true,
            });
        }
        world.query_mut::<AnimationPlayer>().unwrap().get_mut(player).unwrap().clip_handle = 3;
        tap_sneak(&world);
        assert_eq!((clip(&world), walk(&world)), (1, 1), "standing up mid-stride: the standing walk");
        let restores = world.get::<WalkAnimation>(player).unwrap().captured.unwrap().clip_handle;
        assert_eq!(restores, 2, "and the standing idle comes back when the stride ends");
    }

    /// PRD §7.2 scenario 1: the action toggles the stance, and `IsSneaking`'s
    /// source reads 1 then 0.
    #[test]
    fn left_ctrl_toggles_the_stance() {
        let (world, player) = world();
        tap_sneak(&world);
        assert!(world.has::<Sneaking>(player));
        assert!(byroredux_scripting::stance::is_sneaking(&world, player));
        tap_sneak(&world);
        assert!(!world.has::<Sneaking>(player), "a second press stands up");
    }

    /// Scenario 1: a disabled `Control::Sneaking` layer refuses the stance; so
    /// do the fly camera and a dead player. Leaving is never refused.
    #[test]
    fn a_disabled_layer_the_fly_camera_and_death_refuse_entering() {
        let (mut world, player) = world();
        let mut layers = InputLayers::default();
        let layer = layers.create();
        layers.set_enabled(layer, Control::Sneaking, false);
        world.insert_resource(layers);
        tap_sneak(&world);
        assert!(!world.has::<Sneaking>(player), "the layer refuses");
        world.remove_resource::<InputLayers>();

        world.insert_resource(PlayerMode::FlyCam);
        assert_eq!(set_player_sneaking(&world, Some(true)), Err("no player body (fly-cam)"));
        world.insert_resource(PlayerMode::Character);
        world.insert(player, Dead);
        assert_eq!(set_player_sneaking(&world, Some(true)), Err("player cannot act"));
        world.remove::<Dead>(player);

        assert_eq!(set_player_sneaking(&world, Some(true)), Ok(true));
        world.insert_resource(PlayerMode::FlyCam);
        assert_eq!(set_player_sneaking(&world, Some(false)), Ok(false), "leaving is never refused");
    }

    fn index(profile: CharacterRulesProfile) -> EsmIndex {
        EsmIndex { character_rules: profile, ..EsmIndex::default() }
    }

    /// SNK-STN-005, -006 and scenario 1's FO3 / FNV rows: the load order's
    /// `fMoveSneakMult` (else the executable default) scales walking, and
    /// Sprint in the stance moves at the running speed.
    #[test]
    fn fallout_rows_scale_by_the_setting_and_sprint_runs_in_the_stance() {
        for profile in [CharacterRulesProfile::FALLOUT3, CharacterRulesProfile::FALLOUT_NEW_VEGAS] {
            let mut authored = index(profile);
            authored.insert_game_setting(GameSetting {
                form_id: 0x0001_0000,
                editor_id: "fMoveSneakMult".into(),
                value: SettingValue::Float(0.57),
            });
            let (mut world, _) = world();
            install_sneak_movement(&mut world, &authored);
            let movement = *world.resource::<SneakMovement>();
            assert_eq!(movement.source, SneakSpeedSource::Authored);
            assert_eq!(stance_speed_mult(true, false, Some(&movement)), 0.57);
            assert_eq!(stance_speed_mult(true, true, Some(&movement)), 0.57, "Sprint runs in the stance");
            assert_eq!(stance_speed_mult(false, true, Some(&movement)), 1.0, "standing is unscaled");

            install_sneak_movement(&mut world, &index(profile));
            assert_eq!(world.resource::<SneakMovement>().walk_mult, 0.6, "Fallout3.exe's default");
        }
    }

    /// SNK-STN-005 (Skyrim): the ratio of the two movement types' forward speeds.
    #[test]
    fn the_skyrim_row_reads_the_movement_types() {
        let mut skyrim = index(CharacterRulesProfile::SKYRIM);
        for (form_id, editor_id, speeds) in [
            (0x0003_580B, "NPC_Sneaking_MT", vec![41.44, 200.0, 41.44, 200.0, 47.2, 222.0]),
            (0x0003_580D, "NPC_Default_MT", vec![80.1, 370.0, 79.8, 370.0, 80.1, 370.0]),
        ] {
            skyrim.movement_types.insert(
                form_id,
                MovementType { form_id, editor_id: editor_id.into(), name: String::new(), speeds },
            );
        }
        let (mut world, _) = world();
        install_sneak_movement(&mut world, &skyrim);
        let movement = *world.resource::<SneakMovement>();
        assert!((stance_speed_mult(true, false, Some(&movement)) - 47.2 / 80.1).abs() < 1e-6);
        assert!((stance_speed_mult(true, true, Some(&movement)) - 222.0 / 370.0).abs() < 1e-6);
    }
}
