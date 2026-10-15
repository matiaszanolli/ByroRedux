//! Relationship ranks between NPCs (M47.2): the authored `RELA` records and
//! script `SetRelationshipRank` changes that `GetRelationshipRank` reads.
//!
//! The CK *Relationship* page: a relationship joins a "Parent NPC" and a
//! "Child NPC" at a level from Archnemesis to Lover, numbered by
//! *GetRelationshipRank* +4 Lover … 0 Acquaintance … −4 Archnemesis. Ranks
//! live on the NPC base records (the *GetRelationshipRank - Actor* note:
//! "Relationship data is NOT stored for Templated Actors"), so they are keyed
//! by the unordered pair of base FormIDs. A pair with no relationship reads 0,
//! the middle of that scale, the neutral band ("how all factions relate to
//! each other by default").

use std::collections::HashMap;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::EsmIndex;

/// The player's `NPC_` base in every supported game.
pub const PLAYER_BASE: u32 = 0x0000_0007;

pub(crate) fn pair(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

/// The load order's `RELA` ranks. `NOT_SAVED_BY_DESIGN`: rebuilt from the
/// records every boot.
#[derive(Debug, Default, Clone)]
pub struct AuthoredRelationships(HashMap<(u32, u32), i8>);

impl Resource for AuthoredRelationships {}

/// Script `SetRelationshipRank` changes, by base pair. Saved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ScriptedRelationships(pub HashMap<(u32, u32), i8>);

impl Resource for ScriptedRelationships {}

/// Install the load order's [`AuthoredRelationships`]; returns how many.
pub fn install_relationships(world: &mut World, index: &EsmIndex) -> usize {
    let ranks: HashMap<(u32, u32), i8> = index
        .relationships
        .values()
        .filter(|relationship| relationship.parent != 0 && relationship.child != 0)
        .map(|relationship| (pair(relationship.parent, relationship.child), relationship.rank))
        .collect();
    let count = ranks.len();
    world.insert_resource(AuthoredRelationships(ranks));
    if world.try_resource::<ScriptedRelationships>().is_none() {
        world.insert_resource(ScriptedRelationships::default());
    }
    count
}

/// The `NPC_` base an actor entity was placed from; the player's is
/// [`PLAYER_BASE`].
pub fn actor_base(world: &World, entity: EntityId) -> Option<u32> {
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
    if Some(entity) == player {
        return Some(PLAYER_BASE);
    }
    world
        .get::<crate::scene::SceneAliasCandidate>(entity)
        .map(|candidate| candidate.base_form_id)
        .filter(|&base| base != 0)
}

/// The rank between two bases: a script's change, else the authored one,
/// else 0.
pub fn base_relationship_rank(world: &World, a: u32, b: u32) -> i8 {
    let key = pair(a, b);
    let scripted = world
        .try_resource::<ScriptedRelationships>()
        .and_then(|ranks| ranks.0.get(&key).copied());
    scripted.unwrap_or_else(|| {
        world
            .try_resource::<AuthoredRelationships>()
            .and_then(|ranks| ranks.0.get(&key).copied())
            .unwrap_or(0)
    })
}

/// `GetRelationshipRank` between two actors; 0 when either has no base.
pub fn relationship_rank(world: &World, a: EntityId, b: EntityId) -> i8 {
    match (actor_base(world, a), actor_base(world, b)) {
        (Some(a), Some(b)) => base_relationship_rank(world, a, b),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::RelaRecord;

    #[test]
    fn ranks_read_scripts_then_records_then_neutral() {
        const ALVOR: u32 = 0x0001_3478;
        const SIGRID: u32 = 0x0001_3479;
        const HOD: u32 = 0x0001_347A;
        let mut world = World::new();
        crate::register(&mut world);
        let mut index = EsmIndex::default();
        index.relationships.insert(
            1,
            RelaRecord {
                form_id: 1,
                parent: ALVOR,
                child: SIGRID,
                rank: 4,
                association: 0,
            },
        );
        install_relationships(&mut world, &index);
        assert_eq!(base_relationship_rank(&world, SIGRID, ALVOR), 4, "unordered");
        assert_eq!(base_relationship_rank(&world, ALVOR, HOD), 0, "no record: neutral");
        world
            .resource_mut::<ScriptedRelationships>()
            .0
            .insert(pair(SIGRID, ALVOR), -1);
        assert_eq!(base_relationship_rank(&world, ALVOR, SIGRID), -1);
    }
}
