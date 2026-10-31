//! M43 — Words of Power: `Game.TeachWord` / `Game.UnlockWord` and the Story Manager's `NVPE`.
//!
//! CK *TeachWord - Game*: "Teaches the specified WordOfPower to the player." CK *UnlockWord - Game*:
//! "Unlocks the specified word of power so the player can use it in shouts ... Also marks the word as
//! known ... but unlike TeachWord does not add the corresponding shout to the player's inventory."
//! The CK's *New Voice Power* event fires "whenever the player obtains a new Word of Power", so either
//! call raises `NVPE` the first time a word joins the player's set (`OnStoryNewVoicePower(akActor,
//! akVoicePower)`: `R1` the player; the word itself is a form the quests under the event never read,
//! and no event slot carries one).
//!
//! This is the whole of the engine's shout model: which words the player has. Shouting, dragon souls
//! and the shout-list menu are not modelled.

use std::collections::BTreeSet;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

/// The words of power the player has obtained, by `WOOP` FormID. Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct KnownWords(pub BTreeSet<u32>);

impl Resource for KnownWords {}

/// This subsystem's world setup.
pub fn install(world: &mut World) {
    world.init_resource::<KnownWords>();
}

/// This subsystem's saved column; called by the binary's `build_save_registry`.
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<KnownWords>("KnownWords");
}

/// Give the player `word`. True when it was new, in which case `NVPE` is raised with the player in `R1`.
pub fn obtain_word(world: &World, word: u32) -> bool {
    let new = world.try_resource_mut::<KnownWords>().is_some_and(|mut known| known.0.insert(word));
    if !new {
        return false;
    }
    log::info!("word of power {word:08X} obtained; raising NVPE");
    if let Some(player) = crate::papyrus_demo::PapyrusPlayerEntity::get(world) {
        crate::story_manager::raise_story_event(
            world,
            player,
            crate::story_manager::StoryEvent {
                mnemonic: *b"NVPE",
                reference_1: player,
                reference_2: None,
                location_1: None,
                location_2: None,
                scripted: None,
            },
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story_manager::StoryEvent;

    /// A word is new once: the first call stores it and raises `NVPE` on the player, a repeat does neither.
    #[test]
    fn a_new_word_raises_new_voice_power_once() {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        assert!(obtain_word(&world, 0x1000));
        assert!(!obtain_word(&world, 0x1000), "known already");
        let raised: Vec<StoryEvent> = world
            .query::<StoryEvent>()
            .map(|events| events.iter().map(|(_, e)| *e).collect())
            .unwrap_or_default();
        assert_eq!(raised.len(), 1, "the repeat raised nothing");
        assert!(raised.iter().all(|e| e.mnemonic == *b"NVPE" && e.reference_1 == player));
        assert_eq!(world.resource::<KnownWords>().0.len(), 1);
    }
}
