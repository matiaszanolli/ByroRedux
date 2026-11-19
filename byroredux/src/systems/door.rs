//! In-cell doors: the motion half of the Door Runtime (PRD door-runtime, M66).
//!
//! The state and its toggle are the scripting crate's: [`two_state_activator_system`] flips
//! [`TwoStateActivator::is_open`] on an `ActivateEvent` and publishes a [`TwoStateTransitionBatch`], and
//! `set_two_state_open` does the same for a script's `SetOpen`. This system is the batch's consumer: it plays the
//! door model's `Open` or `Close` clip on the transition, keeps `is_animating` set while the clip runs (so the
//! prompt hides and a second activation is refused, DRT-ACT-001), and parks the pose when the door is at rest.
//!
//! It is registered *before* `two_state_activator_system`, which drains the batch at the head of each run: a batch
//! published in frame N is played in frame N + 1. The door's collider rides the animated node (design §3), so
//! nothing here touches physics.

use crate::components::{Door, DoorMotion};
use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::{EntityId, World};
use byroredux_scripting::{TwoStateActivator, TwoStateTransition, TwoStateTransitionBatch};

/// The clip that shows `open` and the clip that moves into it: a door heading open plays `Open`, heading closed `Close`.
fn clip_for(motion: DoorMotion, open: bool) -> u32 {
    match motion {
        DoorMotion::Sequence { open: open_clip, close: close_clip } => {
            if open {
                open_clip
            } else {
                close_clip
            }
        }
    }
}

fn clip_duration(world: &World, handle: u32) -> f32 {
    world.resource::<AnimationClipRegistry>().get(handle).map_or(0.0, |clip| clip.duration)
}

/// Point `entity`'s animation player at `handle`: from the start and playing, or parked on its last frame.
fn set_player(world: &World, entity: EntityId, handle: u32, duration: f32, playing: bool) {
    if let Some(mut players) = world.query_mut::<AnimationPlayer>()
        && let Some(player) = players.get_mut(entity)
    {
        let time = if playing { 0.0 } else { duration };
        player.clip_handle = handle;
        player.local_time = time;
        player.prev_time = time;
        player.playing = playing;
        player.speed = 1.0;
        player.reverse_direction = false;
    }
}

pub(crate) fn door_motion_system(world: &World, dt: f32) {
    // The transitions published since the last run, as (door, opened).
    let mut starts: Vec<(EntityId, bool)> = Vec::new();
    if let Some(batches) = world.query::<TwoStateTransitionBatch>() {
        let doors = world.query::<Door>();
        for (entity, batch) in batches.iter() {
            if doors.as_ref().is_some_and(|doors| doors.contains(entity))
                && let Some(last) = batch.0.last()
            {
                starts.push((entity, matches!(last, TwoStateTransition::Opened)));
            }
        }
    }

    // Begin each clip: the player, the door's bookkeeping, then `is_animating`.
    for (entity, open) in starts {
        let Some(motion) = world.get::<Door>(entity).map(|door| door.motion) else { continue };
        let handle = clip_for(motion, open);
        let duration = clip_duration(world, handle);
        log::debug!("door {entity}: {} (clip {handle}, {duration:.2}s)", if open { "opening" } else { "closing" });
        set_player(world, entity, handle, duration, true);
        if let Some(mut doors) = world.query_mut::<Door>()
            && let Some(door) = doors.get_mut(entity)
        {
            door.remaining = duration;
            door.posed = Some(open);
        }
        if let Some(mut states) = world.query_mut::<TwoStateActivator>()
            && let Some(state) = states.get_mut(entity)
        {
            state.is_animating = duration > 0.0;
        }
    }

    // Advance the playing clips and settle the doors whose clip has ended or whose pose is stale.
    let mut settled: Vec<(EntityId, bool, DoorMotion)> = Vec::new();
    let mut stale: Vec<(EntityId, bool, DoorMotion)> = Vec::new();
    if let Some(mut doors) = world.query_mut::<Door>() {
        let states = world.query::<TwoStateActivator>();
        for (entity, door) in doors.iter_mut() {
            let Some(state) = states.as_ref().and_then(|states| states.get(entity)).copied() else { continue };
            if door.remaining > 0.0 {
                door.remaining -= dt;
                if door.remaining <= 0.0 {
                    door.remaining = 0.0;
                    settled.push((entity, state.is_open, door.motion));
                }
            } else if door.posed != Some(state.is_open) {
                door.posed = Some(state.is_open);
                stale.push((entity, state.is_open, door.motion));
            }
        }
    }
    for (entity, open, motion) in settled {
        let handle = clip_for(motion, open);
        set_player(world, entity, handle, clip_duration(world, handle), false);
        if let Some(mut states) = world.query_mut::<TwoStateActivator>()
            && let Some(state) = states.get_mut(entity)
        {
            state.is_animating = false;
        }
    }
    // A door at rest in a state its pose does not show: its pose is the `Open` clip's last frame when open and its first
    // frame when closed.
    for (entity, open, motion) in stale {
        let DoorMotion::Sequence { open: open_clip, .. } = motion;
        let duration = clip_duration(world, open_clip);
        set_player(world, entity, open_clip, duration, false);
        if !open {
            if let Some(mut players) = world.query_mut::<AnimationPlayer>()
                && let Some(player) = players.get_mut(entity)
            {
                player.local_time = 0.0;
                player.prev_time = 0.0;
            }
        }
    }
}

