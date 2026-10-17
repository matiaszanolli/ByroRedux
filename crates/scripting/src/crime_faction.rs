//! M47.2 — the faction an actor reports its crimes to: `Actor.GetCrimeFaction()`
//! ("the faction this actor reports it's crimes to") and `SetCrimeFaction(akFaction)`
//! ("Sets the faction this actor reports crimes to. You may pass None to clear out
//! the crime faction on the actor. If the actor does not currently belong to the
//! crime faction, it will be added to that faction").
//!
//! An actor's authored crime faction is its base `NPC_`'s `CRIF`
//! ([`CrimeFactions`], derived from the load order); a script's `SetCrimeFaction`
//! overrides it per reference ([`CrimeFactionOverrides`], saved, a cleared faction
//! kept as `0`). No `CRIF` and no override is no crime faction: how the engine
//! would pick one from the actor's other factions is not documented.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

/// Every `NPC_`'s authored crime faction, by base FormID. Derived, never saved.
#[derive(Debug, Default, Clone)]
pub struct CrimeFactions(Arc<HashMap<u32, u32>>);

impl Resource for CrimeFactions {}

/// Install the load order's authored crime factions; returns how many.
pub fn install_crime_factions(world: &mut World, index: &EsmIndex) -> usize {
    let factions: HashMap<u32, u32> = index
        .npcs
        .iter()
        .filter(|(_, npc)| npc.crime_faction_form_id != 0)
        .map(|(&form_id, npc)| (form_id, npc.crime_faction_form_id))
        .collect();
    let count = factions.len();
    world.insert_resource(CrimeFactions(Arc::new(factions)));
    count
}

/// The crime factions scripts set, by actor reference FormID; `0` is a cleared one.
/// Saved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct CrimeFactionOverrides {
    set: BTreeMap<u32, u32>,
}

impl Resource for CrimeFactionOverrides {}

impl CrimeFactionOverrides {
    pub fn set(&mut self, actor: u32, faction: u32) {
        self.set.insert(actor, faction);
    }

    pub fn get(&self, actor: u32) -> Option<u32> {
        self.set.get(&actor).copied()
    }
}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub. The authored [`CrimeFactions`] arrive with the load order
/// ([`install_crime_factions`]).
pub fn install(world: &mut World) {
    world.init_resource::<CrimeFactionOverrides>();
}

/// #5802 — this subsystem's saved column, `Actor.SetCrimeFaction`. Called by
/// the binary's `build_save_registry` in its row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<CrimeFactionOverrides>("CrimeFactionOverrides");
}

/// The faction `actor` (reference FormID `reference`) reports crimes to.
pub fn crime_faction(world: &World, entity: EntityId, reference: u32) -> Option<u32> {
    if let Some(faction) = world
        .try_resource::<CrimeFactionOverrides>()
        .and_then(|overrides| overrides.get(reference))
    {
        return (faction != 0).then_some(faction);
    }
    let base = crate::relationships::actor_base(world, entity)?;
    world.try_resource::<CrimeFactions>()?.0.get(&base).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::NpcRecord;

    #[test]
    fn only_npcs_with_a_crif_have_an_authored_crime_faction() {
        let mut index = EsmIndex::default();
        index.npcs.insert(0x10, NpcRecord { form_id: 0x10, crime_faction_form_id: 0x99, ..Default::default() });
        index.npcs.insert(0x11, NpcRecord { form_id: 0x11, ..Default::default() });
        let mut world = World::new();
        assert_eq!(install_crime_factions(&mut world, &index), 1);
        let factions = world.resource::<CrimeFactions>();
        assert_eq!(factions.0.get(&0x10), Some(&0x99));
        assert_eq!(factions.0.get(&0x11), None);
    }

    #[test]
    fn a_script_override_wins_and_zero_clears() {
        let mut overrides = CrimeFactionOverrides::default();
        assert_eq!(overrides.get(7), None);
        overrides.set(7, 0x55);
        assert_eq!(overrides.get(7), Some(0x55));
        overrides.set(7, 0);
        assert_eq!(overrides.get(7), Some(0));
    }
}
