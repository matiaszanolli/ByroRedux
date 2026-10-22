//! `PlayGroup` and `IsAnimPlaying` for placed objects (M47.3).
//!
//! `PlayGroup <group> <flag>` plays an *animation group* on the calling object; the group is an
//! index into the GECK's *AnimGroups* list (`0 Idle`, `1 DynamicIdle`, `2 SpecialIdle`, `3 Forward`,
//! `4 Backward`, `5 Left`, ... compiled as a u16 literal: `Playgroup Forward 0` is
//! `13 10 09 00 | 02 00 | 03 00 | 6e 00 00 00 00`). A model's groups are its `NiControllerSequence`s
//! and each is named for its group — over FalloutNV.esm's object meshes 217 `forward`, 156
//! `specialidle`, 145 `backward`, 136 `idle`, 126 `left`, 107 `right`
//! (`crates/nif/tests/sequence_names_census.rs`) — so the group index is a name and the name picks
//! a clip from the object's [`NamedSequences`].
//!
//! The call starts the clip on the object's [`AnimationPlayer`] at the beginning. The GECK's init
//! flags distinguish `0` (let the current cycle finish first) from `1` / `2` (start at once, `2` at
//! the start of the loop cycle); here every flag starts at once from the first key, so `0` begins a
//! cycle early. `IsAnimPlaying [group]` is whether the object's player is running a clip that has
//! not ended — a `Clamp` clip ends at its duration — and, with a group, whether it is that group's.
//!
//! The groups an *actor* plays (`AttackPower`, `Recoil`, ...) belong to the actor's animation set,
//! not its mesh's sequences: an object with no sequence of the name declines. The indices above
//! `Right`'s on Oblivion (whose list this repo has no page for) decline too.

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer, CycleType, NamedSequences};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;

/// `Look <target>` / `StopLook`: the actor's head follows `target` — GECK *Look*: "forces the calling
/// actor to head-track the target ... This will override all normal head-tracking behavior until StopLook
/// is called, or the target leaves the loaded area". `yaw` / `pitch` are the radians the head is turned
/// from the body's heading now (the app's head-look system eases them toward the target). `NOT_SAVED_BY_DESIGN`:
/// presentation; the reference it names is a session entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadLook {
    pub target: EntityId,
    pub yaw: f32,
    pub pitch: f32,
    /// The actor's head bone once the head-look system has found it, so the skeleton is
    /// not re-walked every frame (#6313). The system re-validates it on use.
    pub head: Option<EntityId>,
}

impl byroredux_core::ecs::storage::Component for HeadLook {
    type Storage = byroredux_core::ecs::sparse_set::SparseSetStorage<Self>;
}

/// Make `actor` head-track `target`; `false` when the actor cannot hold the component.
pub fn start_look(world: &World, actor: EntityId, target: EntityId) -> bool {
    let Some(mut looks) = world.query_mut::<HeadLook>() else { return false };
    looks.insert(actor, HeadLook { target, yaw: 0.0, pitch: 0.0, head: None });
    true
}

/// End a forced look; `false` when the actor had none.
pub fn stop_look(world: &World, actor: EntityId) -> bool {
    world.query_mut::<HeadLook>().is_some_and(|mut looks| looks.remove(actor).is_some())
}

/// The AnimGroups the object scripts play, by index (GECK *AnimGroups*). Oblivion's first five agree
/// (`playgroup forward` compiles to `3` there too); the rest of its list is not documented here.
const GROUPS: [&str; 17] = [
    "Idle", "DynamicIdle", "SpecialIdle", "Forward", "Backward", "Left", "Right", "FastForward",
    "FastBackward", "FastLeft", "FastRight", "DodgeForward", "DodgeBack", "DodgeLeft", "DodgeRight",
    "TurnLeft", "TurnRight",
];

/// The group named by script argument `index`, if this game's list is known to that depth.
pub fn group_name(world: &World, index: f32) -> Option<&'static str> {
    if index < 0.0 || index.fract() != 0.0 {
        return None;
    }
    let limit = if crate::ScriptGame::is_fallout3_nv(world) { GROUPS.len() } else { 5 };
    GROUPS.get(index as usize).filter(|_| (index as usize) < limit).copied()
}