/// Give a placed in-cell door its runtime (DRT-STATE-001): the open state, set from `Open by Default`, and the
/// [`Door`] the motion system reads. Only a door whose model authors `Open` and `Close` sequences gets one so far;
/// the doors that author nothing (ladders, hatches, door frames and a few hinged doors, design §1) wait on the swing.
/// `root` is the placement root, which already carries the model's `NamedSequences`.
pub(crate) fn attach_door(
    world: &mut World,
    root: EntityId,
    data: byroredux_plugin::esm::cell::DoorData,
    game: byroredux_plugin::esm::reader::GameKind,
    default_open: bool,
) -> bool {
    let Some(sequences) = world.get::<byroredux_core::animation::NamedSequences>(root).map(|s| s.clone()) else {
        return false;
    };
    let (Some(open), Some(close)) = (sequences.handle("open"), sequences.handle("close")) else {
        return false;
    };
    world.insert(
        root,
        TwoStateActivator { is_open: default_open, is_animating: false, do_once: false, activated_once: false },
    );
    world.insert(
        root,
        Door {
            motion: DoorMotion::Sequence { open, close },
            data,
            flags: data.flags(game),
            default_open,
            remaining: 0.0,
            posed: None,
        },
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::animation::{AnimationClip, CycleType};

    fn clip(name: &str, duration: f32) -> AnimationClip {
        AnimationClip {
            name: name.into(),
            duration,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: Default::default(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
            root_yaw: Vec::new(),
        }
    }

    /// A closed door with `Open` (1.0 s) and `Close` (0.8 s) clips, its player parked on the first clip.
    fn door_world(is_open: bool) -> (World, EntityId, u32, u32) {
        let mut world = World::new();
        world.register::<Door>();
        world.register::<AnimationPlayer>();
        byroredux_scripting::register(&mut world);
        let mut registry = AnimationClipRegistry::new();
        let (open, close) = (registry.add(clip("Open", 1.0)), registry.add(clip("Close", 0.8)));
        world.insert_resource(registry);
        let door = world.spawn();
        world.insert(door, TwoStateActivator { is_open, ..Default::default() });
        world.insert(
            door,
            Door {
                motion: DoorMotion::Sequence { open, close },
                data: Default::default(),
                flags: Default::default(),
                default_open: false,
                remaining: 0.0,
                posed: None,
            },
        );
        let mut player = AnimationPlayer::new(open);
        player.playing = false;
        world.insert(door, player);
        (world, door, open, close)
    }

    /// What `two_state_activator_system` does on an activation: flip the state and publish the transition.
    fn publish(world: &World, door: EntityId, transition: TwoStateTransition) {
        world.query_mut::<TwoStateActivator>().unwrap().get_mut(door).unwrap().is_open = transition == TwoStateTransition::Opened;
        world.query_mut::<TwoStateTransitionBatch>().unwrap().insert(door, TwoStateTransitionBatch(vec![transition]));
    }

    /// One frame: the motion system, then the head of the next run of `two_state_activator_system`, which drains the batch.
    fn tick(world: &World, dt: f32) {
        door_motion_system(world, dt);
        byroredux_scripting::two_state_activator_system(world, 0.0);
    }

    fn player_of(world: &World, door: EntityId) -> (u32, f32, bool) {
        let player = world.get::<AnimationPlayer>(door).unwrap();
        (player.clip_handle, player.local_time, player.playing)
    }

    /// DRT-MOVE-001 — an opening plays the `Open` clip from its start, `is_animating` holds while it runs, and the
    /// door settles on the clip's last frame; closing does the same with `Close`.
    #[test]
    fn a_transition_plays_the_matching_clip_and_settles_on_its_last_frame() {
        let (world, door, open, close) = door_world(true);
        tick(&world, 0.0); // the first tick poses the door
        publish(&world, door, TwoStateTransition::Closed);
        tick(&world, 0.0);
        assert_eq!(player_of(&world, door), (close, 0.0, true), "a closing plays `Close` from the start");
        assert!(world.get::<TwoStateActivator>(door).unwrap().is_animating);

        tick(&world, 0.5);
        assert!(world.get::<TwoStateActivator>(door).unwrap().is_animating, "still running half way");
        tick(&world, 0.5);
        assert!(!world.get::<TwoStateActivator>(door).unwrap().is_animating, "the clip has ended");
        let (handle, time, playing) = player_of(&world, door);
        assert_eq!((handle, playing), (close, false));
        assert!((time - 0.8).abs() < 1e-5, "parked on the last frame, at {time}");

        publish(&world, door, TwoStateTransition::Opened);
        tick(&world, 0.0);
        assert_eq!(player_of(&world, door), (open, 0.0, true), "an opening plays `Open`");
    }

    /// DRT-SAVE-001 — a door at rest whose pose does not match its state (restored from a save, set by a script)
    /// snaps: open shows the `Open` clip's last frame, closed its first, with no animation.
    #[test]
    fn a_door_at_rest_in_a_state_its_pose_does_not_show_snaps() {
        let (world, door, open, _) = door_world(true);
        tick(&world, 0.016);
        let (handle, time, playing) = player_of(&world, door);
        assert_eq!((handle, playing), (open, false));
        assert!((time - 1.0).abs() < 1e-5);
        assert!(!world.get::<TwoStateActivator>(door).unwrap().is_animating);

        let (world, door, open, _) = door_world(false);
        tick(&world, 0.016);
        assert_eq!(player_of(&world, door), (open, 0.0, false));
    }

    /// A transition on an entity with no `Door` is not this system's.
    #[test]
    fn a_non_door_transition_is_left_alone() {
        let (mut world, door, open, _) = door_world(false);
        let lever = world.spawn();
        world.insert(lever, TwoStateActivator::default());
        publish(&world, lever, TwoStateTransition::Opened);
        tick(&world, 0.0);
        assert!(!world.get::<TwoStateActivator>(lever).unwrap().is_animating);
        assert_eq!(player_of(&world, door).0, open);
    }
}
