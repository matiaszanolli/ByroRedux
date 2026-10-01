//! P4 blocker 1 — NPC activation → topic selection (MS01 fixture).
//!
//! The authored NPC→topic edge is `DialRecord::quest_refs` (QSTI): an NPC
//! "owns" the dialogue topics of every *running* quest whose live alias
//! bindings name it. This system consumes the player's `ActivateEvent` on
//! such an NPC, walks that edge, and picks the first INFO the M47.1
//! evaluator passes — the same `select_info` discipline the SCEN dialogue
//! path uses, with the activated actor as subject and the player as target.
//!
//! #5037 — which owned topics reach the conversation, per the Creation
//! Kit's branch model ("Bethesda Tutorial Advanced Dialogue"):
//!
//! - Only player topics (`DialogueCategory::Topic`); Scene, Combat and
//!   Miscellaneous topics are barks and scene lines, not prompts.
//! - Only topics with an INFO that passes for *this* NPC. That is the
//!   speaker filter: an INFO's speaker and conditions are what the evaluator
//!   checks, so another speaker's topics drop out.
//! - A **Blocking** branch whose starting topic qualifies is the only thing
//!   the NPC talks about. Its line opens the conversation, and the topics its
//!   INFO links to (`TCLT`) replace the list.
//! - Otherwise the list is the **Top-Level** branches' starting topics. The
//!   NPC's Hello greeting is still unmodeled, so the first entry (lowest
//!   form id) stands in as the opening line.
//! - A topic inside a branch that is not its starting topic, or the start of
//!   a branch with neither flag, is reached only through a link.
//! - Oblivion / FO3 / FNV author no branches, so their topics are all list
//!   entries (FO3/FNV's DIAL `Top-level` flag is not decoded).
//!
//! Deliberate scope (per the fixture's no-speculative-breadth rule):
//!
//! - **Running-quest topics only.** There is no authored generic greeting
//!   scan; a patron bound to no running quest selects nothing. Race/faction
//!   /hostile-scene greetings wait for a fixture that trips on them.
//! - **Selection, not presentation.** The chosen topic lands as
//!   [`NpcDialogueTopic`] on the NPC (and into the
//!   [`byroredux_scripting::DialogueRegistry`] so the presentation side
//!   finds it); the native response surface is the next blocker.
//!
//! Registered `Stage::Late`: the Update-stage interaction system emits
//! `ActivateEvent`, and every other activation consumer already reads it
//! after that point, before the end-of-Late cleanup drains the marker.

use byroredux_core::ecs::components::Dead;
use byroredux_core::ecs::{Component, EntityId, SparseSetStorage, World};
use byroredux_plugin::esm::records::{DialRecord, DialogueCategory, EsmIndex, InfoRecord};
use byroredux_plugin::esm::records::script_instance::ScriptInstanceData;
use byroredux_scripting::{
    running_quests_binding_entity, select_first_info, ActivateEvent, AiCombatState,
    DialogueInfoFragments, DialogueRegistry, Effect, QuestFormId, SceneAliasCandidate,
};
use rustc_hash::FxHashSet;

use crate::cell_loader::LoadedCellIndex;
use crate::systems::PlayerEntity;

/// One selectable entry in the NPC's owned-topic list — the DIALs of the
/// running quests that bind the NPC, captured at selection time so the
/// response surface reads one component instead of walking the index
/// per frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DialogueTopicEntry {
    pub(crate) topic_form_id: u32,
    pub(crate) name: String,
}

/// The activation-driven topic selection on an NPC. At most one entity
/// carries it: [`apply_selection`] strips it from every other NPC before
/// stamping the newly selected one (#5038). Runtime interaction state, never serialized
/// (it re-derives from the authored records + running quests on the next
/// activation; the same posture as `InteractionTrace`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NpcDialogueTopic {
    pub(crate) topic_form_id: u32,
    pub(crate) topic_editor_id: String,
    pub(crate) info_form_id: u32,
    pub(crate) owning_quest: Option<u32>,
    pub(crate) speaker_text: String,
    pub(crate) response_number: u8,
    pub(crate) emotion_type: u8,
    /// The NPC's whole owned-topic list, the selected one included — the
    /// response surface's list column.
    pub(crate) topics: Vec<DialogueTopicEntry>,
}

impl Component for NpcDialogueTopic {
    type Storage = SparseSetStorage<Self>;
}

/// The response surface's open-once-per-selection cue. Every applied
/// selection (activation- or UI-driven) bumps `serial` and names the NPC;
/// the app layer opens the native dialogue page when it sees a serial it
/// has not opened yet. `opened_serial` is the app side's watermark, not
/// gameplay state — the whole resource is runtime plumbing, never
/// serialized.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DialogueSurfaceState {
    pub(crate) serial: u64,
    pub(crate) npc: Option<EntityId>,
    pub(crate) opened_serial: u64,
}

impl byroredux_core::ecs::Resource for DialogueSurfaceState {}

/// The player topics an NPC owns through `owned_quests` (the running quests
/// whose alias bindings name it), ascending by form id — the authored
/// NPC→topic edge, gathered once per selection. #5037 — Scene, Combat and
/// Miscellaneous topics are not prompts and never reach the list.
fn owned_topic_records<'a>(
    index: &'a EsmIndex,
    owned_quests: &[byroredux_scripting::QuestFormId],
) -> Vec<&'a DialRecord> {
    let mut topics: Vec<&DialRecord> = index
        .dialogues
        .values()
        .filter(|record| record.category == DialogueCategory::Topic)
        .filter(|record| {
            owned_quests
                .iter()
                .any(|quest| record.quest_refs.contains(&quest.0))
        })
        .collect();
    topics.sort_unstable_by_key(|record| record.form_id);
    topics
}

/// How a topic is reached (#5037), from its `DLBR` branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopicEntry {
    /// A Blocking branch's starting topic: pre-empts the list when it passes.
    Blocking,
    /// A list entry: a Top-Level branch's starting topic, or a topic with no
    /// branch (Oblivion–FNV, or a branch the loaded plugins do not define).
    TopLevel,
    /// Reached only through another INFO's `TCLT` link.
    LinkOnly,
}

