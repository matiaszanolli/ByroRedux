//! Keywords on references (M47.2): `HasKeyword` (condition 560, the Papyrus
//! `ObjectReference.HasKeyword` guard) and `AddKeyword` / `RemoveKeyword` /
//! `ResetKeyword`.
//!
//! A reference's keywords are, in order of precedence:
//!
//! * a script's `ChangeAnimArchetype`: the actor then has that archetype
//!   keyword and no other (`anim_subgraph`);
//! * a script's `AddKeyword` / `RemoveKeyword`. The FO4 CK *AddKeyword* page:
//!   "Once a keyword has been added by this function it won't be removed if
//!   an alias that also added the keyword was removed from the object. In a
//!   sense, it 'overrides' the base object and alias. To undo the addition,
//!   you can use ResetKeyword." Keyed by the reference's FormID, so a change
//!   outlives its cell's unload, and saved;
//! * the keywords of every alias the reference fills (`QuestAliasInjectedOverlays`);
//! * the base object's `KWDA` — and, for an actor, its race's: Skyrim.esm
//!   authors `ActorTypeDaedra` on 4 `RACE`s and 1 `NPC_`, and the CK
//!   *Keyword* page has Banish "only affect Actors that have the
//!   'ActorTypeDaedra' Keyword".
//!
//! Base objects covered: actors (`NPC_` + `RACE`) and inventory items. Other
//! base types' keywords are not collected yet.

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::EsmIndex;

/// Base object → its keywords (an actor's include its race's).
/// `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone)]
pub struct KeywordTable(HashMap<u32, HashSet<u32>>);

impl Resource for KeywordTable {}

impl KeywordTable {
    pub fn from_index(index: &EsmIndex) -> Self {
        let mut table: HashMap<u32, HashSet<u32>> = HashMap::new();
        for npc in index.npcs.values() {
            let race = index.races.get(&npc.race_form_id);
            let keywords: HashSet<u32> = npc
                .keywords
                .iter()
                .chain(race.into_iter().flat_map(|race| race.keywords.iter()))
                .copied()
                .collect();
            if !keywords.is_empty() {
                table.insert(npc.form_id, keywords);
            }
        }
        for item in index.items.values() {
            if !item.common.keywords.is_empty() {
                table.insert(item.form_id, item.common.keywords.iter().copied().collect());
            }
        }
        Self(table)
    }

    pub fn base_has(&self, base: u32, keyword: u32) -> bool {
        self.0.get(&base).is_some_and(|keywords| keywords.contains(&keyword))
    }
}

/// One reference's script keyword changes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct KeywordChanges {
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
}

/// Script `AddKeyword` / `RemoveKeyword` changes by reference FormID. Saved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ScriptedKeywords(pub HashMap<u32, KeywordChanges>);

impl Resource for ScriptedKeywords {}

/// Install the load order's [`KeywordTable`]; returns how many base objects
/// carry keywords.
pub fn install_keyword_table(world: &mut World, index: &EsmIndex) -> usize {
    let table = KeywordTable::from_index(index);
    let count = table.0.len();
    world.insert_resource(table);
    world.init_resource::<ScriptedKeywords>();
    count
}

/// The FormID a reference's keyword changes are keyed by; the player's is
/// `PlayerRef` (`0x14`).
fn reference_key(world: &World, entity: EntityId) -> Option<u32> {
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
    if Some(entity) == player {
        return Some(0x14);
    }
    crate::condition::entity_global_form_id(world, entity)
}

/// The base object a reference was placed from (the player's `NPC_` for the
/// player).
fn base_of(world: &World, entity: EntityId) -> Option<u32> {
    crate::relationships::actor_base(world, entity)
}

/// Whether `entity` has `keyword`.
pub fn has_keyword(world: &World, entity: EntityId, keyword: u32) -> bool {
    if let Some(key) = reference_key(world, entity) {
        // `ChangeAnimArchetype` swaps the actor's archetype keyword.
        if let Some(has) = crate::anim_subgraph::archetype_override_has(world, key, keyword) {
            return has;
        }
        let scripted = world
            .try_resource::<ScriptedKeywords>()
            .and_then(|changes| changes.0.get(&key).cloned());
        if let Some(changes) = scripted {
            if changes.added.contains(&keyword) {
                return true;
            }
            if changes.removed.contains(&keyword) {
                return false;
            }
        }
    }
    let from_alias = world
        .get::<crate::scene::QuestAliasInjectedOverlays>(entity)
        .is_some_and(|overlays| {
            overlays
                .0
                .values()
                .any(|injected| injected.keywords.contains(&keyword))
        });
    if from_alias {
        return true;
    }
    base_of(world, entity).is_some_and(|base| {
        world
            .try_resource::<KeywordTable>()
            .is_some_and(|table| table.base_has(base, keyword))
    })
}

/// How a script changes a reference's keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum KeywordChange {
    Add,
    Remove,
    /// FO4 `ResetKeyword`: undo this keyword's `AddKeyword` / `RemoveKeyword`.
    Reset,
}

/// Apply a script keyword change to `entity`; `false` when the reference has
/// no FormID to key it by.
pub fn change_keyword(world: &World, entity: EntityId, keyword: u32, change: KeywordChange) -> bool {
    let Some(key) = reference_key(world, entity) else {
        return false;
    };
    let Some(mut scripted) = world.try_resource_mut::<ScriptedKeywords>() else {
        return false;
    };
    let changes = scripted.0.entry(key).or_default();
    changes.added.retain(|&k| k != keyword);
    changes.removed.retain(|&k| k != keyword);
    match change {
        KeywordChange::Add => changes.added.push(keyword),
        KeywordChange::Remove => changes.removed.push(keyword),
        KeywordChange::Reset => {}
    }
    if changes.added.is_empty() && changes.removed.is_empty() {
        scripted.0.remove(&key);
    }
    true
}

pub fn register(world: &mut World) {
    world.init_resource::<ScriptedKeywords>();
}

#[cfg(test)]
mod tests;
