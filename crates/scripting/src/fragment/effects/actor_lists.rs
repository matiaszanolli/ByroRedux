//! Perk, faction, spell and racial-spell list changes. (#6142, split from `apply_effect`)

use super::*;

/// `ChangePerk`, `ChangeFaction`, `AddSpell`/`RemoveSpell` and `AddRaceSpells`/`RemoveRaceSpells`.
///
/// #6142 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays named in
/// `apply_effect`'s nested-lock residual list, which the #3949 scan reads
/// across these helpers too.
pub(super) fn apply_actor_lists_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::ChangePerk { actor, perk, add } => {
            let actor = resolve_actor(
                vmad,
                world,
                context,
                actor,
                &deferred.scene_actor_bindings,
                deferred.speaker,
            )?;
            let perk = resolve_property_form_id(vmad, perk.property_name())?;
            // #6619 — released before `Perks` is taken.
            let rank_cap = world
                .try_resource::<crate::perk_entry_points::PerkEntryTable>()
                .and_then(|table| table.add_perk_rank_cap(perk));
            // `Perks` is direct component storage, written in place like
            // `ActorValues` above.
            let mut perks = world.query_mut::<byroredux_core::character::Perks>()?;
            if perks.get_mut(actor).is_none() {
                perks.insert(actor, byroredux_core::character::Perks::default());
            }
            let owned = perks.get_mut(actor).expect("present or inserted");
            if !*add {
                owned.remove(perk);
            } else {
                let rank = owned.rank(perk);
                if rank == 0 {
                    owned.set_rank(perk, 1);
                } else if let Some(cap) = rank_cap {
                    // A perk already at Num Ranks is rejected and keeps its
                    // rank, which is the cap the sources' rule implies.
                    let _ = owned.try_set_rank(perk, rank.saturating_add(1), cap);
                }
            }
            None
        }
        Effect::ChangeFaction {
            actor,
            faction,
            change,
        } => {
            let actor = resolve_actor(
                vmad,
                world,
                context,
                actor,
                &deferred.scene_actor_bindings,
                deferred.speaker,
            )?;
            let faction = match change {
                crate::FactionChange::RemoveAll => 0,
                _ => resolve_property_form_id(vmad, faction.property_name())?,
            };
            deferred.faction_changes.push((actor, faction, *change));
            None
        }
        Effect::AddSpell { actor, spell } | Effect::RemoveSpell { actor, spell } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            let spell = resolve_property_form_id(vmad, spell.property_name())?;
            let adding = matches!(effect, Effect::AddSpell { .. });
            let changed = if adding {
                crate::magic::add_spell(world, actor, spell)
            } else {
                crate::magic::remove_spell(world, actor, spell)
            };
            // #4822 — never drop the result silently. `magic` itself warns
            // when the actor has no `SpellList` (a lost effect); what is left
            // here is Papyrus' own idempotent no-op.
            if !changed {
                let verb = if adding { "AddSpell" } else { "RemoveSpell" };
                log::debug!(
                    "quest {context:?}: {verb}({spell:08X}) on entity {actor} changed nothing"
                );
            }
            None
        }
        // #4415 — the script-defined racial pair. The racial set was
        // level-resolved and stamped as `RaceSpells` at spawn, so the apply
        // needs no index; each member goes through the same add/remove the
        // scripted `AddSpell` uses (constant modifiers included).
        Effect::AddRaceSpells { actor } | Effect::RemoveRaceSpells { actor } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            let adding = matches!(effect, Effect::AddRaceSpells { .. });
            let changed = if adding {
                crate::magic::add_race_spells(world, actor)
            } else {
                crate::magic::remove_race_spells(world, actor)
            };
            if !changed {
                let verb = if adding {
                    "AddRaceSpells"
                } else {
                    "RemoveRaceSpells"
                };
                log::debug!("quest {context:?}: {verb} on entity {actor} changed nothing");
            }
            None
        }
        _ => unreachable!("`apply_effect` only routes actor lists effects here"),
    }
}
