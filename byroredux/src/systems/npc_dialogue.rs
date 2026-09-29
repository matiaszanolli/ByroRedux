//! P4 blocker 1 — NPC activation → topic selection (MS01 fixture).
//!
//! The authored NPC→topic edge is `DialRecord::quest_refs` (QSTI): an NPC
//! "owns" the dialogue topics of every *running* quest whose live alias
//! bindings name it. This system consumes the player's `ActivateEvent` on
//! such an NPC, walks that edge, and picks the first INFO the M47.1
//! evaluator passes — the same `select_info` discipline the SCEN dialogue
//! path uses, with the activated actor as subject and the player as target.
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
use byroredux_plugin::esm::records::DialRecord;
use byroredux_scripting::{
    running_quests_binding_entity, select_first_info, ActivateEvent, DialogueRegistry,
    SceneAliasCandidate,
};

use crate::cell_loader::LoadedCellIndex;
use crate::systems::PlayerEntity;

/// The activation-driven topic selection on an NPC — one per activation,
/// overwritten by the next. Runtime interaction state, never serialized
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
}

impl Component for NpcDialogueTopic {
    type Storage = SparseSetStorage<Self>;
}

/// One activation's computed selection, applied after all reads drop (the
/// shared two-pass convention of this crate's exclusive systems).
struct TopicSelection {
    npc: EntityId,
    topic: NpcDialogueTopic,
    record: DialRecord,
}

/// Reusable per-frame scratch (mirrors `WalkAnimScratch`'s shape).
#[derive(Default)]
struct NpcDialogueScratch {
    selections: Vec<TopicSelection>,
}

fn npc_dialogue_selection_system_inner(world: &World, scratch: &mut NpcDialogueScratch) {
    // ── Pass 1: read-only gather + decide. ──
    scratch.selections.clear();
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
        // the dead are corpse-loot territory, not conversation partners.
        if activator != player || npc == player || world.get::<Dead>(npc).is_some() {
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
        // Deterministic order: running quests ascending, then DIAL form id —
        // the same record the fixture route must stably select every run.
        let mut selection: Option<(DialRecord, NpcDialogueTopic)> = None;
        'quests: for quest in &owned_quests {
            let mut topics: Vec<&DialRecord> = index
                .dialogues
                .values()
                .filter(|record| record.quest_refs.contains(&quest.0))
                .collect();
            topics.sort_unstable_by_key(|record| record.form_id);
            for record in topics {
                let Some(info) = select_first_info(record, world, Some(npc), Some(player)) else {
                    continue;
                };
                selection = Some((
                    record.clone(),
                    NpcDialogueTopic {
                        topic_form_id: record.form_id,
                        topic_editor_id: record.editor_id.clone(),
                        info_form_id: info.form_id,
                        owning_quest: Some(quest.0),
                        speaker_text: info.response_text.clone(),
                        response_number: info.response_number,
                        emotion_type: info.emotion_type,
                    },
                ));
                break 'quests;
            }
        }
        let Some((record, topic)) = selection else {
            continue;
        };
        log::info!(
            "npc dialogue: activation selected topic {:#08X} ('{}') info {:#08X} \
             for NPC {npc} (quest {:?})",
            topic.topic_form_id,
            topic.topic_editor_id,
            topic.info_form_id,
            topic.owning_quest,
        );
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
    if let Some(mut registry) = world.try_resource_mut::<DialogueRegistry>() {
        for selection in &scratch.selections {
            registry.insert_topic(selection.record.clone());
        }
    }
    if let Some(mut topics) = world.query_mut::<NpcDialogueTopic>() {
        for selection in &scratch.selections {
            topics.insert(selection.npc, selection.topic.clone());
        }
    }
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
        SceneActorBindings,
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
            dial_type: 0,
            infos: vec![
                // Speaker-locked first branch, then the shared branch.
                info(INFO_ELTRYS, SPEAKER_BASE, "You took the note, then."),
                info(INFO_GENERIC, 0, "Watch yourself in this city."),
            ],
            ..Default::default()
        }
    }

    fn install_index(world: &mut World) {
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.dialogues.insert(TOPIC, fixture_topic());
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
        // The registry holds the selected record for the presentation side.
        let registry = world.resource::<DialogueRegistry>();
        assert_eq!(registry.topic(TOPIC).expect("installed").form_id, TOPIC);
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
}
