//! The `Ability` and `Quest` entries of a perk (docs/prd/perk-entry-points.md §5.5, PERK-BODY).
//!
//! Neither is a value on a hook, so neither goes through composition. Both follow the actor's [`Perks`]:
//!
//! * **Ability.** While an actor holds a perk at the rank an `Ability` entry is authored for, the entry's
//!   spell is in its [`SpellList`]; it leaves with the perk or the rank (PERK-BODY-001, CK *Perk*:
//!   "Spells and Abilities added in this way will be removed when the Perk is removed"). The spell is
//!   *derived* from `Perks` (§10 Q7): [`reconcile_perk_bodies`] runs `add_spell` / `remove_spell` for the
//!   difference whenever an actor's perks differ from what it last saw, so the spell follows every path
//!   that adds or removes a perk (`levelup perk`, a script's `AddPerk`, an NPC's `PRKR`, a save load) and a
//!   reload — whose saved `SpellList` and `ActorValues` already carry the spell and its value changes —
//!   adds nothing twice, because `add_spell` is idempotent (PERK-BODY-002).
//! * **Quest.** Gaining the rank a `Quest` entry is authored for starts the quest and sets the entry's
//!   stage through the effect runtime a fragment or an ObScript command uses (PERK-BODY-003). It fires on
//!   *acquisition* of that rank, per entry: vanilla `IntenseTraining` authors one entry per rank, each with
//!   its own stage of the same quest (FalloutNV.esm 101..=110, `perk_body_census`), which only reads as
//!   one stage per rank gained. It is the player's: CK *Perk* describes a perk the player takes, and an
//!   NPC that spawns holding a perk has not just acquired it.
//!
//! An actor seen for the first time is a baseline, not an acquisition: a loaded save, a spawned NPC or a
//! starting perk set must not re-fire the quests their perks already set.

use byroredux_core::character::{PerkRank, Perks};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::ecs::Component;

use super::{entry_applies_at, tabs_pass, PerkBodyEntry, PerkBodyKind, PerkEntryTable};
use crate::magic::{add_spell, remove_spell};
use crate::quest_stages::QuestFormId;
use crate::translate::compose::QuestRef;
use crate::translate::effects::Effect;

/// What [`reconcile_perk_bodies`] last saw of one actor: its perks, and the ability spells it put on the
/// actor for them. `NOT_SAVED_BY_DESIGN`: a reload re-derives it from the saved `Perks` — the spells are
/// already in the saved `SpellList`, and the baseline pass finds them there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PerkBodyState {
    held: Vec<PerkRank>,
    granted: Vec<u32>,
}

impl Component for PerkBodyState {
    type Storage = SparseSetStorage<Self>;
}

impl PerkBodyState {
    /// The ability spells the last reconcile granted for this actor's perks.
    pub fn granted(&self) -> &[u32] {
        &self.granted
    }
}

/// The ability spells `held` entitles an actor to, in perk order, each once.
pub(crate) fn ability_spells(table: &PerkEntryTable, held: &[PerkRank]) -> Vec<u32> {
    let mut spells = Vec::new();
    for perk in held {
        let Some(entries) = table.bodies(perk.perk_form_id) else {
            continue;
        };
        for entry in entries {
            if let PerkBodyKind::Ability { spell } = entry.kind {
                if entry_applies_at(entry.rank, perk.rank) && !spells.contains(&spell) {
                    spells.push(spell);
                }
            }
        }
    }
    spells
}

/// The `Quest` entries a change from `old` to `new` acquires: for each perk whose rank rose, the entries
/// authored for the new rank (and not for the old one).
pub(crate) fn quest_entries_gained(table: &PerkEntryTable, old: &[PerkRank], new: &[PerkRank]) -> Vec<PerkBodyEntry> {
    let mut gained = Vec::new();
    for perk in new {
        let before = old.iter().find(|o| o.perk_form_id == perk.perk_form_id).map_or(0, |o| o.rank);
        if perk.rank <= before {
            continue;
        }
        let Some(entries) = table.bodies(perk.perk_form_id) else {
            continue;
        };
        for entry in entries {
            if matches!(entry.kind, PerkBodyKind::Quest { .. })
                && entry_applies_at(entry.rank, perk.rank)
                && !entry_applies_at(entry.rank, before)
            {
                gained.push(entry.clone());
            }
        }
    }
    gained
}

/// Bring every actor's perk bodies in line with its [`Perks`]. Cheap when nothing changed: one snapshot
/// compare per perk-holding actor. Exclusive — it applies quest effects.
pub fn reconcile_perk_bodies(world: &World) {
    // Snapshot first; every guard is gone before a spell or a quest effect takes its own locks.
    let actors: Vec<(EntityId, Vec<PerkRank>)> = match world.query::<Perks>() {
        Some(perks) => perks.iter().map(|(entity, owned)| (entity, owned.entries.clone())).collect(),
        None => return,
    };
    if actors.is_empty() {
        return;
    }
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
    for (actor, held) in actors {
        let before = world.get::<PerkBodyState>(actor).map(|state| state.clone());
        if before.as_ref().is_some_and(|state| state.held == held) {
            continue;
        }
        // The table guard lives for the two derivations alone.
        let (desired, gained) = {
            let Some(table) = world.try_resource::<PerkEntryTable>() else {
                // No table installed (no plugin loaded): nothing to derive, and nothing to remember.
                return;
            };
            let gained = match &before {
                Some(state) if Some(actor) == player => quest_entries_gained(&table, &state.held, &held),
                _ => Vec::new(),
            };
            (ability_spells(&table, &held), gained)
        };
        let previous = before.as_ref().map(|state| state.granted.clone()).unwrap_or_default();
        for spell in previous.iter().filter(|spell| !desired.contains(spell)) {
            remove_spell(world, actor, *spell);
        }
        for spell in &desired {
            add_spell(world, actor, *spell);
        }
        if let Some(mut states) = world.query_mut::<PerkBodyState>() {
            let state = PerkBodyState { held, granted: desired };
            if let Some(slot) = states.get_mut(actor) {
                *slot = state;
            } else {
                states.insert(actor, state);
            }
        }
        for entry in gained {
            run_quest_entry(world, actor, &entry);
        }
    }
}

/// The scheduled form of [`reconcile_perk_bodies`].
pub fn perk_body_system(world: &World, _dt: f32) {
    reconcile_perk_bodies(world);
}

/// Start the entry's quest and set its stage, as a fragment's `StartQuest` + `SetStage` would.
fn run_quest_entry(world: &World, actor: EntityId, entry: &PerkBodyEntry) {
    let PerkBodyKind::Quest { quest, stage } = entry.kind else {
        return;
    };
    if !tabs_pass(world, actor, &entry.tabs, &[]) {
        return;
    }
    let effects = [
        Effect::StartQuest { quest: QuestRef::SelfRef },
        Effect::SetStage { quest: QuestRef::SelfRef, stage },
    ];
    let advances = crate::fragment::apply_fragment_guard_free(world, &effects, QuestFormId(quest), None);
    log::info!("perk entry: quest {quest:08X} started and set to stage {stage}");
    crate::fragment::flush_advances(world, advances);
}
