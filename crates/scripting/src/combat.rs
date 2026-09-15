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
use std::collections::HashSet;

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

/// Faction pairs `Faction.SetEnemy` has marked hostile this session.
///
/// Tracked for observability and future consumers; nothing currently
/// queries it for *ambient* hostility detection — every MQ101 `SetEnemy`
/// call in the real corpus is paired with an explicit `StartCombat`
/// (`Fragment_112`/`Fragment_113`/`Fragment_296`), which is what actually
/// drives `AiCombatState` above. Modeling ambient faction-to-faction
/// aggro (e.g. "any two hostile-faction actors that see each other start
/// fighting") is a separate, larger AI-perception feature this slice
/// deliberately does not attempt.
///
/// Not saved: plain `u32` FormIDs rather than `FormIdPair`s, so a
/// differing load order across a save/reload could silently mismatch —
/// and since nothing reads this yet (`is_enemy` has no production caller),
/// losing it on reload changes no behavior. Ambient hostility, the consumer
/// this is waiting for, is tracked by #4414.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactionRelations {
    hostile_pairs: HashSet<(u32, u32)>,
}

impl Resource for FactionRelations {}

impl FactionRelations {
    fn key(a: u32, b: u32) -> (u32, u32) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    /// `<faction>.SetEnemy(<other_faction>, false, false)` — marks the pair
    /// mutually hostile. The two Papyrus flags are `abSelfIsNeutralToOther`
    /// and `abOtherIsNeutralToSelf`: a `true` one makes that direction
    /// *neutral* instead, which this single undirected hostile pair cannot
    /// represent, so the lowerer declines any call that sets either (#4318).
    pub fn set_enemy(&mut self, faction: u32, other_faction: u32) {
        self.hostile_pairs.insert(Self::key(faction, other_faction));
    }

    pub fn is_enemy(&self, faction: u32, other_faction: u32) -> bool {
        self.hostile_pairs
            .contains(&Self::key(faction, other_faction))
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

    #[test]
    fn faction_relations_are_order_independent() {
        let mut relations = FactionRelations::default();
        relations.set_enemy(0x0009_9999, 0x0000_0013);
        assert!(relations.is_enemy(0x0009_9999, 0x0000_0013));
        assert!(relations.is_enemy(0x0000_0013, 0x0009_9999));
        assert!(!relations.is_enemy(0x0009_9999, 0x0000_0014));
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