fn topic_entry(index: &EsmIndex, record: &DialRecord) -> TopicEntry {
    let Some(branch) = record
        .branch
        .and_then(|branch| index.dialogue_branches.get(&branch))
    else {
        return TopicEntry::TopLevel;
    };
    if branch.starting_topic != record.form_id {
        TopicEntry::LinkOnly
    } else if branch.blocking() {
        TopicEntry::Blocking
    } else if branch.top_level() {
        TopicEntry::TopLevel
    } else {
        TopicEntry::LinkOnly
    }
}

/// The NPC's topic list outside a blocking branch: every owned Top-Level
/// entry with an INFO that passes for this NPC.
fn top_level_menu<'a>(
    index: &EsmIndex,
    owned: &[&'a DialRecord],
    world: &World,
    npc: EntityId,
    player: EntityId,
) -> Vec<&'a DialRecord> {
    owned
        .iter()
        .copied()
        .filter(|record| topic_entry(index, record) == TopicEntry::TopLevel)
        .filter(|record| select_first_info(record, world, Some(npc), Some(player)).is_some())
        .collect()
}

/// The topics `info` links to (`TCLT`) that pass for this NPC — what
/// replaces the list once that line is spoken.
fn linked_menu<'a>(
    index: &'a EsmIndex,
    info: &InfoRecord,
    world: &World,
    npc: EntityId,
    player: EntityId,
) -> Vec<&'a DialRecord> {
    info.topic_links
        .iter()
        .filter_map(|topic| index.dialogues.get(topic))
        .filter(|record| select_first_info(record, world, Some(npc), Some(player)).is_some())
        .collect()
}

fn topic_entries(records: &[&DialRecord]) -> Vec<DialogueTopicEntry> {
    records
        .iter()
        .map(|record| DialogueTopicEntry {
            topic_form_id: record.form_id,
            name: if record.full_name.is_empty() {
                record.editor_id.clone()
            } else {
                record.full_name.clone()
            },
        })
        .collect()
}

/// Speak `record`'s first passing INFO and package it with the list that
/// follows it: the INFO's passing links, or `fallback_menu` when it links
/// nowhere. An empty list still shows the spoken topic.
fn select_on_topic(
    index: &EsmIndex,
    record: &DialRecord,
    world: &World,
    npc: EntityId,
    player: EntityId,
    fallback_menu: &[&DialRecord],
) -> Option<(NpcDialogueTopic, DialRecord)> {
    let info = select_first_info(record, world, Some(npc), Some(player))?;
    let linked = linked_menu(index, info, world, npc, player);
    let menu: &[&DialRecord] = if !linked.is_empty() {
        &linked
    } else if !fallback_menu.is_empty() {
        fallback_menu
    } else {
        std::slice::from_ref(&record)
    };
    Some((
        NpcDialogueTopic {
            topic_form_id: record.form_id,
            topic_editor_id: record.editor_id.clone(),
            info_form_id: info.form_id,
            owning_quest: record.quest_refs.first().copied(),
            speaker_text: info.response_text.clone(),
            response_number: info.response_number,
            emotion_type: info.emotion_type,
            topics: topic_entries(menu),
        },
        record.clone(),
    ))
}

/// The activation's opening selection (#5037): a qualifying Blocking entry
/// pre-empts everything and its links become the list; otherwise the first
/// Top-Level entry opens the Top-Level list.
fn open_conversation(
    index: &EsmIndex,
    owned: &[&DialRecord],
    world: &World,
    npc: EntityId,
    player: EntityId,
) -> Option<(NpcDialogueTopic, DialRecord)> {
    let blocking = owned.iter().copied().find(|record| {
        topic_entry(index, record) == TopicEntry::Blocking
            && select_first_info(record, world, Some(npc), Some(player)).is_some()
    });
    if let Some(record) = blocking {
        // Blocking: the list is whatever links from this line, never the
        // normal topics.
        return select_on_topic(index, record, world, npc, player, &[]);
    }
    let menu = top_level_menu(index, owned, world, npc, player);
    let first = *menu.first()?;
    select_on_topic(index, first, world, npc, player, &menu)
}

/// Apply one selection: registry install (the presentation side's record
/// source), the component stamp, the surface-serial bump, and — #5152 — the
/// spoken line's fragments: the outgoing line's OnEnd (the line stops being
/// spoken when another replaces it), then the selected line's OnBegin.
/// Shared by the activation path and the UI's re-selection door.
fn apply_selection(world: &World, npc: EntityId, topic: NpcDialogueTopic, record: DialRecord) {
    // The outgoing line ends before the new one begins (vanilla's OnEnd
    // ordering). One live selection (#5038) means every existing stamp IS
    // the outgoing line.
    let outgoing: Vec<NpcDialogueTopic> = world
        .query::<NpcDialogueTopic>()
        .map(|topics| {
            topics
                .iter()
                .map(|(_, topic)| topic.clone())
                .filter(|existing| existing.info_form_id != topic.info_form_id)
                .collect()
        })
        .unwrap_or_default();
    for existing in &outgoing {
        speak_info_end_fragment(world, existing);
    }
    if let Some(mut registry) = world.try_resource_mut::<DialogueRegistry>() {
        registry.insert_topic(record);
    }
    if let Some(mut topics) = world.query_mut::<NpcDialogueTopic>() {
        // #5038 — one live selection. A previous conversation partner's stamp
        // would otherwise outlive the conversation until despawn, and any
        // reader that does not key on `DialogueSurfaceState::npc` would see
        // two selections.
        let stale: Vec<EntityId> = topics
            .iter()
            .map(|(entity, _)| entity)
            .filter(|entity| *entity != npc)
            .collect();
        for entity in stale {
            topics.remove(entity);
        }
        topics.insert(npc, topic.clone());
    }
    if let Some(mut surface) = world.try_resource_mut::<DialogueSurfaceState>() {
        surface.serial += 1;
        surface.npc = Some(npc);
    }
    log::info!(
        "npc dialogue: selected topic {:#08X} ('{}') info {:#08X} for NPC {npc} (quest {:?})",
        topic.topic_form_id,
        topic.topic_editor_id,
        topic.info_form_id,
        topic.owning_quest,
    );
    speak_info_begin_fragment(world, &topic);
}

