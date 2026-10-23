//! P4 blocker 1 — NPC activation → topic selection (MS01 fixture).
//!
//! The authored NPC→topic edge is `DialRecord::quest_refs` (QSTI on
//! Oblivion/FO3/FNV, QNAM on Skyrim/FO4/FO76/Starfield): an NPC
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
//! - Oblivion authors no branches and no DIAL flags byte, and its menu is
//!   the player's *known-topics* list (#5350): topics enter it through an
//!   INFO's `NAME` "Add Topics" or a script `AddTopic`, while a topic named
//!   only by an INFO's `TCLT` "Choices" is link-only. The classifier below
//!   cannot see that set yet, so today every branch-less Oblivion topic
//!   classifies as a list entry — latent, because Oblivion has no quest
//!   aliases and so no NPC owns a topic. Oblivion's census: 1,761 of
//!   Oblivion.esm's 3,184 Topic DIALs are `TCLT` targets and never `NAME`
//!   targets. When Oblivion ownership is wired, `topic_entry` needs an
//!   Oblivion arm returning `LinkOnly` for those.
//!   FO3/FNV author no branches either, but their DIAL `DATA` carries a
//!   flags byte (#5224): only the `Top-level` (0x02) topics are list
//!   entries — 844 of Fallout3.esm's 5,130 Topic DIALs — and the rest are
//!   reached through a link.
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
    /// The INFO the menu pass chose for this topic (#6007). A click speaks
    /// this line rather than evaluating the topic's INFOs a second time: the
    /// menu only offered the entry because that INFO passed, and a fresh
    /// evaluation of a stochastic gate (`GetRandomPercent`) can fail the entry
    /// the player just clicked.
    pub(crate) info_form_id: u32,
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
    /// #5367 Phase L — the chosen line carries the INFO `Goodbye` flag:
    /// the conversation ends when its presentation finishes (the
    /// estimated-duration close), not the page's Close button.
    pub(crate) goodbye: bool,
}

impl Component for NpcDialogueTopic {
    type Storage = SparseSetStorage<Self>;
}

pub(crate) use byroredux_scripting::DialogueSurfaceState;

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
    /// branch (Oblivion, or a branch the loaded plugins do not define). On
    /// FO3/FNV a branch-less topic is only a list entry when its DIAL
    /// `DATA` flags carry `Top-level` (#5224). On Oblivion this is a
    /// placeholder, not the rule: the real opening list is the known-topic
    /// set (`NAME` / `AddTopic`), which this classifier does not model
    /// yet (#5350).
    TopLevel,
    /// Reached only through another INFO's `TCLT` link.
    LinkOnly,
}

fn topic_entry(index: &EsmIndex, record: &DialRecord) -> TopicEntry {
    let Some(branch) = record
        .branch
        .and_then(|branch| index.dialogue_branches.get(&branch))
    else {
        // #5224 — a Fallout-era topic whose `DATA` flags lack `Top-level`
        // is choice-only: never an opening entry. `None` (any other game,
        // or no `DATA` authored) keeps the pre-#5224 shape.
        if record.top_level() == Some(false) {
            return TopicEntry::LinkOnly;
        }
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
    selected: Option<(&'a DialRecord, &'a InfoRecord)>,
) -> Vec<(&'a DialRecord, &'a InfoRecord)> {
    owned
        .iter()
        .copied()
        .filter(|record| topic_entry(index, record) == TopicEntry::TopLevel)
        .filter_map(|record| {
            let info = selected
                .filter(|(selected_record, _)| selected_record.form_id == record.form_id)
                .map(|(_, info)| info)
                .or_else(|| select_first_info(record, world, Some(npc), Some(player)))?;
            Some((record, info))
        })
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
) -> Vec<(&'a DialRecord, &'a InfoRecord)> {
    info.topic_links
        .iter()
        .filter_map(|topic| index.dialogues.get(topic))
        .filter_map(|record| {
            select_first_info(record, world, Some(npc), Some(player)).map(|info| (record, info))
        })
        .collect()
}

fn topic_entries(records: &[(&DialRecord, &InfoRecord)]) -> Vec<DialogueTopicEntry> {
    records
        .iter()
        .map(|(record, info)| DialogueTopicEntry {
            topic_form_id: record.form_id,
            name: if record.full_name.is_empty() {
                record.editor_id.clone()
            } else {
                record.full_name.clone()
            },
            info_form_id: info.form_id,
        })
        .collect()
}

/// Speak the already-selected INFO and package it with the list that follows
/// it: the INFO's passing links, or `fallback_menu` when it links nowhere.
/// Carrying the INFO prevents a second evaluation of stateful conditions.
fn select_on_topic<'a>(
    index: &'a EsmIndex,
    record: &'a DialRecord,
    info: &'a InfoRecord,
    world: &World,
    npc: EntityId,
    player: EntityId,
    fallback_menu: &[(&'a DialRecord, &'a InfoRecord)],
) -> Option<(NpcDialogueTopic, &'a DialRecord)> {
    let linked = linked_menu(index, info, world, npc, player);
    let single = [(record, info)];
    let menu: &[(&DialRecord, &InfoRecord)] = if !linked.is_empty() {
        &linked
    } else if !fallback_menu.is_empty() {
        fallback_menu
    } else {
        &single
    };
    Some((
        NpcDialogueTopic {
            topic_form_id: record.form_id,
            topic_editor_id: record.editor_id.clone(),
            info_form_id: info.form_id,
            // #5271 — name the quest that actually owns the SPOKEN line
            // (its QSTI), not the topic's first listed owner: on a
            // multi-quest topic those differ, and the log previously
            // attributed the line to a quest that may not even run.
            owning_quest: if info.quest != 0 {
                Some(info.quest)
            } else {
                record.quest_refs.first().copied()
            },
            speaker_text: info.response_text.clone(),
            response_number: info.response_number,
            emotion_type: info.emotion_type,
            topics: topic_entries(menu),
            goodbye: info.goodbye(),
        },
        // #5426 — the record goes back as a borrow; the caller clones
        // exactly once where ownership is genuinely needed (the registry
        // copy), instead of every selection paying a whole-DialRecord
        // clone up front (the FNV GREETING record owns 5,300 INFOs).
        record,
    ))
}

