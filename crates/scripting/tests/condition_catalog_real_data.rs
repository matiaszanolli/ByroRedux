//! #6251 — how much of FalloutNV.esm's dialogue the fail-closed condition
//! gates decline: an INFO, or a quest's dialogue conditions, naming a function
//! [`is_modeled`] rejects is never offered, and an objective target's marker
//! never shows. The counts are pinned exactly, so a
//! catalog that grows moves them down on purpose and one that narrows shows up.
//!
//! **Opt-in / `#[ignore]`d** — needs FalloutNV.esm on disk:
//! ```bash
//! cargo test -p byroredux-scripting --test condition_catalog_real_data -- --ignored
//! ```
//! Skips gracefully (passing) when the game data isn't present.

use std::path::PathBuf;

use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::condition::Condition;
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_plugin::esm::test_paths;
use byroredux_scripting::condition::is_modeled;

mod common;

fn fnv_index() -> Option<EsmIndex> {
    let dir = std::env::var(test_paths::FNV_ENV).unwrap_or_else(|_| test_paths::FNV_DEFAULT.into());
    let bytes = std::fs::read(PathBuf::from(dir).join("FalloutNV.esm")).ok()?;
    byroredux_plugin::esm::parse_esm(&bytes).ok()
}

#[test]
#[ignore = "needs FalloutNV.esm on disk"]
fn fnv_dialogue_declined_by_unmodeled_conditions() {
    let Some(index) = fnv_index() else {
        common::require_or_skip("FalloutNV.esm", &"fnv_index()");
        eprintln!("SKIP: FalloutNV.esm not found");
        return;
    };
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Fallout3NV));
    byroredux_scripting::install_actor_value_names(&mut world, &index);
    let declines = |conditions: &[Condition]| conditions.iter().any(|c| !is_modeled(&world, c));

    let infos: Vec<_> = index.dialogues.values().flat_map(|dial| dial.infos.iter()).collect();
    let declined_infos = infos.iter().filter(|info| declines(&info.conditions)).count();
    let declined_quests = index.quests.values().filter(|quest| declines(&quest.dialogue_conditions)).count();
    let targets: Vec<_> = index
        .quests
        .values()
        .flat_map(|quest| quest.objectives.iter().flat_map(|objective| objective.targets.iter()))
        .filter(|target| !target.conditions.is_empty())
        .collect();
    let declined_targets = targets.iter().filter(|target| declines(&target.conditions)).count();
    eprintln!(
        "declined INFOs {declined_infos} of {}, quests {declined_quests}, objective targets {declined_targets} of {} conditioned",
        infos.len(),
        targets.len()
    );

    // Before #6251: 5,968 INFOs, 2,305 of them only by functions the ObScript
    // VM already answered (GetItemCount, GetObjectiveCompleted/Displayed,
    // GetQuestCompleted, GetDeadCount, GetPCIsSex, IsInCombat,
    // GetHealthPercentage, GetIsCreature), leaving 3,662 — mostly GetIsVoiceType
    // (427) and GetScriptVariable (53). #5750 catalogued GetIsVoiceType (FO3/FNV
    // 427, Skyrim/FO4 426), which took 2,673 of those with it: what is left is
    // GetScriptVariable (53) and the like.
    assert_eq!(declined_infos, 989, "declined FNV INFOs moved");
    // Declined by GetIsVoiceType (16 of the 20 — #5750 catalogued it), IsChild
    // (365, foreign on FNV), GetTalkedToPC (50) and IsInList (372).
    assert_eq!(declined_quests, 4, "declined FNV quest dialogue gates moved");
    // Objective targets (`QSTA`): 284 of the 772 conditioned ones were declined,
    // 198 only by catalogued-since getters (GetDisabled ×94 among them). What
    // is left is PlayerInRegion (612), GetInCellParam (230) and the like.
    assert_eq!(targets.len(), 772, "conditioned FNV objective targets moved");
    assert_eq!(declined_targets, 86, "declined FNV objective targets moved");
}

fn skyrim_index() -> Option<EsmIndex> {
    let dir = std::env::var(test_paths::SKYRIM_SE_ENV)
        .unwrap_or_else(|_| test_paths::SKYRIM_SE_DEFAULT.into());
    let bytes = std::fs::read(PathBuf::from(dir).join("Skyrim.esm")).ok()?;
    byroredux_plugin::esm::parse_esm(&bytes).ok()
}