/// #5152 — run the selected INFO's OnBegin fragment (the TIF_ topic-info
/// script the CK attaches to the line). This is the dialogue loop's stage
/// engine: the fixture's Eltrys entry line lowers to
/// `GetOwningQuest().SetStage(..)`, which journals the transition for
/// `quest_fragment_dispatch_system` (Update stage — next frame) to run the
/// stage fragment that displays the next objective.
///
/// Same execution unit as the scene dispatcher: `apply_fragment_guard_free`
/// snapshots + flushes the deferred effects, and any direct stage advances
/// ride the shared player sink (`push_quest_stage_advances`) so the cascade
/// and the journal stay interleaved with every other producer. A no-op when
/// the populate walk never filled the table (no `--scripts-bsa`, pre-Papyrus
/// game) or the line has no OnBegin binding — most INFO dialogue is inert
/// flavor, and 3 773 of vanilla's 5 257 bound INFOs are OnEnd-only.
fn speak_info_begin_fragment(world: &World, topic: &NpcDialogueTopic) {
    let Some((effects, context, vmad)) = spoken_fragment_effects(world, topic.info_form_id, true)
    else {
        return;
    };
    dispatch_spoken_fragment(world, &effects, context, vmad.as_ref());
}

/// The outgoing line's OnEnd fragment — "the line is done". Fires when
/// another selection replaces it ([`apply_selection`]) and when the
/// conversation surface closes ([`end_open_conversation`]).
fn speak_info_end_fragment(world: &World, topic: &NpcDialogueTopic) {
    let Some((effects, context, vmad)) = spoken_fragment_effects(world, topic.info_form_id, false)
    else {
        return;
    };
    dispatch_spoken_fragment(world, &effects, context, vmad.as_ref());
}

/// One resource read for the fragment binding + context + property table.
/// `begin = true` picks the OnBegin binding, `false` the OnEnd one; `None`
/// when the table has no entry or the line carries no such binding.
fn spoken_fragment_effects(
    world: &World,
    info_form_id: u32,
    begin: bool,
) -> Option<(Vec<Effect>, QuestFormId, Option<ScriptInstanceData>)> {
    world
        .try_resource::<DialogueInfoFragments>()?
        .spoken_effects(info_form_id, begin)
}

fn dispatch_spoken_fragment(
    world: &World,
    effects: &[Effect],
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) {
    let advances = byroredux_scripting::apply_spoken_info_fragment(world, effects, context, vmad);
    if advances.is_empty() {
        return;
    }
    // #3580 — copy the entity out and drop the guard before the batch
    // storage acquisition (the scene dispatcher's lock-order note).
    let Some(player) = world
        .try_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
        .map(|player| player.0)
    else {
        return;
    };
    byroredux_scripting::quest_stages::push_quest_stage_advances(world, player, advances);
}

/// The conversation surface closed: the open line's OnEnd fragment runs
/// (the line stops being spoken), then the selection stamp and the surface
/// cue clear. Called from the shared resume path when the dialogue page is
/// the one closing — the same point the pause menu's Continue and the
/// dialogue page's Close button both reach.
pub(crate) fn end_open_conversation(world: &World) {
    let npc = match world.try_resource::<DialogueSurfaceState>() {
        Some(surface) => match surface.npc {
            Some(npc) => npc,
            None => return,
        },
        None => return,
    };
    if let Some(outgoing) = world.get::<NpcDialogueTopic>(npc).map(|topic| topic.clone()) {
        speak_info_end_fragment(world, &outgoing);
    }
    if let Some(mut topics) = world.query_mut::<NpcDialogueTopic>() {
        topics.remove(npc);
    }
    if let Some(mut surface) = world.try_resource_mut::<DialogueSurfaceState>() {
        surface.npc = None;
    }
}

/// One activation's computed selection, applied after all reads drop (the
/// shared two-pass convention of this crate's exclusive systems).
struct TopicSelection {
    npc: EntityId,
    topic: NpcDialogueTopic,
    record: DialRecord,
}

/// #5043 — whether `npc` is in no state to hold a conversation: dead (corpse
/// loot owns it), fighting (`AiCombatState`, against the player or anyone
/// else), or unconscious (#5017). The interaction Talk arm applies the same
/// three filters (in bulk, via [`collect_dialogue_refusals`]), so the prompt
/// and the selection agree.
pub(crate) fn npc_refuses_dialogue(world: &World, npc: EntityId) -> Option<&'static str> {
    if world.get::<Dead>(npc).is_some() {
        return Some("that actor is dead");
    }
    if world.get::<AiCombatState>(npc).is_some() {
        return Some("that actor is in combat");
    }
    // #5017 — "actors also cannot be talked to … in this state" (CK).
    if byroredux_scripting::is_unconscious(world, npc) {
        return Some("that actor is unconscious");
    }
    None
}

/// The bulk form of [`npc_refuses_dialogue`] for the per-frame candidate
/// scan (#5109): the three refusal states gathered as sets under one
/// storage guard each, so the HUD prompt path never re-locks four storages
/// per placement root per frame. The scalar form stays the authority for
/// the one-NPC activation path (it names the refusing state) — the two
/// must always check the same three conditions.
pub(crate) struct DialogueRefusals {
    dead: FxHashSet<EntityId>,
    fighting: FxHashSet<EntityId>,
    unconscious: FxHashSet<EntityId>,
}

impl DialogueRefusals {
    pub(crate) fn refuses(&self, npc: EntityId) -> bool {
        self.dead.contains(&npc)
            || self.fighting.contains(&npc)
            || self.unconscious.contains(&npc)
    }
}

