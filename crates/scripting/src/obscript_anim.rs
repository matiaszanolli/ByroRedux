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

/// Start `group` on `object`. `false` when the object has no such sequence or no player to run it.
pub fn play_group(world: &World, object: EntityId, group: &str) -> bool {
    let Some(handle) = world.get::<NamedSequences>(object).and_then(|sequences| sequences.handle(group)) else {
        return false;
    };
    let Some(mut players) = world.query_mut::<AnimationPlayer>() else {
        return false;
    };
    let Some((_, player)) = players.iter_mut().find(|(_, player)| player.root_entity == Some(object)) else {
        return false;
    };
    player.clip_handle = handle;
    player.local_time = 0.0;
    player.prev_time = 0.0;
    player.reverse_direction = false;
    player.playing = true;
    true
}

/// Whether `object` is animating — and, with `group`, running that group's sequence. `None` when the
/// object has no player to ask.
pub fn is_anim_playing(world: &World, object: EntityId, group: Option<&str>) -> Option<bool> {
    let (clip_handle, playing, local_time) = {
        let players = world.query::<AnimationPlayer>()?;
        let (_, player) = players.iter().find(|(_, player)| player.root_entity == Some(object))?;
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