/// #5750 — Skyrim's generic Hello greeting (`DialogueGenericHello`, 60 INFOs,
/// 57 gated on `GetIsVoiceType`) must select by the speaker's voice type.
/// Before `GetIsVoiceType` (426) and `IsCommandedActor` (700) were catalogued
/// every gated INFO read 0.0, so all 5,118 non-creature bases shared the three
/// unconditioned lines — and the quest-level gate (`DialogueGeneric`: voice type
/// and `IsCommandedActor`) was unmodeled, which the fail-closed rule turns into
/// declining the whole quest.
///
/// Compares the *eligible sets* of two speakers with different voice types, not
/// one winner: the Hello INFOs are Random-flagged, so a pick is a draw from the
/// set.
#[test]
#[ignore = "needs Skyrim.esm on disk"]
fn skyrim_hello_greeting_selects_by_voice_type() {
    use byroredux_core::ecs::EntityId;
    use byroredux_scripting::condition::{evaluate, ConditionContext};

    let Some(index) = skyrim_index() else {
        common::require_or_skip("Skyrim.esm", &"skyrim_index()");
        eprintln!("SKIP: Skyrim.esm not found");
        return;
    };
    let hello = index.dialogues.get(&0x0001_42B5).expect("DialogueGenericHello");
    assert_eq!(hello.infos.len(), 60, "the Hello topic's INFO count moved");
    let quest = index.quests.get(&hello.quest_refs[0]).expect("owner quest DialogueGeneric");

    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Skyrim));
    byroredux_scripting::install_actor_base_traits(&mut world, &index);
    byroredux_scripting::install_form_lists(&mut world, &index);

    // One base per voice type, taken from the data: the first non-creature NPC_
    // (lowest FormID) authoring each of two voice types the topic gates on.
    let voice_of = |edid: &str| {
        index
            .voice_types
            .iter()
            .find(|(_, record)| record.editor_id == edid)
            .map(|(&form, _)| form)
            .unwrap_or_else(|| panic!("VTYP {edid} missing"))
    };
    let base_with_voice = |voice: u32| {
        index
            .npcs
            .iter()
            .filter(|(_, npc)| npc.voice_form_id == voice && npc.creature_stats.is_none())
            .map(|(&form, _)| form)
            .min()
            .unwrap_or_else(|| panic!("no NPC_ authors voice {voice:08X}"))
    };
    let (nord_f, orc_m) = (voice_of("FemaleNord"), voice_of("MaleOrc"));
    let speaker = |world: &mut World, base_form_id: u32| -> EntityId {
        let entity = world.spawn();
        world.insert(
            entity,
            byroredux_scripting::scene::SceneAliasCandidate { base_form_id, ..Default::default() },
        );
        entity
    };
    let nord = speaker(&mut world, base_with_voice(nord_f));
    let orc = speaker(&mut world, base_with_voice(orc_m));

    let eligible = |who: EntityId| -> std::collections::BTreeSet<u32> {
        let ctx = ConditionContext::for_subject(who);
        // The quest-level gate first, fail-closed exactly as `select_info` reads it.
        assert!(
            quest.dialogue_conditions.iter().all(|c| is_modeled(&world, c)),
            "DialogueGeneric's quest gate has an unmodeled term: {:?}",
            quest
                .dialogue_conditions
                .iter()
                .filter(|c| !is_modeled(&world, c))
                .map(|c| c.function_index)
                .collect::<Vec<_>>()
        );
        assert!(
            evaluate(&quest.dialogue_conditions, &world, &ctx),
            "the generic quest's gate must pass for a plain NPC speaker"
        );
        hello
            .infos
            .iter()
            .filter(|info| {
                info.conditions.iter().all(|c| is_modeled(&world, c))
                    && evaluate(&info.conditions, &world, &ctx)
            })
            .map(|info| info.form_id)
            .collect()
    };
    let (nord_set, orc_set) = (eligible(nord), eligible(orc));
    eprintln!("Hello eligible: FemaleNord {nord_set:08x?} MaleOrc {orc_set:08x?}");

    let unconditioned: std::collections::BTreeSet<u32> = hello
        .infos
        .iter()
        .filter(|info| info.conditions.is_empty())
        .map(|info| info.form_id)
        .collect();
    assert_eq!(unconditioned.len(), 3, "the three voice-agnostic lines moved");
    assert_ne!(nord_set, orc_set, "two voice types must not share one Hello pool");
    for (name, set) in [("FemaleNord", &nord_set), ("MaleOrc", &orc_set)] {
        assert!(
            set.difference(&unconditioned).next().is_some(),
            "{name}: every eligible Hello line is one of the three voice-agnostic INFOs — \
             the voice-gated lines are still unreachable"
        );
    }
}