pub(crate) fn collect_dialogue_refusals(world: &World) -> DialogueRefusals {
    let dead = world
        .query::<Dead>()
        .map(|query| query.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    let fighting = world
        .query::<AiCombatState>()
        .map(|query| query.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    // #5017's own bulk gather — one `ActorControlState` guard, reused
    // rather than re-implemented here.
    let mut unconscious = Vec::new();
    byroredux_scripting::collect_unconscious(world, &mut unconscious);
    DialogueRefusals {
        dead,
        fighting,
        unconscious: unconscious.into_iter().collect(),
    }
}

/// Reusable per-frame scratch (mirrors `WalkAnimScratch`'s shape).
#[derive(Default)]
struct NpcDialogueScratch {
    selections: Vec<TopicSelection>,
}

fn npc_dialogue_selection_system_inner(world: &World, scratch: &mut NpcDialogueScratch) {
    // ── Pass 1: read-only gather + decide. ──
    scratch.selections.clear();
    // #5043 / #4701 — a dead player drives no dialogue, the same gate the
    // combat, interaction and inventory entry points apply. A scripted
    // player activation reaches this system without the interaction gate.
    if !crate::systems::player_can_act(world) {
        return;
    }
    let Some(player) = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return;
    };
    let Some(index) = world.try_resource::<LoadedCellIndex>() else {
        return;
    };
    let index = index.0.clone();
    let events: Vec<(EntityId, EntityId)> = world
        .query::<ActivateEvent>()
        .map(|query| {
            query
                .iter()
                .map(|(target, event)| (target, event.activator))
                .collect()
        })
        .unwrap_or_default();
    if events.is_empty() {
        return;
    }
    for (npc, activator) in events {
        // The player activates; the player is never its own dialogue target;
        // the dead and the fighting are not conversation partners (#5043).
        if activator != player || npc == player || npc_refuses_dialogue(world, npc).is_some() {
            continue;
        }
        // The actor identity the alias fill stamped — also what INFO speaker
        // matching reads. No candidate identity, no authored dialogue route.
        if world.get::<SceneAliasCandidate>(npc).is_none() {
            continue;
        }
        let owned_quests = running_quests_binding_entity(world, npc);
        if owned_quests.is_empty() {
            continue;
        }
        // Deterministic order: the owned topics ascending by form id — the
        // same record the fixture route must stably select every run.
        let owned = owned_topic_records(&index, &owned_quests);
        let Some((topic, record)) = open_conversation(&index, &owned, world, npc, player) else {
            continue;
        };
        scratch.selections.push(TopicSelection {
            npc,
            topic,
            record,
        });
    }
    if scratch.selections.is_empty() {
        return;
    }

    // ── Pass 2: apply. ──
    for selection in &scratch.selections {
        apply_selection(world, selection.npc, selection.topic.clone(), selection.record.clone());
    }
}

/// The response surface's topic click, lowered through the same selection
/// the activation path uses — never a separate mutation path. The requested
/// topic must still be among the NPC's owned topics (authored ownership is
/// re-checked against the live bindings); the INFO gate is the evaluator.
/// Returns a user-facing message either way.
pub(crate) fn select_topic_by_form_id(
    world: &mut World,
    npc: EntityId,
    topic_form_id: u32,
) -> Result<String, String> {
    if npc == world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
        .unwrap_or_default()
    {
        return Err("the player is not a dialogue target".to_string());
    }
    // #5043 — a topic click on an already-open surface is a dialogue entry
    // point too.
    if !crate::systems::player_can_act(world) {
        return Err("the player cannot act".to_string());
    }
    if let Some(reason) = npc_refuses_dialogue(world, npc) {
        return Err(reason.to_string());
    }
    let Some(index) = world.try_resource::<LoadedCellIndex>() else {
        return Err("no loaded plugin index".to_string());
    };
    let index = index.0.clone();
    let Some(record) = index.dialogues.get(&topic_form_id).cloned() else {
        return Err(format!("no topic {topic_form_id:08X} in the loaded plugins"));
    };
    let owned_quests = running_quests_binding_entity(world, npc);
    let owned = owned_topic_records(&index, &owned_quests);
    if !owned.iter().any(|owned| owned.form_id == topic_form_id) {
        return Err(format!(
            "NPC {npc} owns no topic {topic_form_id:08X} through a running quest"
        ));
    }
    let player = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
        .ok_or_else(|| "no player".to_string())?;
    // After this line, its links become the list; a line that links nowhere
    // returns to the Top-Level topics (#5037).
    let menu = top_level_menu(&index, &owned, world, npc, player);
    let Some((topic, record)) = select_on_topic(&index, &record, world, npc, player, &menu)
    else {
        return Err(format!(
            "no INFO on topic {topic_form_id:08X} passes its conditions right now"
        ));
    };
    let message = format!(
        "dialogue: topic {:#08X} ('{}') → info {:#08X}",
        topic.topic_form_id, topic.topic_editor_id, topic.info_form_id,
    );
    apply_selection(world, npc, topic, record);
    Ok(message)
}

/// The response surface's snapshot — the plain-data twin the debug-ui
/// crate renders (it cannot see this crate's component types). `None`
/// when no NPC is selected or the selected NPC no longer carries a topic.
///
/// #5038 — keyed on [`DialogueSurfaceState::npc`], the NPC the last applied
/// selection named, never on whichever `NpcDialogueTopic` the sparse set
/// happens to yield first.
pub(crate) fn dialogue_snapshot(
    world: &World,
) -> Option<byroredux_debug_ui::DialogueTopicSnapshot> {
    let npc = world.try_resource::<DialogueSurfaceState>()?.npc?;
    let topic = world.get::<NpcDialogueTopic>(npc)?.clone();
    // Many quest DIALs ship no EDID; the selected entry's authored FULL (the
    // player-facing prompt) is the surface's header then.
    let topic_name = if topic.topic_editor_id.is_empty() {
        topic
            .topics
            .iter()
            .find(|entry| entry.topic_form_id == topic.topic_form_id)
            .map(|entry| entry.name.clone())
            .unwrap_or_default()
    } else {
        topic.topic_editor_id.clone()
    };
    Some(byroredux_debug_ui::DialogueTopicSnapshot {
        npc,
        topic_form_id: topic.topic_form_id,
        topic_name,
        info_form_id: topic.info_form_id,
        response_text: topic.speaker_text.clone(),
        topics: topic
            .topics
            .iter()
            .map(|entry| byroredux_debug_ui::DialogueTopicEntryView {
                topic_form_id: entry.topic_form_id,
                name: entry.name.clone(),
            })
            .collect(),
    })
}

/// Selection-system factory — a persistent [`NpcDialogueScratch`], mirroring
/// [`crate::systems::walk_anim::make_npc_walk_animation_system`]'s shape.
/// Wire with `add_exclusive_with_access(Stage::Late, …)`.
pub(crate) fn make_npc_dialogue_selection_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = NpcDialogueScratch::default();
    move |world: &World, _dt: f32| {
        npc_dialogue_selection_system_inner(world, &mut scratch);
    }
}

