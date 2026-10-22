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
    // GetHealthPercentage, GetIsCreature). What is left is mostly
    // GetIsVoiceType (427) and GetScriptVariable (53).
    assert_eq!(declined_infos, 3_662, "declined FNV INFOs moved");
    // Declined by GetIsVoiceType, IsChild (365, foreign on FNV), GetTalkedToPC
    // (50) and IsInList (372) — none a #6251 function, so unchanged by it.
    assert_eq!(declined_quests, 20, "declined FNV quest dialogue gates moved");
    // Objective targets (`QSTA`): 284 of the 772 conditioned ones were declined,
    // 198 only by catalogued-since getters (GetDisabled ×94 among them). What
    // is left is PlayerInRegion (612), GetInCellParam (230) and the like.
    assert_eq!(targets.len(), 772, "conditioned FNV objective targets moved");
    assert_eq!(declined_targets, 86, "declined FNV objective targets moved");
}
