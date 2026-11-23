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

/// The sound a door plays as it starts moving: `SNAM` when it starts opening, `ANAM` when it starts closing (DRT-SND-001). The loop
/// sound (`BNAM`) is not played: when it plays is unsourced (#7062), so a one-shot of it would be a guess.
fn door_sound(data: &byroredux_plugin::esm::cell::DoorData, opening: bool) -> Option<u32> {
    if opening {
        data.open_sound
    } else {
        data.close_sound
    }
}

/// Play `sound` (a `SOUN` or `SNDR` form) at the door, through the M44 audio path.
fn play_door_sound(world: &World, door: EntityId, sound: u32) {
    let Some(index) = world.try_resource::<crate::components::LoadedCellIndex>().map(|index| index.0.clone()) else {
        return;
    };
    let Some(record) = index.sounds.get(&sound) else {
        log::debug!("door {door}: {sound:08X} is not a sound record");
        return;
    };
    let Some(position) = world.get::<byroredux_core::ecs::components::GlobalTransform>(door).map(|transform| transform.translation) else {
        return;
    };
    crate::systems::scripted_sound::play_sound_record(world, record, position, !record.non_positional, "door sound");
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

/// The door's verdict on each activation aimed at it (DRT-ACT-003, -004), set before `two_state_activator_system` reads it.
/// Refused: the door is still `Locked` and the activator carries no key (the player's own lock, key or pick, is settled where
/// the activation is made and has taken the lock off by now), or a script took the default processing over
/// (`default_activation_blocked`, which a native activation is exempt from). Admitted: otherwise, even for a native activation,
/// whose default processing for a door is the toggle. A refusal leaves the event for the scripts: their `OnActivate` still runs.
fn door_activation_verdicts(world: &World) {
    let aimed: Vec<(EntityId, EntityId)> = {
        let (Some(events), Some(doors)) = (world.query::<byroredux_scripting::ActivateEvent>(), world.query::<Door>()) else {
            return;
        };
        events.iter().filter(|(entity, _)| doors.contains(*entity)).map(|(entity, event)| (entity, event.activator)).collect()
    };
    for (door, activator) in aimed {
        let locked_out = world.get::<byroredux_core::ecs::components::Locked>(door).is_some()
            && crate::interaction::key_held_by(world, door, activator).is_none();
        let verdict = if locked_out || byroredux_scripting::default_activation_blocked(world, door) {
            byroredux_scripting::DefaultActivation::Refused
        } else {
            byroredux_scripting::DefaultActivation::Admitted
        };
        if let Some(mut verdicts) = world.query_mut::<byroredux_scripting::DefaultActivation>() {
            verdicts.insert(door, verdict);
        }
    }
}

pub(crate) fn door_motion_system(world: &World, dt: f32) {
    door_activation_verdicts(world);
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
        // DRT-SND-001: the open sound as the door starts opening and the close sound as it starts closing, at the door.
        if let Some(sound) = world.get::<Door>(entity).and_then(|door| door_sound(&door.data, open)) {
            play_door_sound(world, entity, sound);
        }
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
    // Doors with no clip left to play that still say they are animating: a door saved, or parked by an unload, mid-swing
    // restores the saved flag over a fresh `Door` whose clip time is 0 (#7226). `is_animating` holds only while a clip plays.
    let mut idle: Vec<EntityId> = Vec::new();
    if let Some((states, mut doors)) = world.query_2_mut::<TwoStateActivator, Door>() {
        for (entity, door) in doors.iter_mut() {
            let Some(state) = states.get(entity) else { continue };
            if door.remaining > 0.0 {
                door.remaining -= dt;
                if door.remaining <= 0.0 {
                    door.remaining = 0.0;
                    settled.push((entity, state.is_open, door.motion));
                }
                continue;
            }
            if state.is_animating {
                idle.push(entity);
            }
            if door.posed != Some(state.is_open) {
                door.posed = Some(state.is_open);
                stale.push((entity, state.is_open, door.motion));
            }
        }
    }
    for (entity, open, motion) in settled {
        let handle = clip_for(motion, open);
        set_player(world, entity, handle, clip_duration(world, handle), false);
        idle.push(entity);
    }
    if !idle.is_empty()
        && let Some(mut states) = world.query_mut::<TwoStateActivator>()
    {
        for entity in idle {
            if let Some(state) = states.get_mut(entity) {
                state.is_animating = false;
            }
        }
    }
    // A door at rest in a state its pose does not show: its pose is the `Open` clip's last frame when open and its first
    // frame when closed.
    for (entity, open, motion) in stale {
        let DoorMotion::Sequence { open: open_clip, .. } = motion;
        let pose_time = if open { clip_duration(world, open_clip) } else { 0.0 };
        set_player(world, entity, open_clip, pose_time, false);
    }
}

/// Give a placed in-cell door its runtime (DRT-STATE-001): the open state, set from `Open by Default`, and the
/// [`Door`] the motion system reads. Only a door whose model authors `Open` and `Close` sequences gets one so far;
/// the doors that author nothing (ladders, hatches, door frames and a few hinged doors, design §1) wait on the swing.
/// `root` is the placement root, which already carries the model's `NamedSequences`. A saved state parked for the
/// placement replaces the default (#7342, `reference_state::restore_parked_door_state`).
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
    // #7342 — a state a save parked for this door (it was in the exterior ring) goes on it now, over the default.
    crate::cell_loader::reference_state::restore_parked_door_state(world, root);
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

    /// DRT-SND-001: a door opening plays `SNAM`, a door closing plays `ANAM`, a door with neither (or the loop sound only) plays
    /// nothing; and a transition that names a sound the index does not have is dropped without a panic.
    #[test]
    fn a_door_plays_the_open_sound_opening_and_the_close_sound_closing() {
        use byroredux_plugin::esm::cell::DoorData;
        let data = DoorData { open_sound: Some(0x10), close_sound: Some(0x11), loop_sound: Some(0x12), ..Default::default() };
        assert_eq!(door_sound(&data, true), Some(0x10));
        assert_eq!(door_sound(&data, false), Some(0x11));
        let loop_only = DoorData { loop_sound: Some(0x12), ..Default::default() };
        assert_eq!((door_sound(&loop_only, true), door_sound(&loop_only, false)), (None, None), "the loop sound is not a one-shot");
        // A transition on a door whose sounds are not in the index plays nothing and does not fail.
        let (mut world, door, ..) = door_world(false);
        world.insert_resource(crate::components::LoadedCellIndex(std::sync::Arc::new(byroredux_plugin::esm::records::EsmIndex::default())));
        world.query_mut::<Door>().unwrap().get_mut(door).unwrap().data = data;
        world.register::<byroredux_core::ecs::components::GlobalTransform>();
        publish(&world, door, TwoStateTransition::Opened);
        door_motion_system(&world, 0.0);
        assert!(world.get::<TwoStateActivator>(door).unwrap().is_animating, "the door still opens: a sound that cannot play never blocks the motion");
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

    /// #7226 — a door saved (or parked by an unload) mid-swing comes back with the saved `is_animating` over a fresh `Door`
    /// that has no clip time left. The first tick snaps the pose to the saved target and clears the flag, so the door takes
    /// activations and shows its prompt again instead of staying "opening" or "closing" for good.
    #[test]
    fn a_door_restored_mid_swing_settles_and_takes_activations_again() {
        for is_open in [true, false] {
            let (mut world, door, open, _) = door_world(is_open);
            let player = world.spawn();
            world.query_mut::<TwoStateActivator>().unwrap().get_mut(door).unwrap().is_animating = true;
            tick(&world, 0.016);
            assert!(!world.get::<TwoStateActivator>(door).unwrap().is_animating, "the restored flag clears (open: {is_open})");
            let (handle, time, playing) = player_of(&world, door);
            assert_eq!((handle, playing), (open, false));
            assert!((time - if is_open { 1.0 } else { 0.0 }).abs() < 1e-5, "the pose shows the saved state");

            activate(&world, door, player, false);
            door_activation_verdicts(&world);
            assert_eq!(is_open_after(&world, door), !is_open, "and the next activation toggles it");
        }
    }

    /// A placed door with `Open` and `Close` sequences, as the cell loader hands it to `attach_door`.
    fn placed_door(world: &mut World, default_open: bool) -> EntityId {
        let root = world.spawn();
        world.insert(
            root,
            byroredux_core::animation::NamedSequences(std::sync::Arc::from(vec![("Open".into(), 0u32), ("Close".into(), 1u32)])),
        );
        assert!(attach_door(world, root, Default::default(), byroredux_plugin::esm::reader::GameKind::Oblivion, default_open));
        root
    }

    /// DRT-STATE-003 — a door returns to its default state when its cell is unloaded and re-entered, whatever state it was left
    /// in: the state lives on the placement's entity, which the unload despawns, and the next visit seeds a new one from the
    /// reference's `Open by Default` flag. (A save restores the saved state over that seed through the `TwoStateActivator`
    /// column, which the save round trip pins.)
    #[test]
    fn a_door_returns_to_its_default_state_on_reentry() {
        for default_open in [false, true] {
            let mut world = World::new();
            world.register::<Door>();
            let first = placed_door(&mut world, default_open);
            assert_eq!(world.get::<TwoStateActivator>(first).unwrap().is_open, default_open);
            // The visit leaves the door in the other state, then the cell unloads.
            world.query_mut::<TwoStateActivator>().unwrap().get_mut(first).unwrap().is_open = !default_open;
            world.despawn(first);
            let second = placed_door(&mut world, default_open);
            assert_eq!(world.get::<TwoStateActivator>(second).unwrap().is_open, default_open, "re-entry seeds the default again");
            assert_eq!(world.get::<Door>(second).unwrap().posed, None, "and its pose is not yet shown, so the first tick snaps it");
        }
    }

    /// An activation aimed at `door` by `activator`, as the player's press, an actor's package or a script's `Activate` raises it.
    fn activate(world: &World, door: EntityId, activator: EntityId, native: bool) {
        world.query_mut::<byroredux_scripting::ActivateEvent>().unwrap().insert(door, byroredux_scripting::ActivateEvent { activator });
        if native {
            world.query_mut::<byroredux_scripting::NativeActivation>().unwrap().insert(door, byroredux_scripting::NativeActivation);
        }
    }

    fn is_open_after(world: &World, door: EntityId) -> bool {
        byroredux_scripting::two_state_activator_system(world, 0.0);
        world.get::<TwoStateActivator>(door).unwrap().is_open
    }

    /// DRT-ACT-003 — a locked door refuses an activator that carries no key and admits one that does; the key is read from the
    /// *activator's* inventory, not the player's. The lock itself is left as it was.
    #[test]
    fn a_locked_door_admits_only_an_activator_that_carries_its_key() {
        use byroredux_core::ecs::components::{Inventory, ItemStack, Locked};
        const KEY: u32 = 0x0001_4EC0;
        let (mut world, door, ..) = door_world(false);
        world.register::<Locked>();
        world.register::<Inventory>();
        world.insert(door, Locked { lock_level: 50, key_form_id: Some(KEY) });
        let keyless = world.spawn();
        world.insert(keyless, Inventory::new());
        let keyed = world.spawn();
        let mut bag = Inventory::new();
        bag.push(ItemStack::new(KEY, 1));
        world.insert(keyed, bag);

        door_motion_system(&world, 0.0);
        activate(&world, door, keyless, false);
        door_activation_verdicts(&world);
        assert!(!is_open_after(&world, door), "a keyless activator is refused");

        byroredux_scripting::event_cleanup_system(&world, 0.0);
        activate(&world, door, keyed, false);
        door_activation_verdicts(&world);
        assert!(is_open_after(&world, door), "the keyed one is admitted");
        assert!(world.get::<Locked>(door).is_some(), "an actor's key does not unlock the door for everyone");
    }

    /// DRT-ACT-004 — a script that took the door's default processing over keeps it shut; a native activation (the script's own
    /// `Activate`, which "ignores the blocked flag") toggles it, because for a door the default processing is the toggle.
    #[test]
    fn a_script_blocked_door_stays_shut_but_a_native_activation_toggles_it() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        let (mut world, door, ..) = door_world(false);
        world.register::<FormIdComponent>();
        let mut pool = FormIdPool::default();
        let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Oblivion.esm"), local: LocalFormId(0x6800) });
        world.insert_resource(pool);
        world.insert(door, FormIdComponent(id));
        let mut blocks = byroredux_scripting::ReferenceActivationBlocks::default();
        blocks.set_blocked(byroredux_scripting::condition::entity_global_form_id(&world, door).unwrap(), true);
        world.insert_resource(blocks);
        let player = world.spawn();

        door_motion_system(&world, 0.0);
        activate(&world, door, player, false);
        door_activation_verdicts(&world);
        assert!(!is_open_after(&world, door), "the block keeps the door shut");

        byroredux_scripting::event_cleanup_system(&world, 0.0);
        activate(&world, door, player, true);
        door_activation_verdicts(&world);
        assert!(is_open_after(&world, door), "the native activation ignores the block and toggles");
    }

    /// With nothing in the way a plain activation toggles the door as before.
    #[test]
    fn an_unlocked_unblocked_door_toggles() {
        let (mut world, door, ..) = door_world(false);
        let player = world.spawn();
        door_motion_system(&world, 0.0);
        activate(&world, door, player, false);
        door_activation_verdicts(&world);
        assert!(is_open_after(&world, door));
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
