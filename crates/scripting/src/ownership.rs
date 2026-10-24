//! A placed reference's explicit owner (M47.2): `GetFactionOwner` and
//! `GetActorOwner`.
//!
//! CK *GetFactionOwner - ObjectReference*: "Gets the Faction that owns this
//! object. Will return None if the object isn't owned by a faction", and its
//! notes: it "returns only the explicit owner of an object reference and None
//! when an explicit owner is not found" — a container's or cell's owner is
//! inherited and not returned. *GetActorOwner* is the same for an ActorBase.
//! The `XOWN` form is a faction or an actor base; [`OwnerKinds`] tells which.
//! The cell loader stamps [`ExplicitOwner`] at spawn; both are re-derived.
//!
//! `SetActorOwner(akActorBase)` ("Sets the actor base as the owner of this
//! object. If None, will clear ownership") and `SetFactionOwner(akFaction)`
//! replace that owner: the reference has one owner slot, so each overrides the
//! other, and a `None` clears it (the CK says so for the actor base; the faction
//! page is silent and gets the same reading). [`OwnerOverrides`] keeps them by
//! the reference's FormID, saved.

use std::collections::HashSet;
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

/// The `XOWN` form a placed reference carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplicitOwner(pub u32);

impl Component for ExplicitOwner {
    type Storage = SparseSetStorage<Self>;
}

/// Which forms are factions and which are actor bases.
#[derive(Debug, Default, Clone)]
pub struct OwnerKinds {
    factions: Arc<HashSet<u32>>,
    actors: Arc<HashSet<u32>>,
}

impl Resource for OwnerKinds {}

/// Install the load order's faction and actor-base forms.
pub fn install_owner_kinds(world: &mut World, index: &EsmIndex) {
    world.insert_resource(OwnerKinds {
        factions: Arc::new(index.factions.keys().copied().collect()),
        actors: Arc::new(index.npcs.keys().copied().collect()),
    });
}

/// The owners scripts set, by reference FormID; `0` is a cleared owner. Saved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct OwnerOverrides {
    owners: std::collections::BTreeMap<u32, u32>,
}

impl Resource for OwnerOverrides {}

impl OwnerOverrides {
    pub fn set(&mut self, reference: u32, owner: u32) {
        self.owners.insert(reference, owner);
    }

    pub fn get(&self, reference: u32) -> Option<u32> {
        self.owners.get(&reference).copied()
    }
}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub: the owner kinds (derived), the spawn-stamped
/// [`ExplicitOwner`] storage, and the script overrides (saved).
pub fn install(world: &mut World) {
    world.init_resource::<OwnerKinds>();
    world.register::<ExplicitOwner>();
    world.init_resource::<OwnerOverrides>();
}

/// #5802 — this subsystem's saved column, `SetActorOwner` /
/// `SetFactionOwner`. Called by the binary's `build_save_registry` in its
/// row order (the schema fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<OwnerOverrides>("OwnerOverrides");
}

/// The owner a script set on `entity`'s reference with `SetActorOwner` /
/// `SetFactionOwner`, if it did: `Some(0)` is a cleared owner. The theft rule
/// reads this before the cell loader's stamped `Owned`, so a script's ownership
/// change reaches it (#5866).
pub fn owner_override(world: &World, entity: EntityId) -> Option<u32> {
    let reference = crate::condition::entity_global_form_id(world, entity)?;
    world.try_resource::<OwnerOverrides>()?.get(reference)
}

/// The explicit owner of `entity`: a script's override of its reference, else the
/// `XOWN` form the cell loader stamped. `pending` is the run's own latest set.
fn explicit_owner(world: &World, entity: EntityId, pending: Option<u32>) -> Option<u32> {
    let owner = match pending {
        Some(owner) => owner,
        None => match owner_override(world, entity) {
            Some(owner) => owner,
            None => world.get::<ExplicitOwner>(entity)?.0,
        },
    };
    (owner != 0).then_some(owner)
}

/// The faction explicitly owning `entity`, if its owner is a faction.
pub fn faction_owner(world: &World, entity: EntityId) -> Option<u32> {
    faction_owner_with(world, entity, None)
}

/// The actor base explicitly owning `entity`, if its owner is one.
pub fn actor_owner(world: &World, entity: EntityId) -> Option<u32> {
    actor_owner_with(world, entity, None)
}

/// [`faction_owner`] with the owner a run set earlier in itself.
pub fn faction_owner_with(world: &World, entity: EntityId, pending: Option<u32>) -> Option<u32> {
    let owner = explicit_owner(world, entity, pending)?;
    world.try_resource::<OwnerKinds>()?.factions.contains(&owner).then_some(owner)
}

/// [`actor_owner`] with the owner a run set earlier in itself.
pub fn actor_owner_with(world: &World, entity: EntityId, pending: Option<u32>) -> Option<u32> {
    let owner = explicit_owner(world, entity, pending)?;
    world.try_resource::<OwnerKinds>()?.actors.contains(&owner).then_some(owner)
}

/// Whether `form` is a faction the load order defines.
pub fn is_faction(world: &World, form: u32) -> bool {
    world.try_resource::<OwnerKinds>().is_some_and(|kinds| kinds.factions.contains(&form))
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{FactionRecord, NpcRecord};

    #[test]
    fn the_owner_is_a_faction_or_an_actor_base_never_both() {
        let mut index = EsmIndex::default();
        index.factions.insert(
            0xF,
            FactionRecord {
                form_id: 0xF,
                editor_id: String::new(),
                full_name: String::new(),
                flags: 0,
                relations: Vec::new(),
                ranks: Vec::new(),
                reputation: None,
                vendor: None,
                crime: None,
            },
        );
        index.npcs.insert(0xA, NpcRecord::default());
        let mut world = World::new();
        world.register::<ExplicitOwner>();
        install_owner_kinds(&mut world, &index);
        let (by_faction, by_actor, unowned) = (world.spawn(), world.spawn(), world.spawn());
        world.insert(by_faction, ExplicitOwner(0xF));
        world.insert(by_actor, ExplicitOwner(0xA));
        assert_eq!((faction_owner(&world, by_faction), actor_owner(&world, by_faction)), (Some(0xF), None));
        assert_eq!((faction_owner(&world, by_actor), actor_owner(&world, by_actor)), (None, Some(0xA)));
        assert_eq!((faction_owner(&world, unowned), actor_owner(&world, unowned)), (None, None));
    }
}
