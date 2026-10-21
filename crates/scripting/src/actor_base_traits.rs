//! M47.2 — what an actor base record says about the actor, for the natives that
//! read it: `ActorBase.GetSex()` ("the actor's sex": -1 None, 0 Male, 1 Female)
//! and `GetVoiceType()` ("the VoiceType for this actor or talking activator").
//!
//! The sex is the `ACBS` flags' bit 0 (the Female flag, on every game that
//! authors `ACBS`), the voice type the `NPC_`'s `VTCK`. Derived from the load
//! order at install, never saved.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

/// One `NPC_` base's traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorBaseTrait {
    pub female: bool,
    /// ACBS flag `0x20` ("Unique", xEdit `wbDefinitionsTES5`): `ActorBase.IsUnique`.
    pub unique: bool,
    /// The `VTCK` voice type; `0` when the record authors none.
    pub voice: u32,
    /// `CREA`'s `wbCreatureTypeEnum` value (0 Animal ... 7 Giant); `None` for an `NPC_`.
    pub creature_type: Option<u8>,
}

/// Every `NPC_` base's traits, by base FormID.
#[derive(Debug, Default, Clone)]
pub struct ActorBaseTraits(Arc<HashMap<u32, ActorBaseTrait>>);

impl Resource for ActorBaseTraits {}

impl ActorBaseTraits {
    pub fn get(&self, base: u32) -> Option<ActorBaseTrait> {
        self.0.get(&base).copied()
    }
}

/// Install the load order's actor base traits; returns how many bases.
pub fn install_actor_base_traits(world: &mut World, index: &EsmIndex) -> usize {
    let traits: HashMap<u32, ActorBaseTrait> = index
        .npcs
        .iter()
        .map(|(&form_id, npc)| {
            (
                form_id,
                ActorBaseTrait {
                    female: npc.acbs_flags & 1 != 0,
                    unique: npc.acbs_flags & 0x20 != 0,
                    voice: npc.voice_form_id,
                    creature_type: npc.creature_stats.map(|stats| stats.creature_type),
                },
            )
        })
        .collect();
    let count = traits.len();
    world.insert_resource(ActorBaseTraits(Arc::new(traits)));
    count
}

/// Whether the actor `entity` is female (its base's `ACBS` female flag) —
/// *GetIsSex*, both the script getter and condition 70. `None` when the
/// entity has no actor base with recorded traits.
pub fn is_female(world: &World, entity: EntityId) -> Option<bool> {
    let base = crate::relationships::actor_base(world, entity)?;
    Some(world.try_resource::<ActorBaseTraits>()?.get(base)?.female)
}

/// The voice type of the actor `entity` (its base's `VTCK`).
pub fn voice_type(world: &World, entity: EntityId) -> Option<u32> {
    let base = crate::relationships::actor_base(world, entity)?;
    let voice = world.try_resource::<ActorBaseTraits>()?.get(base)?.voice;
    (voice != 0).then_some(voice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::NpcRecord;

    #[test]
    fn sex_is_the_acbs_female_bit_and_voice_the_vtck() {
        let mut index = EsmIndex::default();
        index.npcs.insert(1, NpcRecord { form_id: 1, acbs_flags: 0x1, voice_form_id: 0x77, ..Default::default() });
        index.npcs.insert(2, NpcRecord { form_id: 2, acbs_flags: 0x2, ..Default::default() });
        let mut world = World::new();
        assert_eq!(install_actor_base_traits(&mut world, &index), 2);
        let traits = world.resource::<ActorBaseTraits>();
        assert_eq!(traits.get(1), Some(ActorBaseTrait { female: true, unique: false, voice: 0x77, creature_type: None }));
        assert_eq!(traits.get(2), Some(ActorBaseTrait { female: false, unique: false, voice: 0, creature_type: None }));
        assert_eq!(traits.get(3), None);
    }
}