/// The master's generic activation-greeting topic (#5367 Phase G).
/// The corpus convention (2026-10-07 census, `dialogue-trees.md` §2):
/// FO3/FNV author exactly one DIAL with EDID `GREETING` (its `HELLO`
/// sibling is the ambient passing bark, not the activation path);
/// Skyrim authors `DialogueGenericHello`. The 300-400 other
/// greeting-named DIALs per master are quest-specific and already ride
/// quest ownership like any owned topic.
fn generic_greeting_record(index: &EsmIndex) -> Option<&DialRecord> {
    // #5426 — production reads the FormID the load order resolved once
    // at merge end (`EsmIndex::generic_greeting_form`); the scan stays
    // as the fallback for hand-built test indexes that insert into
    // `dialogues` without a merge.
    index
        .generic_greeting_form
        .and_then(|form_id| index.dialogues.get(&form_id))
        .or_else(|| {
            index
                .dialogues
                .values()
                .find(|record| matches!(record.editor_id.as_str(), "GREETING" | "DialogueGenericHello"))
        })
}

/// The activation's opening selection (#5037): a qualifying Blocking entry
/// pre-empts everything and its links become the list; otherwise the first
/// Top-Level entry opens the Top-Level list. #5367 Phase G: when the NPC
/// owns no qualifying quest topic at all, the master's generic greeting
/// opens instead — the quest-less-patron hole ("selects nothing") — with
/// the greeting topic itself as the list (its INFOs typically link
/// nowhere; the conversation closes from the page or a Goodbye line).
fn open_conversation<'a>(
    index: &'a EsmIndex,
    owned: &[&'a DialRecord],
    world: &World,
    npc: EntityId,
    player: EntityId,
) -> Option<(NpcDialogueTopic, &'a DialRecord)> {
    let blocking = owned.iter().copied().find_map(|record| {
        (topic_entry(index, record) == TopicEntry::Blocking)
            .then(|| select_first_info(record, world, Some(npc), Some(player)))
            .flatten()
            .map(|info| (record, info))
    });
    if let Some((record, info)) = blocking {
        // Blocking: the list is whatever links from this line, never the
        // normal topics.
        return select_on_topic(index, record, info, world, npc, player, &[]);
    }
    let menu = top_level_menu(index, owned, world, npc, player, None);
    if let Some((first, info)) = menu.first().copied() {
        return select_on_topic(index, first, info, world, npc, player, &menu);
    }
    let greeting = generic_greeting_record(index)?;
    let info = select_first_info(greeting, world, Some(npc), Some(player))?;
    select_on_topic(index, greeting, info, world, npc, player, &[])
}

