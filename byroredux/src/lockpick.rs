//! Picking a lock (M43) — the player action behind the Story Manager's `LOCK` event.
//!
//! Fallout 4's `PlayerLockpickSuccess` / `CA_PickLock` story events have no player action to hang from here:
//! `lock.rs` says outright that lockpicking "is not implemented yet". This is the smallest action that gives
//! the event a producer, and it is **engine policy**, not the original's rule — no local source documents
//! Fallout 4's lock rules (the Locksmith perk's ranks, the minigame, when a pin breaks):
//!
//! - **What can be picked.** The CK's FO4 `GetLockLevel` page: `0` unlocked, `25` Novice, `50` Advanced,
//!   `75` Expert, `100` Master, and `251..=255` Barred / Chained / Terminal Only / Inaccessible / Requires
//!   Key — so a lock of level `0..=100` is pickable and anything above is not.
//! - **What it takes.** A pick in the inventory: the misc item whose editor id is `BobbyPin` (Fallout 4) or
//!   `Lockpick` (Skyrim and earlier; checked against `Fallout4.esm`'s `EDID`s). There is no minigame, so the
//!   pick works every time and is not used up, and no tier needs a perk.
//! - **Where it applies.** Only on a title whose Story Manager tree has a `LOCK` root (Fallout 4 and later);
//!   every other game keeps "a lock without its key stays shut" until its own lock rules are sourced.
//!
//! A picked lock is recorded like a key unlock (the persistent lock ledger), and `LOCK` is raised with the
//! player in `R1` — the slots of that event are undocumented, and the quests under it read none.

use byroredux_core::ecs::components::{Inventory, Locked};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_scripting::story_manager::{raise_story_event, SmTree, StoryEvent};

/// The `SMEN` mnemonic of the lockpick-success event.
pub(crate) const LOCK: [u8; 4] = *b"LOCK";

/// The highest lock level a pick can open (`Master`).
const MAX_PICKABLE_LEVEL: u8 = 100;

/// The base form of the player's picks. `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct LockpickItem(pub(crate) Option<u32>);

impl Resource for LockpickItem {}

pub(crate) fn install_lockpick_item(world: &mut World, index: &EsmIndex) {
    let form = index
        .items
        .values()
        .filter(|item| matches!(item.common.editor_id.as_str(), "BobbyPin" | "Lockpick"))
        .map(|item| item.form_id)
        .min();
    world.insert_resource(LockpickItem(form));
}

/// Whether the player could pick the lock on `target` now: it is locked at a pickable level, the game has the
/// `LOCK` event, and the player holds a pick.
pub(crate) fn can_pick(world: &World, target: EntityId) -> bool {
    let Some(lock) = world.get::<Locked>(target).map(|lock| *lock) else {
        return false;
    };
    if lock.lock_level > MAX_PICKABLE_LEVEL {
        return false;
    }
    if !world
        .try_resource::<SmTree>()
        .is_some_and(|tree| tree.roots_by_mnemonic.contains_key(&LOCK))
    {
        return false;
    }
    let Some(pick) = world.try_resource::<LockpickItem>().and_then(|item| item.0) else {
        return false;
    };
    byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world)
        .and_then(|player| world.get::<Inventory>(player).map(|inventory| inventory.count_of(pick) > 0))
        .unwrap_or(false)
}

/// The lock on the player's activated target was picked: raise `LOCK`.
pub(crate) fn raise_lockpick_event(world: &World) {
    let Some(player) = byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world) else {
        return;
    };
    raise_story_event(
        world,
        player,
        StoryEvent {
            mnemonic: LOCK,
            reference_1: player,
            reference_2: None,
            location_1: None,
            location_2: None,
            scripted: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::ItemStack;
    use byroredux_plugin::esm::records::{SmNodeKind, SmNodeRecord};
    use std::collections::HashMap;

    const PIN: u32 = 0xA;

    fn setup(root: bool, pins: u32) -> (World, EntityId, EntityId) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<Inventory>();
        world.register::<Locked>();
        let player = world.spawn();
        world.insert(player, Inventory { items: if pins > 0 { vec![ItemStack::new(PIN, pins)] } else { Vec::new() } });
        world.insert_resource(byroredux_scripting::papyrus_demo::PapyrusPlayerEntity(player));
        world.insert_resource(LockpickItem(Some(PIN)));
        let records = if root {
            HashMap::from([(0x10, SmNodeRecord { form_id: 0x10, kind: SmNodeKind::Event, event_mnemonic: Some(LOCK), ..Default::default() })])
        } else {
            HashMap::new()
        };
        byroredux_scripting::story_manager::install_story_manager(&mut world, &records);
        let door = world.spawn();
        (world, player, door)
    }

    #[test]
    fn a_pick_opens_levels_up_to_master_and_nothing_above() {
        let (mut world, _player, door) = setup(true, 1);
        for (level, pickable) in [(0, true), (25, true), (100, true), (101, false), (251, false), (255, false)] {
            world.insert(door, Locked { lock_level: level, key_form_id: None });
            assert_eq!(can_pick(&world, door), pickable, "level {level}");
        }
    }

    #[test]
    fn it_needs_a_pick_a_lock_and_a_game_with_the_event() {
        let (mut w, _p, door) = setup(true, 0);
        w.insert(door, Locked { lock_level: 25, key_form_id: None });
        assert!(!can_pick(&w, door), "no pick held");
        let (w, _p, door) = setup(true, 1);
        assert!(!can_pick(&w, door), "not locked");
        let (mut w, _p, door) = setup(false, 1);
        w.insert(door, Locked { lock_level: 25, key_form_id: None });
        assert!(!can_pick(&w, door), "no LOCK root: the game keeps its locks shut");
    }

    #[test]
    fn a_pick_success_raises_lock_on_the_player() {
        let (w, player, _door) = setup(true, 1);
        raise_lockpick_event(&w);
        let event = w.get::<StoryEvent>(player).map(|e| *e).expect("raised");
        assert_eq!((event.mnemonic, event.reference_1), (LOCK, player));
    }
}
