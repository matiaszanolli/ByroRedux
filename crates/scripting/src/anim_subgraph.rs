//! The consumer of `Actor.ChangeAnimArchetype` (M47.2): an animation archetype
//! is an *actor keyword*, and the actor's RACE subgraph data picks its
//! behavior graph by keyword.
//!
//! CK *ChangeAnimArchetype*: "Adds the specified AnimArchetype keyword to this
//! actor", and with no argument "removes any existing AnimArchetype keyword
//! from the actor (sets actor back to 'Neutral')". CK *Subgraphs*: a RACE
//! "Subgraph Data" entry lists *Actor Keywords* that "need to exist on the Actor
//! for this sub graph data to get picked"; "The game will always choose the
//! subgraph data that matches the maximum number of actor keywords". "'Female'
//! is a special actor keyword automatically added based on the combination of
//! Actor's gender & 'opposite gender anims' checkbox" (ACBS bits 0 and 19,
//! xEdit "Female" / "Swap Gender Anims").
//!
//! So two things consume the recorded archetype:
//!
//! * [`crate::keywords::has_keyword`] — while an override exists, the actor has
//!   exactly the override among the archetype keywords (its authored one is
//!   shadowed; `0` is Neutral and leaves none);
//! * [`select_subgraph`] — the entry of a role a graph runtime attaches.
//!
//! Archetype keywords are the `KYWD`s whose editor id starts `AnimArchetype`
//! (Fallout4.esm names ten, with the FormIDs the CK page lists; no NPC_ or RACE
//! authors more than one).

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::{EsmIndex, SubgraphData, SubgraphRole};

/// Subgraph entries per race, the archetype keywords, and what the `Female`
/// keyword needs. `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone)]
pub struct SubgraphTable {
    /// Race → its effective entries (own, then its `SRAC` template's, then those
    /// of races that are `SADD`-additive onto it).
    races: HashMap<u32, Vec<SubgraphData>>,
    /// Base `NPC_` → (race, uses female animations).
    npcs: HashMap<u32, (u32, bool)>,
    archetypes: HashSet<u32>,
    female_keyword: Option<u32>,
}

impl Resource for SubgraphTable {}

/// The prefix of the archetype keywords' editor ids.
const ARCHETYPE_PREFIX: &str = "animarchetype";

impl SubgraphTable {
    pub fn from_index(index: &EsmIndex) -> Self {
        let archetypes = index
            .keywords
            .values()
            .filter(|keyword| keyword.editor_id.to_ascii_lowercase().starts_with(ARCHETYPE_PREFIX))
            .map(|keyword| keyword.form_id)
            .collect();
        let female_keyword = index
            .keywords
            .values()
            .find(|keyword| keyword.editor_id.eq_ignore_ascii_case("Female"))
            .map(|keyword| keyword.form_id);
        let mut races = HashMap::new();
        for race in index.races.values() {
            let entries = effective_entries(index, race.form_id);
            if !entries.is_empty() {
                races.insert(race.form_id, entries);
            }
        }
        const FEMALE: u32 = 1 << 0;
        const SWAP_GENDER_ANIMS: u32 = 1 << 19;
        let npcs = index
            .npcs
            .values()
            .map(|npc| {
                // #6228 — race and the sex flags are Use-Traits fields: the shell's own are not the body's.
                let traits = byroredux_plugin::equip::traits_terminal(npc, index);
                let female = (traits.acbs_flags & FEMALE != 0) != (traits.acbs_flags & SWAP_GENDER_ANIMS != 0);
                (npc.form_id, (traits.race_form_id, female))
            })
            .collect();
        Self { races, npcs, archetypes, female_keyword }
    }

    /// Whether `keyword` is an `AnimArchetype…` keyword.
    pub fn is_archetype(&self, keyword: u32) -> bool {
        self.archetypes.contains(&keyword)
    }

    /// The `AnimArchetype…` keyword FormIDs, in no particular order.
    pub fn archetype_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.archetypes.iter().copied()
    }

    pub fn archetype_count(&self) -> usize {
        self.archetypes.len()
    }

    /// Every race's third-person entry for `role` (the same entry repeats across
    /// the races that share it), for a host that pre-resolves what the entries'
    /// graphs play.
    pub fn entries_for(&self, role: SubgraphRole) -> impl Iterator<Item = &SubgraphData> {
        self.races
            .values()
            .flatten()
            .filter(move |entry| entry.role == role && !entry.first_person)
    }
}

/// A race's own entries, its template's, and the additive races' onto it.
fn effective_entries(index: &EsmIndex, race: u32) -> Vec<SubgraphData> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut next = Some(race);
    // The template chain (a bounded walk: a cycle in a mod cannot hang it).
    while let Some(current) = next.filter(|id| seen.insert(*id)) {
        let Some(record) = index.races.get(&current) else { break };
        out.extend(record.subgraph_data.iter().cloned());
        next = record.subgraph_template;
    }
    for additive in index.races.values().filter(|candidate| candidate.subgraph_additive == Some(race)) {
        out.extend(additive.subgraph_data.iter().cloned());
    }
    out
}

/// Install the load order's [`SubgraphTable`]; returns how many archetype
/// keywords it names.
pub fn install_subgraph_table(world: &mut World, index: &EsmIndex) -> usize {
    let table = SubgraphTable::from_index(index);
    let count = table.archetype_count();
    world.insert_resource(table);
    count
}

/// The archetype keyword a script's [`crate::AnimArchetypeOverrides`] gave
/// `key` when `keyword` is an archetype keyword: `Some(true)` for the chosen
/// one, `Some(false)` for any other, `None` when no override applies.
pub(crate) fn archetype_override_has(world: &World, key: u32, keyword: u32) -> Option<bool> {
    let chosen = world.try_resource::<crate::AnimArchetypeOverrides>()?.get(key)?;
    if keyword == chosen {
        return Some(chosen != 0);
    }
    let is_archetype = world
        .try_resource::<SubgraphTable>()
        .is_some_and(|table| table.is_archetype(keyword));
    is_archetype.then_some(false)
}

/// The subgraph entry `entity`'s graph runs for `role`, third person: among the
/// entries whose actor keywords the actor all has (and, for a role with a
/// target, whose target keywords `target` has), the one with the most actor
/// keywords — the first authored on a tie, which the CK calls undetermined.
pub fn select_subgraph(
    world: &World,
    entity: EntityId,
    role: SubgraphRole,
    target: &[u32],
) -> Option<SubgraphData> {
    let base = crate::relationships::actor_base(world, entity)?;
    let table = world.try_resource::<SubgraphTable>()?;
    let (race, female) = *table.npcs.get(&base)?;
    let entries = table.races.get(&race)?;
    let has = |keyword: u32| {
        if Some(keyword) == table.female_keyword {
            female
        } else {
            crate::keywords::has_keyword(world, entity, keyword)
        }
    };
    let mut best: Option<&SubgraphData> = None;
    for entry in entries {
        if entry.role != role
            || entry.first_person
            || !entry.actor_keywords.iter().all(|&keyword| has(keyword))
            || !entry.target_keywords.iter().all(|keyword| target.contains(keyword))
        {
            continue;
        }
        let better = best.is_none_or(|current| {
            (entry.actor_keywords.len(), entry.target_keywords.len())
                > (current.actor_keywords.len(), current.target_keywords.len())
        });
        if better {
            best = Some(entry);
        }
    }
    best.cloned()
}

#[cfg(test)]
mod tests;