/// Apply one selection: registry install (the presentation side's record
/// source), the component stamp, the surface-serial bump, and — #5152 — the
/// spoken line's fragments: the outgoing line's OnEnd (the line stops being
/// spoken when another replaces it), then the selected line's OnBegin.
/// Shared by the activation path and the UI's re-selection door.
fn apply_selection(world: &World, npc: EntityId, topic: NpcDialogueTopic, record: &DialRecord) {
    // #6190 — the NPC has now spoken to the player: `GetTalkedToPC` reads this.
    if let Some(form) = byroredux_scripting::condition::entity_global_form_id(world, npc) {
        if let Some(mut ledger) = world.try_resource_mut::<byroredux_scripting::obscript_ledger::ObScriptLedger>() {
            ledger.note_talked_to_player(form);
        }
    }
    // The outgoing line ends before the new one begins (vanilla's OnEnd
    // ordering). One live selection (#5038) means every existing stamp IS
    // the outgoing line.
    let (outgoing, speakers): (Vec<NpcDialogueTopic>, Vec<EntityId>) = world
        .query::<NpcDialogueTopic>()
        .map(|topics| {
            // #5300 — every existing stamp ends, INCLUDING one naming the
            // same INFO: re-selecting a line re-runs its OnBegin, so the
            // matching OnEnd must fire first (the old `!= info_form_id`
            // filter ran OnBegin twice with no OnEnd between).
            let outgoing = topics.iter().map(|(_, topic)| topic.clone()).collect();
            let speakers = topics.iter().map(|(entity, _)| entity).collect();
            (outgoing, speakers)
        })
        .unwrap_or_default();
    for (existing, speaker) in outgoing.iter().zip(&speakers) {
        speak_info_end_fragment(world, existing, *speaker);
    }
    // #5432 — the outgoing line stops being spoken: cut every stamped
    // speaker's still-playing/queued segments (a stale partner's, and
    // this NPC's own previous line) BEFORE `play_line_voice` queues the
    // new line's, which `stop_sounds_for` would otherwise dequeue too.
    for speaker in speakers.iter().copied().chain(std::iter::once(npc)) {
        crate::systems::dialogue_voice::stop_line_voice(world, speaker);
    }
    // #5367 Phase V — resolve the voice while the record is still
    // owned (insert_topic takes it below). The file name is composed
    // from the selection's topic + owning-quest EDIDs.
    let voice_seconds = record
        .infos
        .iter()
        .find(|info| info.form_id == topic.info_form_id)
        .and_then(|info| {
            crate::systems::dialogue_voice::play_line_voice(
                world,
                npc,
                info,
                &topic.topic_editor_id,
                topic.owning_quest,
            )
        });
    if let Some(mut registry) = world.try_resource_mut::<DialogueRegistry>() {
        // #5426 — the one whole-record clone a conversation open pays:
        // the registry's own (Arc-shared) copy.
        registry.insert_topic(record.clone());
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
        // #5367 Phase L — a Goodbye line closes the conversation when its
        // presentation finishes (voice duration when Phase V resolved
        // it, the subtitle estimate otherwise).
        surface.close_after = topic.goodbye.then(|| {
            let now = world
                .try_resource::<byroredux_core::ecs::resources::TotalTime>()
                .map(|time| time.0)
                .unwrap_or_default();
            let estimate =
                byroredux_scripting::estimate_dialogue_duration(&topic.speaker_text);
            now + voice_seconds.map(|s| s as f32).unwrap_or(estimate).max(0.1)
        });
    }
    // #5367 Phase L — the spoken set feeds Say-Once disqualification.
    byroredux_scripting::note_info_spoken(world, npc, topic.info_form_id);
    log::info!(
        "npc dialogue: selected topic {:#08X} ('{}') info {:#08X} for NPC {npc} (quest {:?})",
        topic.topic_form_id,
        topic.topic_editor_id,
        topic.info_form_id,
        topic.owning_quest,
    );
    speak_info_begin_fragment(world, &topic, npc);
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
/// and the journal stay interleaved with every other producer. #5297 — the
/// journal is *not* polled here (see `apply_spoken_info_fragment`): this
/// runs in `Stage::Late`, and a poll would claim the cursor ahead of the
/// next frame's Update dispatcher while the re-emitted batch is drained
/// unread by `event_cleanup_system` at the end of this same stage. A no-op
/// when the populate walk never filled the table (no `--scripts-bsa`,
/// pre-Papyrus game) or the line has no OnBegin binding — most INFO
/// dialogue is inert flavor, and 3 773 of vanilla's 5 257 bound INFOs are
/// OnEnd-only.
fn speak_info_begin_fragment(world: &World, topic: &NpcDialogueTopic, speaker: EntityId) {
    run_spoken_result_script(world, topic, speaker, true);
    let Some((effects, context, vmad)) = spoken_fragment_effects(world, topic.info_form_id, true)
    else {
        return;
    };
    dispatch_spoken_fragment(world, &effects, context, vmad.as_ref(), speaker);
}

/// The outgoing line's OnEnd fragment — "the line is done". Fires when
/// another selection replaces it ([`apply_selection`]) and when the
/// conversation surface closes ([`end_open_conversation`]).
fn speak_info_end_fragment(world: &World, topic: &NpcDialogueTopic, speaker: EntityId) {
    run_spoken_result_script(world, topic, speaker, false);
    let Some((effects, context, vmad)) = spoken_fragment_effects(world, topic.info_form_id, false)
    else {
        return;
    };
    dispatch_spoken_fragment(world, &effects, context, vmad.as_ref(), speaker);
}

/// M47.3 — the Oblivion / FO3 / FNV form of a spoken line's script: the
/// INFO's compiled Begin (`begin`) or End result script, run with the speaker
/// as its implicit reference. Skyrim+ INFOs carry none (their lines run the
/// `TIF_` fragments above), so this is a lookup and a return there.
fn run_spoken_result_script(world: &World, topic: &NpcDialogueTopic, speaker: EntityId, begin: bool) {
    // Scope the guard to the clone, as every `LoadedCellIndex` reader here
    // does (#5066): the script's commands take world resources.
    let index = {
        let Some(index) = world.try_resource::<LoadedCellIndex>() else {
            return;
        };
        index.0.clone()
    };
    let Some(info) = index
        .dialogues
        .get(&topic.topic_form_id)
        .and_then(|record| record.infos.iter().find(|info| info.form_id == topic.info_form_id))
    else {
        return;
    };
    let script = if begin { &info.result_script_begin } else { &info.result_script_end };
    if script.is_none() {
        return;
    }
    let Some(speaker_form) = byroredux_scripting::condition::entity_global_form_id(world, speaker) else {
        log::warn!(
            "npc dialogue: INFO {:#08X}'s result script has no speaker reference to run on",
            info.form_id
        );
        return;
    };
    byroredux_scripting::obscript_results::run_info_result_script(world, info, begin, speaker, speaker_form);
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

/// `speaker` is the NPC saying the line — the fragment's `akSpeaker` (M47.2).
fn dispatch_spoken_fragment(
    world: &World,
    effects: &[Effect],
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    speaker: EntityId,
) {
    let advances = byroredux_scripting::apply_spoken_info_fragment(
        world,
        effects,
        context,
        vmad,
        Some(speaker),
    );
    if advances.is_empty() {
        return;
    }
    // #3580 — copy the entity out and drop the guard before the batch
    // storage acquisition (the scene dispatcher's lock-order note).
    let Some(player) = byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world)
    else {
        return;
    };
    byroredux_scripting::quest_stages::push_quest_stage_advances(world, player, advances);
}

/// #5367 Phase F — a force-greet package opened the conversation without
/// activation: the *package* authorizes the topic (ownership via running
/// quests is bypassed — that is the point of the procedure), the INFO
/// gate is still the evaluator, and the generic greeting stands in when
/// the package authored no topic (`PKDD` zero). Same two-pass
/// gather-then-apply discipline as the activation path. Returns whether
/// a line was applied.
pub(crate) fn forcegreet_open(world: &World, npc: EntityId, topic: Option<u32>) -> bool {
    // #5066 / #5372 — scope the guard to the clone. The shadowed
    // read used to live to the end of the function, and any nested
    // `LoadedCellIndex` reader under it (raise_hello_story_event,
    // play_line_voice) closed a lock-order cycle.
    let index = {
        let Some(index) = world.try_resource::<LoadedCellIndex>() else {
            return false;
        };
        index.0.clone()
    };
    let Some(player) = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return false;
    };
    // #5392 — the gate every other dialogue-open path applies.
    if !crate::systems::player_can_act(world) || npc_refuses_dialogue(world, npc).is_some() {
        return false;
    }
    // #5426 — borrow the record end to end: the only whole-DialRecord
    // clone on this path is the registry copy inside apply_selection
    // (was two — the topic-less generic greeting cloned the 5,300-INFO
    // record once here and once in select_on_topic).
    let record = match topic {
        Some(form_id) => index.dialogues.get(&form_id),
        None => generic_greeting_record(&index),
    };
    let Some(record) = record else {
        return false;
    };
    let Some(info) = select_first_info(record, world, Some(npc), Some(player)) else {
        return false;
    };
    let Some((selected, record)) = select_on_topic(&index, record, info, world, npc, player, &[])
    else {
        return false;
    };
    apply_selection(world, npc, selected, record);
    // #5366 Phase 4 — the hello story event rides every opened
    // conversation (greeter → R1, player → R2).
    crate::systems::story_events::raise_hello_story_event(world, npc, player);
    true
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
        speak_info_end_fragment(world, &outgoing, npc);
    }
    // #5432 — closing the conversation cuts the line it was speaking.
    crate::systems::dialogue_voice::stop_line_voice(world, npc);
    if let Some(mut topics) = world.query_mut::<NpcDialogueTopic>() {
        topics.remove(npc);
    }
    if let Some(mut surface) = world.try_resource_mut::<DialogueSurfaceState>() {
        surface.npc = None;
        // #5367 Phase L — the close can come from the page (Close/Escape)
        // instead of the Goodbye timer; either way no close is pending.
        surface.close_after = None;
    }
}

