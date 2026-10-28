//! #5804 — the `evaluate_function` arms that read the Run-On actor: death,
//! level and XP, factions, reputation, equipment, perks, relationships and
//! the follower flags.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::Condition;

use super::{entity_global_form_id, resolve_entity_by_global_form_id, truth};

pub(super) fn get_relationship_rank(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    match resolve_entity_by_global_form_id(world, condition.param_1) {
        Some(other) => f32::from(crate::relationships::relationship_rank(world, entity, other)),
        None => 0.0,
    }
}

pub(super) fn get_player_teammate(entity: EntityId, world: &World) -> f32 {
    let form = entity_global_form_id(world, entity);
    truth(form.is_some_and(|form| {
        world
            .try_resource::<crate::FollowerState>()
            .is_some_and(|state| state.read(form, crate::FollowerFlag::Teammate))
    }))
}

/// #5778 — the actor's `Background` race against the load's
/// [`PlayableRaces`](crate::obscript_more::PlayableRaces).
pub(super) fn get_is_playable_race(entity: EntityId, world: &World) -> f32 {
    use byroredux_core::character::Background;
    let race = world.get::<Background>(entity).map(|b| b.race_form_id);
    truth(race.is_some_and(|race| {
        world
            .try_resource::<crate::obscript_more::PlayableRaces>()
            .is_some_and(|races| races.contains(race))
    }))
}

pub(super) fn get_dead(entity: EntityId, world: &World) -> f32 {
    // Death is a sparse lifecycle marker: absence is the overwhelmingly
    // common alive state, while combat/scripts/resurrection can share
    // one explicit component with authored CTDA checks.
    use byroredux_core::ecs::components::Dead;
    if world.has::<Dead>(entity) {
        1.0
    } else {
        0.0
    }
}

pub(super) fn get_faction_rank(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    // GetFactionRank(faction_form_id) → the Run-On actor's rank in
    // `param_1`'s faction, or -1.0 (Bethesda's "not in faction"
    // sentinel) when the actor has no `FactionRanks` or isn't a
    // member. Faction ids are compared in the NPC's source space —
    // identity-equal to a remapped `param_1` in single-plugin loads
    // (see `FactionRanks` docs).
    use byroredux_core::ecs::components::FactionRanks;
    world
        .get::<FactionRanks>(entity)
        .and_then(|f| f.rank(condition.param_1))
        .map_or(-1.0, |rank| rank as f32)
}

pub(super) fn get_in_faction(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    // GetInFaction(faction_form_id) → 1.0 when the Run-On actor
    // holds any rank in `param_1`'s faction, 0.0 otherwise. Same
    // `FactionRanks` read and the same source-space faction-id
    // compare as `GetFactionRank` above, collapsed to membership.
    use byroredux_core::ecs::components::FactionRanks;
    if world
        .get::<FactionRanks>(entity)
        .is_some_and(|f| f.rank(condition.param_1).is_some())
    {
        1.0
    } else {
        0.0
    }
}

pub(super) fn get_level(entity: EntityId, world: &World) -> f32 {
    // GetLevel → the Run-On actor's character level (0.0 when the
    // actor carries no `CharacterLevel`).
    use byroredux_core::character::CharacterLevel;
    world
        .get::<CharacterLevel>(entity)
        .map_or(0.0, |l| f32::from(l.level))
}

pub(super) fn get_equipped(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    // `EquipmentSlots` stores inventory indices rather than item
    // FormIDs. Resolve each occupied index through the actor's
    // Inventory and compare its base record with param_1.
    use byroredux_core::ecs::components::{EquipmentSlots, Inventory};
    // #3112 — `equipped_indices()` spans the biped occupants AND the
    // wielded weapon, which lives outside the occupancy array. A bare
    // `occupants` scan reports every equipped weapon as unequipped.
    //
    // #3580 — collect the indices and DROP the `EquipmentSlots` guard
    // before taking `Inventory`. Holding the two overlapped recorded
    // `EquipmentSlots -> Inventory`, while the fragment runtime
    // reaches the same pair through
    // `query_2_mut_mut::<Inventory, EquipmentSlots>`, which acquires
    // in TypeId order. One of the two orders is then always wrong,
    // and the cycle detector aborted whichever ran second. Not
    // overlapping the guards makes this site order-independent.
    let Some(equipped_indices) = world
        .get::<EquipmentSlots>(entity)
        .map(|slots| slots.equipped_indices().collect::<Vec<_>>())
    else {
        return 0.0;
    };
    let Some(inventory) = world.get::<Inventory>(entity) else {
        return 0.0;
    };
    let equipped = equipped_indices.into_iter().any(|index| {
        inventory
            .get(index)
            .is_some_and(|stack| stack.count > 0 && stack.base_form_id == condition.param_1)
    });
    if equipped {
        1.0
    } else {
        0.0
    }
}

pub(super) fn get_xp_for_next_level(entity: EntityId, world: &World) -> f32 {
    // GetXPForNextLevel → XP required for the actor's next level, from
    // the per-game `LevelingModel` in the `CharacterRuleset` resource.
    // 0.0 without the ruleset (the leveling curve is game-supplied).
    use byroredux_core::character::{CharacterLevel, CharacterRuleset};
    let Some(rs) = world.try_resource::<CharacterRuleset>() else {
        return 0.0;
    };
    let level = world.get::<CharacterLevel>(entity).map_or(0, |l| l.level);
    rs.leveling.xp_to_next(level)
}