/// The player that runs `object`'s sequences: the one scoped to it whose clip is one of its named
/// sequences, else the one scoped to it. A model with standalone controllers as well as sequences has
/// a player for each (#6253); the script commands drive the sequence one and leave the ambient
/// controllers (a flag's wave, a gate's glow) running.
fn sequence_player(world: &World, object: EntityId) -> Option<EntityId> {
    let sequences = world.get::<NamedSequences>(object).map(|sequences| sequences.clone());
    let players = world.query::<AnimationPlayer>()?;
    let scoped: Vec<(EntityId, u32)> = players
        .iter()
        .filter(|(_, player)| player.root_entity == Some(object))
        .map(|(entity, player)| (entity, player.clip_handle))
        .collect();
    let runs_sequence = |clip: u32| sequences.as_ref().is_some_and(|sequences| sequences.name_of(clip).is_some());
    scoped.iter().find(|(_, clip)| runs_sequence(*clip)).or(scoped.first()).map(|(entity, _)| *entity)
}

/// Start `group` on `object`. `false` when the object has no such sequence or no player to run it.
pub fn play_group(world: &World, object: EntityId, group: &str) -> bool {
    let Some(handle) = world.get::<NamedSequences>(object).and_then(|sequences| sequences.handle(group)) else {
        return false;
    };
    let Some(entity) = sequence_player(world, object) else {
        return false;
    };
    let Some(mut players) = world.query_mut::<AnimationPlayer>() else {
        return false;
    };
    let Some(player) = players.get_mut(entity) else {
        return false;
    };
    player.clip_handle = handle;
    player.local_time = 0.0;
    player.prev_time = 0.0;
    player.reverse_direction = false;
    player.playing = true;
    true
}

/// `Reset3DState`: "clears any saved data regarding a reference's animation/havok state ... Note that
/// this does not reset the object's 3d position" (GECK) — a trap that has played its animation is back
/// at the first frame. The object's player rewinds; what it was playing and whether it runs are kept.
/// `false` when the object has no player to rewind.
pub fn reset_animation(world: &World, object: EntityId) -> bool {
    let Some(entity) = sequence_player(world, object) else {
        return false;
    };
    let Some(mut players) = world.query_mut::<AnimationPlayer>() else {
        return false;
    };
    let Some(player) = players.get_mut(entity) else {
        return false;
    };
    player.local_time = 0.0;
    player.prev_time = 0.0;
    player.reverse_direction = false;
    true
}