/// #5367 Phase L — end a Goodbye conversation when its line's
/// presentation finishes: `end_open_conversation` (OnEnd fragments,
/// selection cleared) plus the app-side page-close cue. Called at the
/// head of the Late selection system; a no-op every frame no Goodbye
/// line is open.
fn npc_dialogue_goodbye_close_check(world: &World) {
    // #5867 — the conversation ends when its partner can no longer hold one:
    // dead, bleeding out / unconscious, or fighting. The page used to stay
    // open, listing a corpse's topics (every click refused) and answering
    // `IsInDialogueWithPlayer` for it, until the player closed it and the dead
    // speaker's OnEnd fragments could open a barter with the corpse.
    let partner_gone = world
        .try_resource::<DialogueSurfaceState>()
        .and_then(|surface| surface.npc)
        .is_some_and(|npc| npc_refuses_dialogue(world, npc).is_some());
    if partner_gone {
        end_open_conversation(world);
        if let Some(mut surface) = world.try_resource_mut::<DialogueSurfaceState>() {
            surface.close_requested = true;
        }
        return;
    }
    let due = world
        .try_resource::<DialogueSurfaceState>()
        .and_then(|surface| surface.close_after)
        .is_some_and(|close_after| {
            world
                .try_resource::<byroredux_core::ecs::resources::TotalTime>()
                .is_some_and(|time| time.0 >= close_after)
        });
    if !due {
        return;
    }
    end_open_conversation(world);
    if let Some(mut surface) = world.try_resource_mut::<DialogueSurfaceState>() {
        surface.close_requested = true;
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
    // #5367 Phase L — a spoken Goodbye line ends its conversation when
    // the presentation finishes (before any new selection this frame).
    // Same exclusive lane, same conversation lifecycle owner.
    npc_dialogue_goodbye_close_check(world);

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
    // #5066 / #5372 — scope the guard to the clone. The shadowed
    // read used to live to the end of the function, and any nested
    // `LoadedCellIndex` reader under it (raise_hello_story_event,
    // play_line_voice) closed a lock-order cycle.
    let index = {
        let Some(index) = world.try_resource::<LoadedCellIndex>() else {
            return;
        };
        index.0.clone()
    };
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
        if activator != player
            || npc == player
            || npc_refuses_dialogue(world, npc).is_some()
            // M47.2 — script-blocked activation opens no conversation.
            || byroredux_scripting::default_activation_blocked(world, npc)
        {
            continue;
        }
        // The actor identity the alias fill stamped — also what INFO speaker
        // matching reads. No candidate identity, no authored dialogue route.
        if world.get::<SceneAliasCandidate>(npc).is_none() {
            continue;
        }
        let owned_quests = running_quests_binding_entity(world, npc);
        // #5367 Phase G — an empty owned set no longer ends the walk: the
        // generic greeting still opens a conversation for a quest-less
        // patron. `open_conversation` decides between the two.
        let owned = owned_topic_records(&index, &owned_quests);
        let Some((topic, record)) = open_conversation(&index, &owned, world, npc, player) else {
            continue;
        };
        scratch.selections.push(TopicSelection {
            npc,
            topic,
            // #5426 — one clone into the cross-pass scratch (pass 2
            // applies after all readers drop); apply_selection borrows.
            record: record.clone(),
        });
    }
    if scratch.selections.is_empty() {
        return;
    }

    // ── Pass 2: apply. ──
    for selection in &scratch.selections {
        apply_selection(world, selection.npc, selection.topic.clone(), &selection.record);
        // #5366 Phase 4 — each opened activation conversation raises the
        // hello story event (greeter → R1, player → R2).
        crate::systems::story_events::raise_hello_story_event(world, selection.npc, player);
    }
}