pub(super) fn has_perk(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    // 1.0 iff the Run-On actor's canonical ranked `Perks` component
    // owns `param_1`. Spawned NPCs carry this component; the legacy
    // `PerkList` projection is not populated by production code.
    use byroredux_core::character::Perks;
    let Some(perks) = world.get::<Perks>(entity) else {
        return 0.0;
    };
    if perks.rank(condition.param_1) > 0 {
        1.0
    } else {
        0.0
    }
}

pub(super) fn get_reputation(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    // GetReputation(repu_form_id, axis) → the Run-On actor's raw
    // Fame/Infamy with `param_1`. `param_2` is the axis selector
    // (Bethesda console convention: 1 = Fame, 0 = Infamy). 0.0 when the
    // actor carries no `FactionReputation` or the faction is unknown.
    // `param_1` is the global-space `REPU` FormID (#1666 remap).
    use byroredux_core::character::FactionReputation;
    let Some(rep) = world.get::<FactionReputation>(entity) else {
        return 0.0;
    };
    let points = if condition.param_2 == 1 {
        rep.fame(condition.param_1)
    } else {
        rep.infamy(condition.param_1)
    };
    f32::from(points)
}

pub(super) fn get_reputation_threshold(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    // GetReputationThreshold(repu_form_id, axis) → the Range 0..=3 band
    // the chosen axis falls into for `param_1`'s thresholds. Uses the
    // vanilla FNV threshold table as the fallback source until the live
    // faction/REPU record path supplies per-faction thresholds; an
    // unknown faction yields Range 0 (the safe "Neutral" default).
    use byroredux_core::character::reputation::fnv_faction_thresholds;
    use byroredux_core::character::FactionReputation;
    let Some(rep) = world.get::<FactionReputation>(entity) else {
        return 0.0;
    };
    let points = if condition.param_2 == 1 {
        rep.fame(condition.param_1)
    } else {
        rep.infamy(condition.param_1)
    };
    fnv_faction_thresholds::thresholds_for(condition.param_1)
        .map_or(0.0, |t| f32::from(t.range(u32::from(points))))
}

/// `GetIsVoiceType` (CTDA 426 Skyrim / FO4, 427 FO3 / FNV; #5750): whether the
/// actor's voice type is `param_1`, or a member of it when `param_1` is a
/// `FLST` of voice types — the form `DialogueGenericHello`'s 124 conditions use
/// (`DefaultNPCVoiceTypes`, `GuardVoiceTypes`, ...). 0.0 for an actor with no
/// voice type. The voice is the leveled base's `VTCK` (#6228), the same lookup
/// the dialogue voice resolver and `ActorBase.GetVoiceType` use.
pub(super) fn get_is_voice_type(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    let Some(voice) = crate::actor_base_traits::voice_type(world, entity) else {
        return 0.0;
    };
    let wanted =
        crate::form_lists::forms_of(world, condition.param_1, &[]).unwrap_or_else(|| vec![condition.param_1]);
    if wanted.contains(&voice) {
        1.0
    } else {
        0.0
    }
}

/// `IsCommandedActor` (CTDA 700 Skyrim / FO4; #5750): whether the actor was
/// raised as a zombie or summoned by a summoning spell. The engine has no
/// summon or reanimate mechanic — nothing can make an actor commanded — so the
/// answer is always 0.0. When one exists, its marker is read here.
pub(super) fn is_commanded_actor(_entity: EntityId, _world: &World) -> f32 {
    0.0
}

/// `GetItemCount` (CTDA 47, ObScript `0x102f`): how many of `item` the
/// container's inventory holds. A form list counts every one of its members.
/// The one implementation the condition and the legacy getter share (#6251).
pub(crate) fn item_count(world: &World, container: EntityId, item: u32) -> f32 {
    use byroredux_core::ecs::components::Inventory;
    let items = crate::form_lists::forms_of(world, item, &[]).unwrap_or_else(|| vec![item]);
    let Some(inventory) = world.get::<Inventory>(container) else {
        return 0.0;
    };
    inventory.count_of_any(&items) as f32
}

/// `GetHealthPercentage` (FO3/FNV CTDA 431, ObScript `0x11af`): GECK "how
/// injured an actor is ... a value from 0 to 1.0" — current Health over the
/// undamaged ceiling. `None` for an actor with no Health stat. Shared by the
/// condition and the legacy getter (#6251).
pub(crate) fn health_percentage(world: &World, actor: EntityId) -> Option<f32> {
    use super::{actor_value_view, ActorValueView};
    use byroredux_core::ecs::components::ActorVitals;
    let health = world.get::<ActorVitals>(actor).map(|vitals| vitals.health)?;
    let current = actor_value_view(world, actor, health, ActorValueView::Current);
    let ceiling = actor_value_view(world, actor, health, ActorValueView::Undamaged);
    Some(if ceiling > 0.0 { (current / ceiling).clamp(0.0, 1.0) } else { 0.0 })
}