/// Kept for test ergonomics. Production uses
/// [`make_npc_dialogue_selection_system`].
#[cfg(test)]
pub(crate) fn npc_dialogue_selection_system(world: &World) {
    npc_dialogue_selection_system_inner(world, &mut NpcDialogueScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::InfoRecord;
    use byroredux_plugin::esm::records::{AliasFillType, QuestAlias, QustRecord};
    use byroredux_scripting::quest_stages::QuestStageState;
    use byroredux_scripting::{
        install_scene_quest_aliases, refresh_scene_actor_bindings, QuestFormId,
    };
    use std::sync::Arc;

    /// The fixture quest (MS01's FormID) and two Eltrys-shaped speakers:
    /// one whose identity the speaker-locked INFO names, one served by the
    /// shared branch.
    const QUEST: u32 = 0x0001_8B4B;
    const SPEAKER_REF: u32 = 0xA0_0001;
    const SPEAKER_BASE: u32 = 0x20_0001;
    const OTHER_REF: u32 = 0xA0_0002;
    const TOPIC: u32 = 0x20_0010;
    const INFO_ELTRYS: u32 = 0x20_0011;
    const INFO_GENERIC: u32 = 0x20_0012;
    const TOPIC_RUMORS: u32 = 0x20_0020;
    const INFO_RUMORS: u32 = 0x20_0021;

    fn info(form_id: u32, speaker: u32, text: &str) -> InfoRecord {
        InfoRecord {
            form_id,
            response_text: text.to_string(),
            actor_form_id: speaker,
            ..Default::default()
        }
    }

    fn fixture_topic() -> DialRecord {
        DialRecord {
            form_id: TOPIC,
            editor_id: "MS01EltrysTopic".to_string(),
            full_name: "Eltrys".to_string(),
            quest_refs: vec![QUEST],
            infos: vec![
                // Speaker-locked first branch, then the shared branch.
                info(INFO_ELTRYS, SPEAKER_BASE, "You took the note, then."),
                info(INFO_GENERIC, 0, "Watch yourself in this city."),
            ],
            ..Default::default()
        }
    }

    fn fixture_second_topic() -> DialRecord {
        DialRecord {
            form_id: TOPIC_RUMORS,
            editor_id: "MS01Rumors".to_string(),
            full_name: "Rumors".to_string(),
            quest_refs: vec![QUEST],
            infos: vec![info(INFO_RUMORS, 0, "Keep your head down.")],
            ..Default::default()
        }
    }

    fn install_index(world: &mut World) {
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.dialogues.insert(TOPIC, fixture_topic());
        index
            .dialogues
            .insert(TOPIC_RUMORS, fixture_second_topic());
        world.insert_resource(crate::cell_loader::LoadedCellIndex(Arc::new(index)));
    }

    /// Install the quest's alias 1 (forced to `reference`) and start the
    /// quest running; `refresh` binds the alias live.
    fn install_quest(world: &mut World, reference: u32, alias_id: i32) {
        install_scene_quest_aliases(
            world,
            [QustRecord {
                form_id: QUEST,
                aliases: vec![QuestAlias {
                    alias_id,
                    fill_type: Some(AliasFillType::ForcedReference(reference)),
                    ..Default::default()
                }],
                ..Default::default()
            }],
        );
    }

    fn start_quest(world: &mut World) {
        let mut stages = QuestStageState::default();
        stages.start_quest(QuestFormId(QUEST), None);
        world.insert_resource(stages);
    }

    fn spawn_actor(world: &mut World, reference: u32, base: u32) -> EntityId {
        let npc = world.spawn();
        world.insert(
            npc,
            SceneAliasCandidate {
                reference_form_id: reference,
                base_form_id: base,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        npc
    }

    fn spawn_player(world: &mut World) -> EntityId {
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        player
    }

    fn selected(world: &World, npc: EntityId) -> Option<NpcDialogueTopic> {
        world.get::<NpcDialogueTopic>(npc).map(|topic| topic.clone())
    }

    #[test]
    fn activation_selects_the_speaker_locked_branch() {
        let mut world = World::new();
        world.register::<ActivateEvent>();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        world.insert_resource(DialogueRegistry::default());
        world.insert_resource(DialogueSurfaceState::default());
        let player = spawn_player(&mut world);
        let eltrys = spawn_actor(&mut world, SPEAKER_REF, SPEAKER_BASE);
        install_index(&mut world);
        install_quest(&mut world, SPEAKER_REF, 1);
        start_quest(&mut world);
        refresh_scene_actor_bindings(&world);
        world.insert(eltrys, ActivateEvent { activator: player });

        npc_dialogue_selection_system(&world);

        let topic = selected(&world, eltrys).expect("the bound speaker must select");
        assert_eq!(topic.topic_form_id, TOPIC);
        assert_eq!(topic.info_form_id, INFO_ELTRYS);
        assert_eq!(topic.owning_quest, Some(QUEST));
        assert_eq!(topic.speaker_text, "You took the note, then.");
        assert_eq!(topic.response_number, 0);
        // Blocker 2: the response surface's list column rides the selection —
        // every owned DIAL, ascending, the selected one included.
        assert_eq!(
            topic
                .topics
                .iter()
                .map(|entry| entry.topic_form_id)
                .collect::<Vec<_>>(),
            vec![TOPIC, TOPIC_RUMORS]
        );
        assert_eq!(topic.topics[0].name, "Eltrys");
        assert_eq!(topic.topics[1].name, "Rumors");
        {
            let surface = world.resource::<DialogueSurfaceState>();
            assert_eq!(surface.serial, 1, "one applied selection bumps the serial once");
            assert_eq!(surface.npc, Some(eltrys));
        }
        // The registry holds the selected record for the presentation side.
        {
            let registry = world.resource::<DialogueRegistry>();
            assert_eq!(registry.topic(TOPIC).expect("installed").form_id, TOPIC);
        }

        // Blocker 2 — the surface's topic click lowers through the same
        // selection: another owned topic re-selects and bumps the serial.
        let message = select_topic_by_form_id(&mut world, eltrys, TOPIC_RUMORS)
            .expect("the second owned topic re-selects");
        assert!(message.contains(&format!("topic {TOPIC_RUMORS:#08X}")), "{message}");
        let topic = selected(&world, eltrys).expect("selection refreshed");
        assert_eq!(topic.topic_form_id, TOPIC_RUMORS);
        assert_eq!(topic.info_form_id, INFO_RUMORS);
        assert_eq!(topic.speaker_text, "Keep your head down.");
        {
            let surface = world.resource::<DialogueSurfaceState>();
            assert_eq!(surface.serial, 2, "the UI-driven selection bumps too");
        }

        // A topic the NPC does not own is refused at the ownership gate.
        assert!(
            select_topic_by_form_id(&mut world, eltrys, 0xDEAD_BEEF).is_err(),
            "unowned topic must not re-select"
        );
    }

    #[test]
    fn a_differently_bound_npc_takes_the_shared_branch() {
        let mut world = World::new();
        world.register::<ActivateEvent>();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        let player = spawn_player(&mut world);
        let patron = spawn_actor(&mut world, OTHER_REF, 0x20_0002);
        install_index(&mut world);
        install_quest(&mut world, OTHER_REF, 1);
        start_quest(&mut world);
        refresh_scene_actor_bindings(&world);
        world.insert(patron, ActivateEvent { activator: player });

        npc_dialogue_selection_system(&world);

        let topic = selected(&world, patron).expect("the same quest's other NPC still talks");
        assert_eq!(topic.info_form_id, INFO_GENERIC);
        assert_eq!(topic.speaker_text, "Watch yourself in this city.");
    }

    #[test]
    fn an_unbound_npc_or_stopped_quest_selects_nothing() {
        let mut world = World::new();
        world.register::<ActivateEvent>();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        let player = spawn_player(&mut world);
        let eltrys = spawn_actor(&mut world, SPEAKER_REF, SPEAKER_BASE);
        install_index(&mut world);
        // Aliases installed and quest running, but THIS actor's reference is
        // not any alias's fill — no ownership, no dialogue.
        install_quest(&mut world, OTHER_REF, 1);
        start_quest(&mut world);
        refresh_scene_actor_bindings(&world);
        world.insert(eltrys, ActivateEvent { activator: player });

        npc_dialogue_selection_system(&world);
        assert!(selected(&world, eltrys).is_none(), "unbound actor");

        // Now bind him, but stop the quest: ownership rides a *running*
        // quest, and a stopped one owns no topics.
        install_quest(&mut world, SPEAKER_REF, 2);
        refresh_scene_actor_bindings(&world);
        world.insert_resource(QuestStageState::default());

        world.insert(eltrys, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        assert!(selected(&world, eltrys).is_none(), "stopped quest");
    }

    #[test]
    fn dead_targets_and_nonplayer_activators_select_nothing() {
        let mut world = World::new();
        world.register::<ActivateEvent>();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        let player = spawn_player(&mut world);
        let bystander = spawn_actor(&mut world, OTHER_REF, 0x20_0002);
        let eltrys = spawn_actor(&mut world, SPEAKER_REF, SPEAKER_BASE);
        install_index(&mut world);
        install_quest(&mut world, SPEAKER_REF, 1);
        start_quest(&mut world);
        refresh_scene_actor_bindings(&world);

        // A corpse: death reconciliation owns it, not the dialogue loop.
        world.insert(eltrys, Dead);
        world.insert(eltrys, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        assert!(selected(&world, eltrys).is_none(), "dead actor");

        // Another NPC activating the target is not a dialogue request.
        world.remove::<Dead>(eltrys);
        world.insert(eltrys, ActivateEvent { activator: bystander });
        npc_dialogue_selection_system(&world);
        assert!(selected(&world, eltrys).is_none(), "non-player activator");
    }

    /// Registers everything the selection reads, installs the index, and
    /// binds `refs` to aliases 1..=n of the running fixture quest.
    fn bound_world(refs: &[(u32, u32)]) -> (World, EntityId, Vec<EntityId>) {
        let mut world = World::new();
        world.register::<ActivateEvent>();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        world.register::<AiCombatState>();
        world.insert_resource(DialogueRegistry::default());
        world.insert_resource(DialogueSurfaceState::default());
        let player = spawn_player(&mut world);
        let npcs = refs
            .iter()
            .map(|&(reference, base)| spawn_actor(&mut world, reference, base))
            .collect();
        install_index(&mut world);
        install_scene_quest_aliases(
            &mut world,
            [QustRecord {
                form_id: QUEST,
                aliases: refs
                    .iter()
                    .enumerate()
                    .map(|(i, &(reference, _))| QuestAlias {
                        alias_id: i as i32 + 1,
                        fill_type: Some(AliasFillType::ForcedReference(reference)),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
        );
        start_quest(&mut world);
        refresh_scene_actor_bindings(&world);
        (world, player, npcs)
    }

    /// #5038 — talk to A, then B: the surface presents B, and A no longer
    /// carries a selection. Sparse-set order used to hand the snapshot A's
    /// topic and route the next click to A.
    #[test]
    fn a_second_conversation_presents_the_second_npc() {
        let (mut world, player, npcs) =
            bound_world(&[(SPEAKER_REF, SPEAKER_BASE), (OTHER_REF, 0x20_0002)]);
        let (eltrys, patron) = (npcs[0], npcs[1]);

        world.insert(eltrys, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        world.remove::<ActivateEvent>(eltrys);
        assert_eq!(dialogue_snapshot(&world).expect("A selected").npc, eltrys);

        world.insert(patron, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);

        let snapshot = dialogue_snapshot(&world).expect("B selected");
        assert_eq!(snapshot.npc, patron, "the surface must present B");
        assert_eq!(snapshot.info_form_id, INFO_GENERIC);
        assert_eq!(snapshot.response_text, "Watch yourself in this city.");
        assert!(
            selected(&world, eltrys).is_none(),
            "A's selection must not outlive the conversation"
        );
        assert_eq!(world.resource::<DialogueSurfaceState>().serial, 2);
    }

    /// #5043 / #4701 — a dead player drives neither entry point.
    #[test]
    fn a_dead_player_drives_no_dialogue() {
        let (mut world, player, npcs) = bound_world(&[(SPEAKER_REF, SPEAKER_BASE)]);
        let eltrys = npcs[0];
        world.insert(player, Dead);

        world.insert(eltrys, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        assert!(selected(&world, eltrys).is_none(), "scripted activation");
        assert!(
            select_topic_by_form_id(&mut world, eltrys, TOPIC).is_err(),
            "topic click on an open surface"
        );
        assert_eq!(world.resource::<DialogueSurfaceState>().serial, 0);
    }

    /// #5043 — an NPC in combat refuses both entry points.
    #[test]
    fn a_combatant_npc_refuses_dialogue() {
        let (mut world, player, npcs) = bound_world(&[(SPEAKER_REF, SPEAKER_BASE)]);
        let eltrys = npcs[0];
        world.insert(
            eltrys,
            AiCombatState {
                target: player,
                attack_cooldown_remaining: 0.0,
            },
        );

        world.insert(eltrys, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        assert!(selected(&world, eltrys).is_none(), "activation mid-fight");
        let err = select_topic_by_form_id(&mut world, eltrys, TOPIC).unwrap_err();
        assert!(err.contains("combat"), "{err}");

        // Out of combat, the same NPC talks again.
        world.remove::<AiCombatState>(eltrys);
        npc_dialogue_selection_system(&world);
        assert_eq!(
            selected(&world, eltrys)
                .expect("talks after combat")
                .info_form_id,
            INFO_ELTRYS
        );
    }

    /// #5152 — the spoken line's INFO fragment dispatches at selection:
    /// the OnBegin binding runs when the line is selected, and the OnEnd
    /// binding runs when the conversation closes. The stage advance rides
    /// the canonical journal/sink the quest-fragment dispatcher owns.
    #[test]
    fn the_spoken_lines_fragments_advance_the_stage() {
        use byroredux_scripting::quest_stages::{QuestObjectiveState, QuestStageState};
        use byroredux_scripting::translate::compose::QuestRef;
        use byroredux_scripting::{DialogueInfoFragments, Effect};

        const HELPER_QUEST: u32 = 0x30_0001;
        let (mut world, player, npcs) = bound_world(&[(SPEAKER_REF, SPEAKER_BASE)]);
        let eltrys = npcs[0];
        world.register::<byroredux_scripting::quest_stages::QuestStageAdvancedBatch>();
        // bound_world already installed the running fixture quest's state —
        // start the helper quest on THAT resource (a fresh default would
        // drop the MS01 running state and the alias ownership with it).
        world.insert_resource(QuestObjectiveState::default());
        world
            .resource_mut::<QuestStageState>()
            .start_quest(QuestFormId(HELPER_QUEST), None);
        world.insert_resource(
            byroredux_scripting::papyrus_demo::PapyrusPlayerEntity(player),
        );
        // The INFO the fixture topic's speaker-locked branch selects
        // (INFO_ELTRYS) carries an OnBegin `SetStage 13` on the helper
        // quest; the second topic's INFO carries an OnEnd `SetStage 82`.
        let mut fragments = DialogueInfoFragments::default();
        fragments.insert(
            INFO_ELTRYS,
            QuestFormId(HELPER_QUEST),
            None,
            Some(vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 13,
            }]),
            None,
        );
        fragments.insert(
            INFO_RUMORS,
            QuestFormId(HELPER_QUEST),
            None,
            None,
            Some(vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 82,
            }]),
        );
        world.insert_resource(fragments);

        // Activation speaks the line: its OnBegin fragment sets stage 13,
        // journaled onto the shared player sink for the dispatcher.
        world.insert(eltrys, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        let advances = world
            .query::<byroredux_scripting::quest_stages::QuestStageAdvancedBatch>()
            .map(|query| {
                query
                    .iter()
                    .flat_map(|(_, batch)| batch.0.to_vec())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // The retained journal also carries the fixture/helper quest-start
        // events the bound_world setup produced; the fragment's own advance
        // is the (helper, 13) transition.
        let begin_advance = advances
            .iter()
            .find(|advance| advance.quest.0 == HELPER_QUEST && advance.new_stage == 13)
            .expect("the OnBegin SetStage advanced the helper quest to 13");
        assert_eq!(begin_advance.previous_stage, 0);

        // Closing the conversation runs the open line's... nothing (no
        // end binding on INFO_ELTRYS), but selects the rumors topic whose
        // END binding carries stage 82 — the close path fires it.
        select_topic_by_form_id(&mut world, eltrys, TOPIC_RUMORS)
            .expect("the second owned topic re-selects");
        end_open_conversation(&world);
        let advances = world
            .query::<byroredux_scripting::quest_stages::QuestStageAdvancedBatch>()
            .map(|query| {
                query
                    .iter()
                    .flat_map(|(_, batch)| batch.0.to_vec())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(
            advances
                .iter()
                .any(|advance| advance.quest.0 == HELPER_QUEST && advance.new_stage == 82),
            "the OnEnd SetStage advanced the helper quest to 82 on close"
        );
        assert!(
            world.query::<NpcDialogueTopic>().unwrap().iter().next().is_none(),
            "the close clears the selection stamp"
        );
    }

    /// #5017 — an unconscious NPC "cannot be talked to" (CK): both entry
    /// points refuse, and waking restores the conversation.
    #[test]
    fn an_unconscious_npc_refuses_dialogue() {
        let (mut world, player, npcs) = bound_world(&[(SPEAKER_REF, SPEAKER_BASE)]);
        world.register::<byroredux_scripting::ActorControlState>();
        let eltrys = npcs[0];
        byroredux_scripting::update_actor_control(&world, eltrys, |state| {
            state.set_unconscious(true)
        });
        world.insert(eltrys, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        assert!(selected(&world, eltrys).is_none());
        let err = select_topic_by_form_id(&mut world, eltrys, TOPIC).unwrap_err();
        assert!(err.contains("unconscious"), "{err}");

        byroredux_scripting::update_actor_control(&world, eltrys, |state| {
            state.set_unconscious(false)
        });
        npc_dialogue_selection_system(&world);
        assert!(selected(&world, eltrys).is_some(), "awake: talks");
    }

    /// #5037 fixture: MS01's real shape in miniature. A blocking branch
    /// (`0x18A96`-like) whose entry topic has a HIGHER form id than its own
    /// mid-branch child, a top-level branch, another speaker's top-level
    /// topic, and a Scene topic with the lowest form id of all.
    mod branches {
        use super::*;
        use byroredux_plugin::esm::records::DlbrRecord;

        const CHILD: u32 = 0x20_0100; // mid-branch child, lowest topic id
        const SCENE: u32 = 0x20_00F0; // Scene category, lowest id overall
        const BLOCKING_ENTRY: u32 = 0x20_0200;
        const TOP_ENTRY: u32 = 0x20_0300;
        const OTHER_SPEAKER: u32 = 0x20_0400;
        const BLOCKING_BRANCH: u32 = 0x20_1000;
        const TOP_BRANCH: u32 = 0x20_2000;
        const OTHER_BRANCH: u32 = 0x20_3000;
        const HOGNI_BASE: u32 = 0x20_0009;

        fn topic(form_id: u32, branch: Option<u32>, infos: Vec<InfoRecord>) -> DialRecord {
            DialRecord {
                form_id,
                editor_id: format!("Topic{form_id:X}"),
                quest_refs: vec![QUEST],
                branch,
                infos,
                ..Default::default()
            }
        }

        fn dlbr(form_id: u32, flags: u32, starting_topic: u32) -> DlbrRecord {
            DlbrRecord {
                form_id,
                quest: QUEST,
                flags,
                starting_topic,
                ..Default::default()
            }
        }

        /// `blocking_speaker` is whom the blocking entry's INFO names.
        fn install(world: &mut World, blocking_speaker: u32) {
            let mut index = byroredux_plugin::esm::records::EsmIndex::default();
            let mut blocking_info = info(0x20_0201, blocking_speaker, "You there. The shrine.");
            blocking_info.topic_links = vec![CHILD];
            let topics = [
                topic(
                    CHILD,
                    Some(BLOCKING_BRANCH),
                    vec![info(0x20_0101, 0, "Mid-branch answer.")],
                ),
                DialRecord {
                    category: DialogueCategory::Scene,
                    ..topic(SCENE, None, vec![info(0x20_00F1, 0, "A scene line.")])
                },
                topic(BLOCKING_ENTRY, Some(BLOCKING_BRANCH), vec![blocking_info]),
                topic(
                    TOP_ENTRY,
                    Some(TOP_BRANCH),
                    vec![info(0x20_0301, 0, "Ask away.")],
                ),
                topic(
                    OTHER_SPEAKER,
                    Some(OTHER_BRANCH),
                    vec![info(0x20_0401, HOGNI_BASE, "Hogni's line.")],
                ),
            ];
            for record in topics {
                index.dialogues.insert(record.form_id, record);
            }
            for branch in [
                dlbr(BLOCKING_BRANCH, DlbrRecord::FLAG_BLOCKING, BLOCKING_ENTRY),
                dlbr(TOP_BRANCH, DlbrRecord::FLAG_TOP_LEVEL, TOP_ENTRY),
                dlbr(OTHER_BRANCH, DlbrRecord::FLAG_TOP_LEVEL, OTHER_SPEAKER),
            ] {
                index.dialogue_branches.insert(branch.form_id, branch);
            }
            world.insert_resource(crate::cell_loader::LoadedCellIndex(Arc::new(index)));
        }

        fn activate(blocking_speaker: u32) -> (World, EntityId) {
            let (mut world, player, npcs) = bound_world(&[(SPEAKER_REF, SPEAKER_BASE)]);
            install(&mut world, blocking_speaker);
            world.insert(npcs[0], ActivateEvent { activator: player });
            npc_dialogue_selection_system(&world);
            (world, npcs[0])
        }

        fn listed(topic: &NpcDialogueTopic) -> Vec<u32> {
            topic
                .topics
                .iter()
                .map(|entry| entry.topic_form_id)
                .collect()
        }

        /// A qualifying blocking entry opens the conversation even though its
        /// own mid-branch child has a lower form id, and its link replaces the
        /// list.
        #[test]
        fn a_qualifying_blocking_entry_opens_and_its_links_replace_the_list() {
            let (world, eltrys) = activate(SPEAKER_BASE);
            let topic = selected(&world, eltrys).expect("blocking entry selects");
            assert_eq!(topic.topic_form_id, BLOCKING_ENTRY);
            assert_eq!(topic.speaker_text, "You there. The shrine.");
            assert_eq!(
                listed(&topic),
                vec![CHILD],
                "the linked child, nothing else"
            );
        }

        /// Without a qualifying blocking entry, the Top-Level list opens. It
        /// excludes the mid-branch child, the Scene topic and another speaker's
        /// topic, although each has a lower form id than an entry or a passing
        /// INFO of its own.
        #[test]
        fn otherwise_the_top_level_list_opens_without_children_scenes_or_other_speakers() {
            let (world, eltrys) = activate(HOGNI_BASE);
            let topic = selected(&world, eltrys).expect("a top-level entry selects");
            assert_eq!(topic.topic_form_id, TOP_ENTRY);
            assert_eq!(listed(&topic), vec![TOP_ENTRY]);
        }

        /// Clicking the linked child answers it, and a line that links
        /// nowhere returns to the Top-Level list. The Scene topic is refused
        /// outright.
        #[test]
        fn clicking_a_link_answers_it_then_returns_to_the_top_level_list() {
            let (mut world, eltrys) = activate(SPEAKER_BASE);
            select_topic_by_form_id(&mut world, eltrys, CHILD).expect("the link answers");
            let topic = selected(&world, eltrys).unwrap();
            assert_eq!(topic.speaker_text, "Mid-branch answer.");
            assert_eq!(listed(&topic), vec![TOP_ENTRY]);
            assert!(select_topic_by_form_id(&mut world, eltrys, SCENE).is_err());
        }
    }
}