/// The response surface's topic click, lowered through the same selection
/// the activation path uses — never a separate mutation path. The requested
/// topic must still be among the NPC's owned topics (authored ownership is
/// re-checked against the live bindings); the INFO gate is the evaluator.
/// Returns a user-facing message either way.
pub(crate) fn select_topic_by_form_id(
    world: &World,
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
    // #5066 / #5372 — scope the guard to the clone. The shadowed
    // read used to live to the end of the function, and any nested
    // `LoadedCellIndex` reader under it (raise_hello_story_event,
    // play_line_voice) closed a lock-order cycle.
    let index = {
        let Some(index) = world.try_resource::<LoadedCellIndex>() else {
            return Err("no loaded plugin index".to_string());
        };
        index.0.clone()
    };
    let Some(record) = index.dialogues.get(&topic_form_id) else {
        return Err(format!("no topic {topic_form_id:08X} in the loaded plugins"));
    };
    // #6007 — the line the open menu offered for this entry, if the NPC is
    // mid-conversation and the entry is in its list: the menu pass already
    // evaluated (and, for a stochastic gate, drew) that INFO's conditions. Only
    // a click on a topic the NPC's menu does not list falls back to a fresh
    // evaluation.
    let offered = world
        .get::<NpcDialogueTopic>(npc)
        .and_then(|current| {
            current
                .topics
                .iter()
                .find(|entry| entry.topic_form_id == topic_form_id)
                .map(|entry| entry.info_form_id)
        })
        .and_then(|info_form_id| record.infos.iter().find(|info| info.form_id == info_form_id));
    // A topic is selectable when a running quest the NPC is bound to owns it,
    // or when the open menu offers it: the menu is the spoken line's `TCLT`
    // links, and on Oblivion / FO3 / FNV — whose quests bind no aliases, so
    // the NPC owns nothing — the links are the only way a quest topic is ever
    // reached (Sunny Smiles's greeting links "Back in the Saddle"'s opener).
    let owned_quests = running_quests_binding_entity(world, npc);
    let owned = owned_topic_records(&index, &owned_quests);
    if offered.is_none() && !owned.iter().any(|owned| owned.form_id == topic_form_id) {
        return Err(format!(
            "NPC {npc} neither offers nor owns topic {topic_form_id:08X} through a running quest"
        ));
    }
    let player = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
        .ok_or_else(|| "no player".to_string())?;
    // After this line, its links become the list; a line that links nowhere
    // returns to the Top-Level topics (#5037).
    let Some(info) = offered.or_else(|| select_first_info(record, world, Some(npc), Some(player)))
    else {
        return Err(format!(
            "no INFO on topic {topic_form_id:08X} passes its conditions right now"
        ));
    };
    let menu = top_level_menu(&index, &owned, world, npc, player, Some((record, info)));
    let Some((topic, record)) = select_on_topic(&index, record, info, world, npc, player, &menu)
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

/// #5432 — the voice line's lifetime is source-pinned: a headless test
/// world has no audio manager, so nothing is ever queued to observe being
/// stopped. These pin the two points where a line stops being current.
#[cfg(test)]
mod voice_lifetime_source_tests {
    /// The production half of this file (everything before its first
    /// `#[cfg(test)] mod`), so these tests' own text cannot satisfy them.
    fn production_src() -> &'static str {
        byroredux_core::source_scan::production_text(include_str!("npc_dialogue.rs"))
    }

    fn body_of(src: &'static str, signature: &str) -> &'static str {
        let start = src
            .find(signature)
            .unwrap_or_else(|| panic!("{signature} is gone from npc_dialogue.rs"));
        let rest = &src[start..];
        &rest[..rest.find("\n}\n").expect("function closes at column 0")]
    }

    #[test]
    fn replacing_a_line_stops_the_old_voice_before_queueing_the_new_one() {
        let body = body_of(production_src(), "fn apply_selection(");
        let stop = body
            .find("dialogue_voice::stop_line_voice(")
            .expect("apply_selection must cut the outgoing line's voice (#5432)");
        let play = body
            .find("dialogue_voice::play_line_voice(")
            .expect("apply_selection plays the new line's voice");
        assert!(
            stop < play,
            "the stop must precede play_line_voice — stop_sounds_for also \
             dequeues the speaker's pending segments, which would cut the \
             new line"
        );
    }

    #[test]
    fn closing_the_conversation_stops_the_voice() {
        let body = body_of(production_src(), "pub(crate) fn end_open_conversation(");
        assert!(
            body.contains("dialogue_voice::stop_line_voice(world, npc)"),
            "end_open_conversation must cut the open line's voice (#5432)"
        );
    }
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

    #[test]
    fn selected_info_is_not_re_evaluated_after_menu_filtering() {
        use byroredux_plugin::esm::records::condition::{
            ComparisonOp, Condition, ConditionValue,
        };

        let mut record = fixture_topic();
        record.infos[0].conditions = vec![Condition {
            function_index: 77, // GetRandomPercent
            comparator: ComparisonOp::Lt,
            comparand: ConditionValue::Literal(101.0),
            ..Default::default()
        }];
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.dialogues.insert(record.form_id, record.clone());
        let owned = [index.dialogues.get(&record.form_id).unwrap()];
        let mut world = World::new();
        world.insert_resource(byroredux_scripting::story_manager::StoryManagerRng {
            state: 0xC0FFEE,
        });
        let npc = world.spawn();
        let player = world.spawn();

        let menu = top_level_menu(&index, &owned, &world, npc, player, None);
        let (selected_record, selected_info) = menu
            .first()
            .copied()
            .expect("the random-gated topic qualifies");
        let rng_after_menu = world
            .resource::<byroredux_scripting::story_manager::StoryManagerRng>()
            .state;

        let (selection, _) = select_on_topic(
            &index,
            selected_record,
            selected_info,
            &world,
            npc,
            player,
            &menu,
        )
        .expect("the INFO selected by the menu pass is spoken");

        assert_eq!(selection.info_form_id, selected_info.form_id);
        assert_eq!(
            world
                .resource::<byroredux_scripting::story_manager::StoryManagerRng>()
                .state,
            rng_after_menu,
            "selection must reuse the INFO chosen during filtering"
        );
    }

    /// #6007 — a topic click speaks the INFO the open menu offered. The menu
    /// pass admitted the entry because its INFO passed; the click used to
    /// evaluate the topic's INFOs again, so a gate that no longer held (a
    /// stochastic `GetRandomPercent` draw is the vanilla shape; a global is the
    /// deterministic stand-in here) refused the topic the player just clicked.
    #[test]
    fn a_topic_click_speaks_the_info_the_menu_offered() {
        use byroredux_plugin::esm::records::condition::{
            ComparisonOp, Condition, ConditionValue,
        };
        const GLOBAL: u32 = 0x20_0099;
        let (mut world, player, npcs) = bound_world(&[(SPEAKER_REF, SPEAKER_BASE)]);
        let eltrys = npcs[0];
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.dialogues.insert(TOPIC, fixture_topic());
        let mut rumors = fixture_second_topic();
        rumors.infos[0].conditions = vec![Condition {
            function_index: 74, // GetGlobalValue
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            param_1: GLOBAL,
            ..Default::default()
        }];
        index.dialogues.insert(TOPIC_RUMORS, rumors);
        world.insert_resource(crate::cell_loader::LoadedCellIndex(Arc::new(index)));
        let mut globals = byroredux_scripting::Globals::default();
        globals.set(GLOBAL, 1.0);
        world.insert_resource(globals);

        // The gate holds when the menu is built: Rumors is offered.
        world.insert(eltrys, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        let menu = selected(&world, eltrys).expect("opens").topics;
        assert!(
            menu.iter().any(|entry| entry.topic_form_id == TOPIC_RUMORS
                && entry.info_form_id == INFO_RUMORS),
            "the menu records the INFO it admitted each entry on: {menu:?}"
        );

        // Between the menu and the click the gate stops holding.
        world.resource_mut::<byroredux_scripting::Globals>().set(GLOBAL, 0.0);
        select_topic_by_form_id(&mut world, eltrys, TOPIC_RUMORS)
            .expect("the entry the player clicked answers with the INFO the menu offered");
        assert_eq!(selected(&world, eltrys).expect("selected").info_form_id, INFO_RUMORS);

        // A topic the NPC's menu does not list is still evaluated fresh.
        world.remove::<NpcDialogueTopic>(eltrys);
        assert!(
            select_topic_by_form_id(&mut world, eltrys, TOPIC_RUMORS).is_err(),
            "with no open menu the gate is evaluated, and it no longer holds"
        );
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

    // ── #5367 Phases G + L ──────────────────────────────────────────

    const GREETING_TOPIC: u32 = 0x30_00C8;
    const INFO_GREETING: u32 = 0x30_00C9;
    const INFO_GREETING_BYE: u32 = 0x30_00CA;

    fn greeting_index(with_goodbye: bool) -> crate::cell_loader::LoadedCellIndex {
        let bye_info = InfoRecord {
            form_id: INFO_GREETING_BYE,
            response_text: "Go away, stranger.".to_string(),
            data: Some(byroredux_plugin::esm::records::InfoDataHeader {
                flags1: 0x01, // Goodbye
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        let plain_greeting = InfoRecord {
            form_id: INFO_GREETING,
            response_text: "Hey there, stranger.".to_string(),
            // Say Once: the spoken set can disqualify this branch so the
            // Goodbye one below is reachable — the same discipline the
            // live masters author greetings with.
            data: Some(byroredux_plugin::esm::records::InfoDataHeader {
                flags1: 0x04,
                ..Default::default()
            }),
            ..Default::default()
        };
        index.dialogues.insert(
            GREETING_TOPIC,
            DialRecord {
                form_id: GREETING_TOPIC,
                editor_id: "GREETING".to_string(),
                infos: vec![plain_greeting, bye_info],
                ..Default::default()
            },
        );
        let _ = with_goodbye;
        crate::cell_loader::LoadedCellIndex(std::sync::Arc::new(index))
    }

    /// #5367 Phase G — a patron bound to no running quest still opens a
    /// conversation: the master's generic greeting is the opening line
    /// (pre-#5367 this activation selected nothing).
    #[test]
    fn quest_less_npc_opens_with_the_generic_greeting() {
        let mut world = World::new();
        world.register::<ActivateEvent>();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        world.insert_resource(DialogueRegistry::default());
        world.insert_resource(DialogueSurfaceState::default());
        let player = spawn_player(&mut world);
        let patron = spawn_actor(&mut world, 0xAB_0001, 0x97_0001);
        world.insert_resource(greeting_index(false));

        world.insert(patron, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);

        let topic = selected(&world, patron).expect("the greeting selects");
        assert_eq!(topic.topic_form_id, GREETING_TOPIC);
        assert_eq!(topic.info_form_id, INFO_GREETING);
        assert_eq!(topic.speaker_text, "Hey there, stranger.");
        // The greeting stands as its own list entry.
        assert_eq!(
            topic
                .topics
                .iter()
                .map(|entry| entry.topic_form_id)
                .collect::<Vec<_>>(),
            vec![GREETING_TOPIC]
        );
    }

    /// #6190 — a selection is the NPC speaking to the player, which is what
    /// `GetTalkedToPC` reads; before this nothing ever set the flag outside a test.
    #[test]
    fn a_selection_notes_the_speaker_as_having_talked_to_the_player() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        use byroredux_scripting::obscript_ledger::ObScriptLedger;
        let mut world = World::new();
        world.register::<ActivateEvent>();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        world.register::<FormIdComponent>();
        world.insert_resource(DialogueRegistry::default());
        world.insert_resource(DialogueSurfaceState::default());
        world.insert_resource(ObScriptLedger::default());
        let player = spawn_player(&mut world);
        let patron = spawn_actor(&mut world, 0xAB_0001, 0x97_0001);
        let mut pool = FormIdPool::default();
        let pair = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0xAB_0001),
        });
        world.insert_resource(pool);
        world.insert(patron, FormIdComponent(pair));
        world.insert_resource(greeting_index(false));
        assert!(!world.resource::<ObScriptLedger>().talked_to_player(0xAB_0001));

        world.insert(patron, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);

        assert!(selected(&world, patron).is_some(), "the greeting selects");
        assert!(world.resource::<ObScriptLedger>().talked_to_player(0xAB_0001));
    }

    /// #5642 — two greeters in radius on the same tick: the first opens
    /// its conversation, the second must hold. Pass 1 judged both `Open`
    /// against a pre-open snapshot, so the second open used to strip the
    /// first's line mid-sentence and spend both directives.
    #[test]
    fn two_same_frame_force_greeters_open_one_conversation() {
        use crate::systems::forcegreet::{forcegreet_system, ForceGreetDirective};
        use byroredux_core::ecs::components::{GlobalTransform, Transform};

        let mut world = World::new();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        world.register::<ForceGreetDirective>();
        world.register::<GlobalTransform>();
        world.register::<Transform>();
        world.insert_resource(DialogueRegistry::default());
        world.insert_resource(DialogueSurfaceState::default());
        world.insert_resource(greeting_index(false));
        let player = spawn_player(&mut world);
        world.insert(player, GlobalTransform::default());
        world.insert(player, Transform::default());
        let greeters = [
            spawn_actor(&mut world, 0xAB_0001, 0x97_0001),
            spawn_actor(&mut world, 0xAB_0002, 0x97_0002),
        ];
        for npc in greeters {
            world.insert(npc, GlobalTransform::default());
            world.insert(npc, Transform::default());
            world.insert(
                npc,
                ForceGreetDirective {
                    topic: None,
                    radius: crate::systems::forcegreet::FORCE_GREET_RADIUS,
                },
            );
        }

        forcegreet_system(&world, 0.016);

        let open: Vec<EntityId> = greeters
            .into_iter()
            .filter(|npc| selected(&world, *npc).is_some())
            .collect();
        assert_eq!(open.len(), 1, "exactly one greeter owns the conversation");
        assert_eq!(
            world.resource::<DialogueSurfaceState>().npc,
            Some(open[0]),
            "the surface belongs to the greeter that opened"
        );
        let pending: Vec<EntityId> = greeters
            .into_iter()
            .filter(|npc| world.get::<ForceGreetDirective>(*npc).is_some())
            .collect();
        assert_eq!(pending.len(), 1, "the held greeter keeps its directive");
        assert_ne!(pending[0], open[0], "the opener's directive was spent");
    }

    /// #5367 Phase L — a Goodbye line arms the timed close; once
    /// `TotalTime` passes the estimate, the Late system ends the
    /// conversation (OnEnd ran via `end_open_conversation`) and cues the
    /// page close.
    #[test]
    fn goodbye_line_ends_the_conversation_when_its_duration_passes() {
        let mut world = World::new();
        world.register::<ActivateEvent>();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        world.insert_resource(DialogueRegistry::default());
        world.insert_resource(DialogueSurfaceState::default());
        world.insert_resource(byroredux_core::ecs::resources::TotalTime(10.0));
        let player = spawn_player(&mut world);
        let patron = spawn_actor(&mut world, 0xAB_0001, 0x97_0001);
        world.insert_resource(greeting_index(true));

        // The Goodbye branch is the only passing INFO when the plain one
        // is disqualified — Say-Once it via the spoken set, which the
        // activation itself stamps.
        world.insert_resource(byroredux_scripting::DialogueSpokenInfoForms(
            [(0xAB_0001, INFO_GREETING)].into_iter().collect(),
        ));
        world.insert(patron, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        // The end-of-Late cleanup drains the marker; do the same so the
        // later runs exercise only the timed close, not a re-activation.
        if let Some(mut events) = world.query_mut::<ActivateEvent>() {
            events.remove(patron);
        }

        let topic = selected(&world, patron).expect("the goodbye line selects");
        assert_eq!(topic.info_form_id, INFO_GREETING_BYE);
        assert!(topic.goodbye);
        {
            let surface = world.resource::<DialogueSurfaceState>();
            let close_after =
                surface.close_after.expect("goodbye arms the timed close");
            assert!(
                close_after > 10.0,
                "close_after is now + the estimated duration, not now"
            );
            assert!(!surface.close_requested);
        }

        // Not yet due: nothing ends.
        world.insert_resource(byroredux_core::ecs::resources::TotalTime(10.5));
        npc_dialogue_selection_system(&world);
        assert!(selected(&world, patron).is_some(), "still open early");

        // Due: the conversation ends and the page close is requested.
        world.insert_resource(byroredux_core::ecs::resources::TotalTime(30.0));
        npc_dialogue_selection_system(&world);
        assert!(selected(&world, patron).is_none(), "the goodbye ended it");
        let surface = world.resource::<DialogueSurfaceState>();
        assert!(surface.close_requested);
        assert_eq!(surface.close_after, None);
    }

    /// #5867 — an open conversation ends when its partner dies, bleeds out or
    /// starts fighting: the page is asked to close and the selection clears,
    /// instead of listing a corpse's topics until the player closes it.
    #[test]
    fn a_conversation_ends_when_its_partner_can_no_longer_talk() {
        for kill in [
            |world: &mut World, npc| world.insert(npc, Dead),
            |world: &mut World, npc| {
                world.insert(npc, AiCombatState { target: npc, attack_cooldown_remaining: 0.0 })
            },
        ] {
            let mut world = World::new();
            world.register::<ActivateEvent>();
            world.register::<NpcDialogueTopic>();
            world.register::<SceneAliasCandidate>();
            world.register::<Dead>();
            world.register::<AiCombatState>();
            world.insert_resource(DialogueRegistry::default());
            world.insert_resource(DialogueSurfaceState::default());
            world.insert_resource(byroredux_core::ecs::resources::TotalTime(10.0));
            let player = spawn_player(&mut world);
            let patron = spawn_actor(&mut world, 0xAB_0001, 0x97_0001);
            world.insert_resource(greeting_index(true));
            world.insert(patron, ActivateEvent { activator: player });
            npc_dialogue_selection_system(&world);
            if let Some(mut events) = world.query_mut::<ActivateEvent>() {
                events.remove(patron);
            }
            assert!(selected(&world, patron).is_some(), "the conversation opened");
            assert!(!world.resource::<DialogueSurfaceState>().close_requested);

            kill(&mut world, patron);
            npc_dialogue_selection_system(&world);
            assert!(selected(&world, patron).is_none(), "the partner can no longer talk");
            let surface = world.resource::<DialogueSurfaceState>();
            assert!(surface.close_requested);
            assert_eq!(surface.npc, None);
        }
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
        // #5372 — the real boot registers the SM storages; without them
        // every hello-raising path silently no-ops and the lock-order
        // lane sees nothing of the dialogue-open acquisition chain.
        byroredux_scripting::story_manager::register(&mut world);
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

        // #5297 — the Late-stage spoken dispatch must survive the
        // end-of-frame batch drain and still run the new stage's QUST
        // fragment on the next Update. Pre-fix, `apply_spoken_info_fragment`
        // polled the fragment journal here in Late: the poll claimed the
        // SetStage-13 cursor, the re-emitted batch was drained unread by
        // `event_cleanup_system` at the end of the same stage, and the
        // dispatcher saw neither — the stage fragment never ran.
        let mut stage_fragments = byroredux_scripting::QuestStageFragments::default();
        stage_fragments.insert(
            QuestFormId(HELPER_QUEST),
            13,
            vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 14,
            }],
        );
        world.insert_resource(stage_fragments);
        // The real Late→Update ordering: cleanup drains the batch mirrors
        // first, then the dispatcher polls the journal.
        byroredux_scripting::event_cleanup_system(&world, 0.0);
        assert!(
            world
                .query::<byroredux_scripting::quest_stages::QuestStageAdvancedBatch>()
                .map(|query| query.iter().next().is_none())
                .unwrap_or(true),
            "the Late tail drained the batch mirrors"
        );
        byroredux_scripting::quest_fragment_dispatch_system(&world);
        assert_eq!(
            world
                .resource::<QuestStageState>()
                .get_stage(QuestFormId(HELPER_QUEST)),
            14,
            "the spoken line's SetStage-13 must still run its stage fragment \
             after the Late-stage batch drain (#5297)"
        );

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

    /// #5300 — re-selecting the SAME INFO ends the outgoing line first:
    /// its OnEnd runs, then OnBegin runs again. The old
    /// `existing.info_form_id != topic.info_form_id` filter skipped the
    /// outgoing OnEnd for an identical INFO, so a click on the topic that
    /// was just spoken replayed OnBegin with no OnEnd between.
    #[test]
    fn reselecting_the_same_info_fires_its_on_end_before_the_next_on_begin() {
        use byroredux_scripting::quest_stages::{QuestObjectiveState, QuestStageState};
        use byroredux_scripting::translate::compose::QuestRef;
        use byroredux_scripting::{DialogueInfoFragments, Effect};

        const HELPER_QUEST: u32 = 0x30_0001;
        let (mut world, player, npcs) = bound_world(&[(SPEAKER_REF, SPEAKER_BASE)]);
        let eltrys = npcs[0];
        world.register::<byroredux_scripting::quest_stages::QuestStageAdvancedBatch>();
        world.insert_resource(QuestObjectiveState::default());
        world
            .resource_mut::<QuestStageState>()
            .start_quest(QuestFormId(HELPER_QUEST), None);
        world.insert_resource(byroredux_scripting::papyrus_demo::PapyrusPlayerEntity(player));
        let mut fragments = DialogueInfoFragments::default();
        fragments.insert(
            INFO_ELTRYS,
            QuestFormId(HELPER_QUEST),
            None,
            Some(vec![Effect::SetStage { quest: QuestRef::SelfRef, stage: 13 }]),
            Some(vec![Effect::SetStage { quest: QuestRef::SelfRef, stage: 77 }]),
        );
        world.insert_resource(fragments);

        // Activation speaks the line: OnBegin only — nothing to end yet.
        world.insert(eltrys, ActivateEvent { activator: player });
        npc_dialogue_selection_system(&world);
        let stage_reached = |world: &World, stage: u16| {
            world
                .query::<byroredux_scripting::quest_stages::QuestStageAdvancedBatch>()
                .map(|query| {
                    query
                        .iter()
                        .flat_map(|(_, batch)| batch.0.to_vec())
                        .any(|advance| advance.quest.0 == HELPER_QUEST && advance.new_stage == stage)
                })
                .unwrap_or(false)
        };
        assert!(stage_reached(&world, 13), "OnBegin ran on activation");
        assert!(!stage_reached(&world, 77), "no OnEnd before any line was replaced");

        // The same topic (hence the same INFO) selected again.
        select_topic_by_form_id(&mut world, eltrys, TOPIC)
            .expect("the open topic re-selects");
        assert!(
            stage_reached(&world, 77),
            "the outgoing line's OnEnd must run before its OnBegin replays (#5300)"
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

    /// #5224 — a branch-less FO3/FNV topic is a list entry only when its
    /// DIAL `DATA` flags carry `Top-level` (0x02); the choice-only rest
    /// are reached through a link. Unknown flags (`None` — other games,
    /// or no `DATA`) keep the pre-#5224 shape.
    #[test]
    fn a_branch_less_fallout_topic_needs_the_top_level_flag() {
        let index = byroredux_plugin::esm::records::EsmIndex::default();
        let mut choice_only = fixture_topic();
        choice_only.data_flags = Some(0x00);
        let mut top = fixture_topic();
        top.data_flags = Some(0x02);
        let unknown = fixture_topic();
        assert_eq!(topic_entry(&index, &choice_only), TopicEntry::LinkOnly);
        assert_eq!(topic_entry(&index, &top), TopicEntry::TopLevel);
        assert_eq!(topic_entry(&index, &unknown), TopicEntry::TopLevel);
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

    // ── #5271 — per-INFO QSTI ownership on a multi-quest topic ──────

    const QUEST_OTHER: u32 = 0x0001_9CDE;
    const TOPIC_MULTI: u32 = 0x20_0030;
    const INFO_STOPPED: u32 = 0x20_0031;
    const INFO_RUNNING: u32 = 0x20_0032;
    const INFO_LOW_PRIO: u32 = 0x20_0033;
    const INFO_HIGH_PRIO: u32 = 0x20_0034;

    fn info_with_quest(form_id: u32, quest: u32, text: &str) -> InfoRecord {
        InfoRecord {
            quest,
            ..info(form_id, 0, text)
        }
    }

    fn multi_quest_world() -> (World, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<ActivateEvent>();
        world.register::<NpcDialogueTopic>();
        world.register::<SceneAliasCandidate>();
        world.register::<Dead>();
        world.insert_resource(DialogueRegistry::default());
        world.insert_resource(DialogueSurfaceState::default());
        let player = spawn_player(&mut world);
        let npc = spawn_actor(&mut world, SPEAKER_REF, SPEAKER_BASE);
        install_quest(&mut world, SPEAKER_REF, 1);
        (world, npc, player)
    }

    fn install_multi_quest_topic(world: &mut World, infos: Vec<InfoRecord>) {
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.dialogues.insert(
            TOPIC_MULTI,
            DialRecord {
                form_id: TOPIC_MULTI,
                editor_id: "DoctorMedical".to_string(),
                full_name: "Medical".to_string(),
                // One topic, two owning quests — the #5271 shape. The NPC
                // reaches it through QUEST (bound + running).
                quest_refs: vec![QUEST, QUEST_OTHER],
                infos,
                ..Default::default()
            },
        );
        world.insert_resource(crate::cell_loader::LoadedCellIndex(Arc::new(index)));
    }

    /// #5271 — the INFO whose own quest is stopped must not speak just
    /// because a sibling owner of the topic is running: the running
    /// quest's later INFO wins instead of file order.
    #[test]
    fn a_stopped_quests_info_does_not_speak_through_a_running_sibling() {
        let (mut world, npc, player) = multi_quest_world();
        install_multi_quest_topic(
            &mut world,
            vec![
                info_with_quest(INFO_STOPPED, QUEST_OTHER, "The stopped quest's line."),
                info_with_quest(INFO_RUNNING, QUEST, "The running quest's line."),
            ],
        );
        start_quest(&mut world);
        refresh_scene_actor_bindings(&world);
        world.insert(npc, ActivateEvent { activator: player });

        npc_dialogue_selection_system(&world);

        let topic = selected(&world, npc).expect("the running quest's INFO selects");
        assert_eq!(
            topic.info_form_id, INFO_RUNNING,
            "the stopped quest's INFO must not win by file order (#5271)"
        );
        assert_eq!(topic.owning_quest, Some(QUEST));
    }

    /// #5271 — when both quests run, the higher-priority quest's INFO wins
    /// regardless of file order, and the selection attributes the line to
    /// the spoken INFO's quest.
    #[test]
    fn a_higher_priority_quests_info_wins_in_a_multi_quest_topic() {
        let (mut world, npc, player) = multi_quest_world();
        install_multi_quest_topic(
            &mut world,
            vec![
                info_with_quest(INFO_LOW_PRIO, QUEST, "Priority 50 line."),
                info_with_quest(INFO_HIGH_PRIO, QUEST_OTHER, "Priority 80 line."),
            ],
        );
        let mut stages = QuestStageState::default();
        stages.start_quest(QuestFormId(QUEST), None);
        stages.start_quest(QuestFormId(QUEST_OTHER), None);
        world.insert_resource(stages);
        world.insert_resource(byroredux_scripting::DialogueQuestPriorities::from_quests(
            [(QUEST, 50), (QUEST_OTHER, 80)],
        ));
        refresh_scene_actor_bindings(&world);
        world.insert(npc, ActivateEvent { activator: player });

        npc_dialogue_selection_system(&world);

        let topic = selected(&world, npc).expect("a qualifying INFO selects");
        assert_eq!(
            topic.info_form_id, INFO_HIGH_PRIO,
            "quest priority orders the topic's INFOs, higher first (#5271)"
        );
        assert_eq!(
            topic.owning_quest,
            Some(QUEST_OTHER),
            "the selection names the SPOKEN line's quest, not the topic's first owner (#5271)"
        );
    }
}
