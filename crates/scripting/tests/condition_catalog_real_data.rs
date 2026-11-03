//! #6251 — how much of FalloutNV.esm's dialogue the fail-closed condition
//! gates decline: an INFO, or a quest's dialogue conditions, naming a function
//! [`is_modeled`] rejects is never offered, and an objective target's marker
//! never shows. The counts are pinned exactly, so a
//! catalog that grows moves them down on purpose and one that narrows shows up.
//!
//! **Opt-in / `#[ignore]`d** — needs FalloutNV.esm (and Fallout3.esm, Oblivion.esm,
//! Skyrim.esm for their legs) on disk:
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
    //
    // #6460 then catalogued the ten script getters still missing (IsChild 365,
    // GetTalkedToPC 50, GetHasNote 382, IsInList 372, GetLocked 5, GetCurrentTime 18,
    // GetInSameCell 32, GetBaseActorValue 277, GetDestructionStage 471,
    // GetMapMarkerVisible 492), bridged to the VM's own bodies: 989 -> 650. #6558 bridged nine
    // more (GetIsCurrentPackage 161, IsInInterior 300, HasMagicEffect 214, GetSitting 159,
    // GetDetected 45, IsKiller 409, IsWeaponOut 101, GetRestrained 244, IsSneaking 286): 650 -> 641.
    assert_eq!(declined_infos, 641, "declined FNV INFOs moved");
    // Four quests were declined whole by IsChild (GenericAdultCombat, GenericKids),
    // IsInList (Followers) and GetTalkedToPC (DialogueCrimeKarma) until #6460.
    assert_eq!(declined_quests, 0, "declined FNV quest dialogue gates moved");
    // Objective targets (`QSTA`): 284 of the 772 conditioned ones were declined,
    // 198 only by catalogued-since getters (GetDisabled ×94 among them). What
    // is left is PlayerInRegion (612), GetInCellParam (230) and the like (#6460 took
    // GetDestructionStage 471 and GetMapMarkerVisible 492: 86 -> 70; #6558's nine: 70 -> 69).
    assert_eq!(targets.len(), 772, "conditioned FNV objective targets moved");
    assert_eq!(declined_targets, 69, "declined FNV objective targets moved");
}

fn fo3_index() -> Option<EsmIndex> {
    let dir = std::env::var(test_paths::FO3_ENV).unwrap_or_else(|_| test_paths::FO3_DEFAULT.into());
    let bytes = std::fs::read(PathBuf::from(dir).join("Fallout3.esm")).ok()?;
    byroredux_plugin::esm::parse_esm(&bytes).ok()
}

/// #6460 — the FO3 leg: before the script getters were catalogued, `IsInList` (372) and
/// `GetTalkedToPC` (50) alone declined 21 FO3 quests whole — 5,377 of 22,327 INFOs (24 %). Now no quest
/// is declined whole and 1,777 INFOs (8 %) were, by the functions still uncatalogued. #6558 bridged
/// nine more getters the VM answers — `GetIsCurrentPackage` (161), the gate on package-context barks,
/// led them — and 1,202 (5 %) are left, led by `GetScriptVariable` (53).
#[test]
#[ignore = "needs Fallout3.esm on disk"]
fn fo3_dialogue_declined_by_unmodeled_conditions() {
    let Some(index) = fo3_index() else {
        common::require_or_skip("Fallout3.esm", &"fo3_index()");
        eprintln!("SKIP: Fallout3.esm not found");
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
    eprintln!("FO3 declined INFOs {declined_infos} of {}, quests {declined_quests}", infos.len());
    assert_eq!(declined_infos, 1202, "declined FO3 INFOs moved");
    assert_eq!(declined_quests, 0, "declined FO3 quest dialogue gates moved");
}

fn oblivion_index() -> Option<EsmIndex> {
    let dir = std::env::var(test_paths::OBLIVION_ENV).unwrap_or_else(|_| test_paths::OBLIVION_DEFAULT.into());
    let bytes = std::fs::read(PathBuf::from(dir).join("Oblivion.esm")).ok()?;
    byroredux_plugin::esm::parse_esm(&bytes).ok()
}

/// #6554 — the Oblivion leg: four of the #6460 getters (`GetTalkedToPC` 50, `GetCurrentTime` 18,
/// `GetInSameCell` 32, `GetLocked` 5) are the same function at the same index on Oblivion, but were
/// bridged for FO3 / FNV only, so INFOs gated on them alone — the first-meeting `GREETING` lines
/// (`GetTalkedToPC == 0`) above all — were never selectable.
#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn oblivion_dialogue_declined_by_unmodeled_conditions() {
    let Some(index) = oblivion_index() else {
        common::require_or_skip("Oblivion.esm", &"oblivion_index()");
        eprintln!("SKIP: Oblivion.esm not found");
        return;
    };
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Oblivion));
    let declines = |conditions: &[Condition]| conditions.iter().any(|c| !is_modeled(&world, c));
    let infos: Vec<_> = index.dialogues.values().flat_map(|dial| dial.infos.iter()).collect();
    let declined_infos = infos.iter().filter(|info| declines(&info.conditions)).count();
    let greetings: Vec<_> = index
        .dialogues
        .values()
        .filter(|dial| dial.editor_id.eq_ignore_ascii_case("GREETING"))
        .flat_map(|dial| dial.infos.iter())
        .collect();
    let declined_greetings = greetings.iter().filter(|info| declines(&info.conditions)).count();
    eprintln!(
        "Oblivion declined INFOs {declined_infos} of {}, GREETING {declined_greetings} of {}",
        infos.len(),
        greetings.len()
    );
    assert_eq!(infos.len(), 19_278, "Oblivion's INFO count moved");
    // 3,172 before #6554; 533 of those were declined only by the four getters. What is left is led
    // by GetDisposition (76) and GetScriptVariable (53).
    assert_eq!(declined_infos, 2_639, "declined Oblivion INFOs moved");
    // Every Oblivion activation opens through GREETING (#5367): 941 of its INFOs were declined.
    assert_eq!(greetings.len(), 3_743, "Oblivion's GREETING INFO count moved");
    assert_eq!(declined_greetings, 450, "declined Oblivion GREETING INFOs moved");
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