/// Whether `object` is animating — and, with `group`, running that group's sequence. `None` when the
/// object has no player to ask.
pub fn is_anim_playing(world: &World, object: EntityId, group: Option<&str>) -> Option<bool> {
    let entity = sequence_player(world, object)?;
    let (clip_handle, playing, local_time) = {
        let players = world.query::<AnimationPlayer>()?;
        let player = players.get(entity)?;
        (player.clip_handle, player.playing, player.local_time)
    };
    let clips = world.try_resource::<AnimationClipRegistry>()?;
    let clip = clips.get(clip_handle)?;
    let ended = clip.cycle_type == CycleType::Clamp && local_time >= clip.duration;
    let running = playing && !ended;
    Some(match group {
        None => running,
        Some(group) => {
            running
                && world
                    .get::<NamedSequences>(object)
                    .and_then(|sequences| sequences.name_of(clip_handle).map(|name| name.eq_ignore_ascii_case(group)))
                    .unwrap_or(false)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::animation::AnimationClip;
    use byroredux_plugin::esm::reader::GameKind;
    use std::sync::Arc;

    fn clip(name: &str, duration: f32, cycle_type: CycleType) -> AnimationClip {
        AnimationClip {
            name: name.to_owned(),
            duration,
            cycle_type,
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

    /// An object with a `forward` and a `backward` sequence and a player running the first.
    fn object(game: GameKind) -> (World, EntityId, u32, u32) {
        let mut world = World::new();
        world.insert_resource(crate::ScriptGame(game));
        let mut registry = AnimationClipRegistry::new();
        let forward = registry.add(clip("Forward", 2.0, CycleType::Clamp));
        let backward = registry.add(clip("Backward", 2.0, CycleType::Loop));
        world.insert_resource(registry);
        let object = world.spawn();
        world.insert(object, NamedSequences(Arc::from(vec![("forward".into(), forward), ("backward".into(), backward)])));
        let player = world.spawn();
        let mut running = AnimationPlayer::new(forward);
        running.root_entity = Some(object);
        world.insert(player, running);
        (world, object, forward, backward)
    }

    #[test]
    fn the_group_index_is_the_animgroups_name_to_the_depth_the_game_documents() {
        let (fallout, ..) = object(GameKind::Fallout3NV);
        assert_eq!(group_name(&fallout, 3.0), Some("Forward"));
        assert_eq!(group_name(&fallout, 4.0), Some("Backward"));
        assert_eq!(group_name(&fallout, 16.0), Some("TurnRight"));
        assert_eq!(group_name(&fallout, 17.0), None, "an actor's combat groups");
        let (oblivion, ..) = object(GameKind::Oblivion);
        assert_eq!(group_name(&oblivion, 3.0), Some("Forward"));
        assert_eq!(group_name(&oblivion, 5.0), None, "past the five Oblivion's list is not documented");
        assert_eq!(group_name(&oblivion, 2.5), None);
    }

    #[test]
    fn play_group_starts_the_named_sequence_from_the_beginning() {
        let (world, object, forward, backward) = object(GameKind::Fallout3NV);
        world.query_mut::<AnimationPlayer>().unwrap().iter_mut().for_each(|(_, p)| p.local_time = 1.5);
        assert!(play_group(&world, object, "Backward"));
        let players = world.query::<AnimationPlayer>().unwrap();
        let (_, player) = players.iter().next().unwrap();
        assert_eq!((player.clip_handle, player.local_time, player.playing), (backward, 0.0, true));
        assert_ne!(forward, backward);
        // A group the object has no sequence of, and an object with no sequences, decline.
        assert!(!play_group(&world, object, "Left"));
        assert!(!play_group(&world, 12345, "Forward"));
    }

    /// #6253 — beside the sequence player, a model with standalone controllers runs a second player
    /// on the same object. The script commands drive the sequence one; the ambient one keeps running.
    #[test]
    fn the_script_commands_drive_the_sequence_player_not_the_ambient_controllers() {
        let (mut world, object, forward, backward) = object(GameKind::Fallout3NV);
        let ambient_clip = world.resource_mut::<AnimationClipRegistry>().add(clip("Ambient", 1.0, CycleType::Loop));
        // The ambient player spawns first, as the cell loader does.
        let ambient = world.spawn();
        let mut ambient_player = AnimationPlayer::new(ambient_clip);
        ambient_player.root_entity = Some(object);
        ambient_player.local_time = 0.5;
        world.insert(ambient, ambient_player);
        let clip_of = |world: &World, entity| world.get::<AnimationPlayer>(entity).map(|p| (p.clip_handle, p.local_time));

        assert_eq!(is_anim_playing(&world, object, Some("Forward")), Some(true));
        assert!(play_group(&world, object, "Backward"));
        assert_eq!(clip_of(&world, ambient), Some((ambient_clip, 0.5)), "the ambient player is untouched");
        let sequence = sequence_player(&world, object).unwrap();
        assert_ne!(sequence, ambient);
        assert_eq!(clip_of(&world, sequence), Some((backward, 0.0)));
        world.get_mut::<AnimationPlayer>(sequence).unwrap().local_time = 1.0;
        assert!(reset_animation(&world, object));
        assert_eq!(clip_of(&world, sequence), Some((backward, 0.0)));
        assert_eq!(clip_of(&world, ambient), Some((ambient_clip, 0.5)));
        assert_ne!(forward, ambient_clip);
    }

    #[test]
    fn look_and_stop_look_set_and_clear_the_actors_head_target() {
        let (mut world, object, _, _) = object(GameKind::Fallout3NV);
        let target = world.spawn();
        world.register::<HeadLook>();
        assert!(start_look(&world, object, target));
        assert_eq!(world.get::<HeadLook>(object).map(|look| look.target), Some(target));
        assert!(stop_look(&world, object));
        assert!(!stop_look(&world, object), "nothing to stop");
    }

    #[test]
    fn reset_animation_rewinds_the_objects_player_and_keeps_its_clip() {
        let (world, object, forward, _) = object(GameKind::Fallout3NV);
        world.query_mut::<AnimationPlayer>().unwrap().iter_mut().for_each(|(_, p)| p.local_time = 1.5);
        assert!(reset_animation(&world, object));
        let players = world.query::<AnimationPlayer>().unwrap();
        let (_, player) = players.iter().next().unwrap();
        assert_eq!((player.clip_handle, player.local_time), (forward, 0.0));
        drop(players);
        assert!(!reset_animation(&world, 12345));
    }

    #[test]
    fn is_anim_playing_reports_a_running_clip_and_which_group() {
        let (world, object, _, _) = object(GameKind::Fallout3NV);
        assert_eq!(is_anim_playing(&world, object, None), Some(true));
        assert_eq!(is_anim_playing(&world, object, Some("Forward")), Some(true));
        assert_eq!(is_anim_playing(&world, object, Some("Backward")), Some(false));
        // A `Clamp` clip that has reached its duration has ended.
        world.query_mut::<AnimationPlayer>().unwrap().iter_mut().for_each(|(_, p)| p.local_time = 2.0);
        assert_eq!(is_anim_playing(&world, object, None), Some(false));
        assert_eq!(is_anim_playing(&world, 12345, None), None, "nothing to ask");
    }
}
