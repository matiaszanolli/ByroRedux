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
use byroredux_sdk::relationships::{CombatReaction, FactionRelationshipCatalog};
use std::sync::Arc;

/// An actor in combat against `target`. Two producers arm it:
/// `Effect::StartCombat` (a quest fragment forces the fight) and, since
/// #4414, `byroredux`'s `faction_hostility_system` (an actor sees a target
/// its faction relations and aggression say it attacks). The ambient
/// producer never overwrites a state a script already set.
///
/// Not saved: `target` is a session-local `EntityId` (the same #4139/
/// #1696 hazard class `ActorCinematicState::vehicle` and
/// `PendingFragmentActivations` are already documented against), and
/// losing an in-progress combat across a save/reload is the same posture
/// the player-combat siblings `CombatState`/`MeleeState` already take (see
/// their save-registry allowlist entries). What that loses differs by
/// producer: ambient combat is re-derived, because
/// `faction_hostility_system` re-creates it within one
/// `EVALUATION_PERIOD_SECS` (0.5 s) when the pair is still in range and in
/// sight; only a scripted combat against a target that is not otherwise
/// hostile is gone.
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

/// CK *IsInCombat - Actor*: whether the actor is in combat — it has a combat
/// target ([`AiCombatState`]), or, for the player, some actor is fighting it.
pub fn is_in_combat(world: &World, actor: EntityId) -> bool {
    if world.get::<AiCombatState>(actor).is_some() {
        return true;
    }
    let is_player = crate::papyrus_demo::PapyrusPlayerEntity::get(world).is_some_and(|player| player == actor);
    is_player
        && world
            .query::<AiCombatState>()
            .is_some_and(|states| states.iter().any(|(_, state)| state.target == actor))
}

/// #4816 — marks an [`AiCombatState`] that the engine's ambient
/// `faction_hostility_system` started, with the target it started against
/// and how long that target has been out of contact. Only a combat carrying
/// a current marker is ever dropped for lost contact.
///
/// A combat whose current target differs (a script re-targeted it) or whose
/// `AiCombatState` is gone (target died, `StopCombat`, death) is no longer
/// ambient, and the marker is dropped. #5046 — `Effect::StartCombat` removes
/// it too, so a script that re-issues combat against the *same* target the
/// ambient pass already chose takes ownership of the fight instead of
/// leaving it on the ambient grace timer. It lives in this crate rather than
/// beside its producer for exactly that reason. Not saved: `AiCombatState`
/// is not either, and ambient combat is re-derived after a load.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientEngagement {
    pub target: EntityId,
    pub out_of_contact_secs: f32,
}

