//! NPC combat state: the runtime side of `Faction.SetEnemy`/
//! `Actor.StartCombat` (MQ101's Alduin dragon-attack and keep-escape
//! fights, stages 270+, previously undocumented as a hard blocker —
//! ROADMAP.md's "MQ101 end-to-end playability" entry). Deliberately
//! minimal: this crate only tracks *state*; the actual chase-and-strike
//! behavior is `byroredux::combat::npc_combat_ai_system`, which drives it
//! through the existing player-melee `HitEvent`/`combat_damage_system`
//! pipeline rather than a second damage-application path. No packages,
//! animation selection, ranged, or magic attacks — the smallest slice
//! that gives those stages combat the player can see and be a party to.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_sdk::identity::FormRef;
use byroredux_sdk::relationships::CombatReaction;

/// An actor forced into combat against `target` by `Effect::StartCombat`.
///
/// Not saved: `target` is a session-local `EntityId` (the same #4139/
/// #1696 hazard class `ActorCinematicState::vehicle` and
/// `PendingFragmentActivations` are already documented against), and
/// losing an in-progress scripted combat across a save/reload is the same
/// posture the player-combat siblings `CombatState`/`MeleeState` already
/// take (see their save-registry allowlist entries).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiCombatState {
    pub target: EntityId,
    /// Seconds remaining before the next strike. Starts at `0.0` so an
    /// actor already within melee range on the frame `StartCombat` lands
    /// applies its first strike immediately, matching the vanilla
    /// "already adjacent" case rather than imposing an unauthored delay.
    pub attack_cooldown_remaining: f32,
}

impl Component for AiCombatState {
    type Storage = SparseSetStorage<Self>;
}

/// Directed faction-to-faction combat reactions scripts have set with
/// `Faction.SetEnemy`, layered over the load order's authored `XNAM`
/// relations (the SDK `FactionRelationshipCatalog`).
///
/// Directed because the authored data is: `XNAM` on faction A names B and
/// A's reaction to B, with B's reaction to A a separate record. `SetEnemy`'s
/// two flags likewise set each direction independently (#4318). Consumed by
/// the ambient faction-hostility system (#4414), which reads an override
/// here before the authored relation.
///
/// Keyed by portable [`FormRef`] rather than the global FormID, whose
/// load-order slot means nothing across a save/reload with a different load
/// order, and saved: a scripted hostility (MQ101 turning the Imperials on
/// the player) must survive a reload. A handful of entries per playthrough,
/// so a flat list with replace-on-write rather than a map — which also keeps
/// the save column a plain JSON array.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FactionRelations {
    overrides: Vec<FactionReactionOverride>,
}

/// One directed scripted reaction: how `faction` treats `other_faction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FactionReactionOverride {
    pub faction: FormRef,
    pub other_faction: FormRef,
    /// The `XNAM` combat-reaction encoding (0 Neutral, 1 Enemy, 2 Ally,
    /// 3 Friend) — see [`CombatReaction::from_raw`].
    pub reaction: u32,
}

impl Resource for FactionRelations {}

impl FactionRelations {
    /// `<faction>.SetEnemy(<other_faction>, self_neutral, other_neutral)`:
    /// `faction` turns hostile to `other_faction` unless `self_neutral`,
    /// and the reverse direction unless `other_neutral` — a `true` flag makes
    /// that direction neutral instead (#4318).
    pub fn set_enemy(
        &mut self,
        faction: FormRef,
        other_faction: FormRef,
        self_neutral: bool,
        other_neutral: bool,
    ) {
        let reaction = |neutral: bool| {
            if neutral {
                CombatReaction::Neutral
            } else {
                CombatReaction::Enemy
            }
        };
        self.set_reaction(faction, other_faction, reaction(self_neutral));
        self.set_reaction(other_faction, faction, reaction(other_neutral));
    }

    pub fn set_reaction(
        &mut self,
        faction: FormRef,
        other_faction: FormRef,
        reaction: CombatReaction,
    ) {
        let reaction = reaction_raw(reaction);
        match self
            .overrides
            .iter_mut()
            .find(|o| o.faction == faction && o.other_faction == other_faction)
        {
            Some(existing) => existing.reaction = reaction,
            None => self.overrides.push(FactionReactionOverride {
                faction,
                other_faction,
                reaction,
            }),
        }
    }

    /// The scripted reaction of `faction` toward `other_faction`, if any.
    pub fn reaction(&self, faction: FormRef, other_faction: FormRef) -> Option<CombatReaction> {
        self.overrides
            .iter()
            .find(|o| o.faction == faction && o.other_faction == other_faction)
            .and_then(|o| CombatReaction::from_raw(o.reaction))
    }

    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }
}

/// Inverse of [`CombatReaction::from_raw`].
fn reaction_raw(reaction: CombatReaction) -> u32 {
    match reaction {
        CombatReaction::Neutral => 0,
        CombatReaction::Enemy => 1,
        CombatReaction::Ally => 2,
        CombatReaction::Friend => 3,
    }
}

pub fn register(world: &mut World) {
    world.register::<AiCombatState>();
    if world.try_resource::<FactionRelations>().is_none() {
        world.insert_resource(FactionRelations::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(local: u32) -> FormRef {
        FormRef::new([7; 16], local)
    }

    #[test]
    fn set_enemy_sets_each_direction_from_its_own_flag() {
        let (a, b) = (form(0x13), form(0x9999));
        let mut relations = FactionRelations::default();
        relations.set_enemy(a, b, false, false);
        assert_eq!(relations.reaction(a, b), Some(CombatReaction::Enemy));
        assert_eq!(relations.reaction(b, a), Some(CombatReaction::Enemy));

        // `abOtherIsNeutralToSelf` neutralises only the reverse direction,
        // and a later call replaces rather than duplicates.
        relations.set_enemy(a, b, false, true);
        assert_eq!(relations.reaction(a, b), Some(CombatReaction::Enemy));
        assert_eq!(relations.reaction(b, a), Some(CombatReaction::Neutral));
        assert_eq!(relations.overrides.len(), 2);
        assert_eq!(relations.reaction(a, form(0x14)), None);
    }

    #[test]
    fn register_installs_default_resource_and_storage() {
        let mut world = World::new();
        register(&mut world);
        assert!(world.try_resource::<FactionRelations>().is_some());
        let entity = world.spawn();
        let mut states = world.query_mut::<AiCombatState>().unwrap();
        states.insert(
            entity,
            AiCombatState {
                target: entity,
                attack_cooldown_remaining: 0.0,
            },
        );
        assert!(states.get(entity).is_some());
    }
}