impl Component for AmbientEngagement {
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

/// The load order's authored faction relations (`XNAM`), published beside the
/// resolver that builds them so scripts can read how two actors' factions
/// treat each other. `NOT_SAVED_BY_DESIGN`: derived from the load order.
#[derive(Debug, Clone, Default)]
pub struct FactionCatalog(pub Arc<FactionRelationshipCatalog>);

impl Resource for FactionCatalog {}

/// The engine-policy combination of every directed faction pair between two
/// actors: an override set by script wins over the authored relation, a Friend
/// pair beats an Ally pair, either beats an Enemy pair, and anything else is
/// Neutral. How the original engine combines conflicting memberships is
/// undocumented; this is the one rule ambient hostility and
/// `GetFactionReaction` share.
pub fn combined_reaction(
    own: &[FormRef],
    other: &[FormRef],
    overrides: &FactionRelations,
    catalog: &FactionRelationshipCatalog,
) -> CombatReaction {
    let (mut friend, mut ally, mut enemy) = (false, false, false);
    for &own in own {
        for &other in other {
            let reaction = overrides.reaction(own, other).or_else(|| {
                catalog
                    .relationship(own, other)
                    .and_then(|relation| relation.combat_reaction())
            });
            match reaction {
                Some(CombatReaction::Friend) => friend = true,
                Some(CombatReaction::Ally) => ally = true,
                Some(CombatReaction::Enemy) => enemy = true,
                Some(CombatReaction::Neutral) | None => {}
            }
        }
    }
    if friend {
        CombatReaction::Friend
    } else if ally {
        CombatReaction::Ally
    } else if enemy {
        CombatReaction::Enemy
    } else {
        CombatReaction::Neutral
    }
}

/// The portable identities of the factions `actor` belongs to (a negative rank
/// is not membership: content uses -1 to take an actor out of a faction).
fn faction_refs(world: &World, actor: EntityId) -> Option<Vec<FormRef>> {
    let ids: Vec<u32> = world
        .get::<byroredux_core::ecs::components::FactionRanks>(actor)
        .map(|ranks| ranks.0.iter().filter(|(_, rank)| *rank >= 0).map(|(faction, _)| *faction).collect())
        .unwrap_or_default();
    let identity = world.try_resource::<crate::LoadOrderIdentity>()?;
    Some(ids.into_iter().filter_map(|faction| identity.form_ref(faction)).collect())
}

/// CK *GetFactionReaction - Actor*: "the faction-based reaction between this
/// actor and another" — 0 Neutral, 1 Enemy, 2 Ally, 3 Friend. `None` when the
/// load order's identity is not installed.
pub fn faction_reaction(world: &World, actor: EntityId, other: EntityId) -> Option<i32> {
    let own = faction_refs(world, actor)?;
    let other = faction_refs(world, other)?;
    let overrides = world.try_resource::<FactionRelations>().map(|r| r.clone()).unwrap_or_default();
    let catalog = world.try_resource::<FactionCatalog>().map(|c| Arc::clone(&c.0)).unwrap_or_default();
    Some(reaction_raw(combined_reaction(&own, &other, &overrides, &catalog)) as i32)
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
    world.init_resource::<FactionRelations>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(local: u32) -> FormRef {
        FormRef::new([7; 16], local)
    }

    /// CK *IsInCombat - Actor*: an actor with a combat target is in combat, and
    /// so is the player when some actor is fighting it.
    #[test]
    fn in_combat_follows_combat_targets() {
        let mut world = World::new();
        world.register::<AiCombatState>();
        let player = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        let guard = world.spawn();
        let bystander = world.spawn();
        assert!(!is_in_combat(&world, guard) && !is_in_combat(&world, player));
        world.insert(
            guard,
            AiCombatState {
                target: player,
                attack_cooldown_remaining: 0.0,
            },
        );
        assert!(is_in_combat(&world, guard));
        assert!(is_in_combat(&world, player), "someone is fighting the player");
        assert!(!is_in_combat(&world, bystander));
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

    /// The shared policy: a Friend pair beats an Ally pair, either beats an
    /// Enemy pair, and a script's override wins over the authored relation.
    #[test]
    fn combined_reaction_prefers_friendship_then_hostility() {
        let (a, b, c) = (form(1), form(2), form(3));
        let catalog = FactionRelationshipCatalog::default();
        let mut relations = FactionRelations::default();
        assert_eq!(combined_reaction(&[a], &[b], &relations, &catalog), CombatReaction::Neutral);
        relations.set_reaction(a, b, CombatReaction::Enemy);
        assert_eq!(combined_reaction(&[a], &[b], &relations, &catalog), CombatReaction::Enemy);
        relations.set_reaction(a, c, CombatReaction::Ally);
        assert_eq!(combined_reaction(&[a], &[b, c], &relations, &catalog), CombatReaction::Ally);
        relations.set_reaction(a, c, CombatReaction::Friend);
        assert_eq!(combined_reaction(&[a], &[b, c], &relations, &catalog), CombatReaction::Friend);
        assert_eq!(combined_reaction(&[], &[b, c], &relations, &catalog), CombatReaction::Neutral);
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
