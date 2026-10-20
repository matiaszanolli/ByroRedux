//! #5804 — the `evaluate_function` arms that read game state rather than
//! the Run-On: quest stages and variables, globals, script variables, scene
//! actions, the Hardcore flag, location records, and `GetRandomPercent`.
//! Every one is a `reads_run_on_entity: false` row of the catalog.

use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::Condition;

use crate::quest_stages::QuestFormId;

use super::resolve_entity_by_global_form_id;

pub(super) fn get_quest_running(condition: &Condition, world: &World) -> f32 {
    // GetQuestRunning(quest) → 1.0 while the quest is running.
    use crate::quest_stages::{QuestFormId, QuestStageState};
    let Some(state) = world.try_resource::<QuestStageState>() else {
        return 0.0;
    };
    // `is_running` alone — the predicate ObScript's `GetQuestRunning`,
    // dialogue ownership, SM pool eligibility and alias refresh all
    // use. `is_started` is plain map membership, so it stays true
    // after `Quest.Stop()` and after Complete/Fail; folding it in
    // made every `GetQuestRunning(X) == 0` gate permanently false
    // once X had run and stopped (#5636).
    let running = state.is_running(QuestFormId(condition.param_1));
    if running { 1.0 } else { 0.0 }
}

pub(super) fn get_global_value(condition: &Condition, world: &World) -> f32 {
    // GetGlobalValue(global) → the GLOB's current value.
    world
        .try_resource::<crate::globals::Globals>()
        .and_then(|g| g.get(condition.param_1))
        .unwrap_or(0.0)
}

pub(super) fn get_random_percent(world: &World) -> f32 {
    // GetRandomPercent() → [0, 100]. Deterministic per world in
    // tests: the engine has no seeded general RNG on the World,
    // so this rides the StoryManagerRng when present (SM nodes
    // are its native consumer) and the quest-random state
    // otherwise; both are seeded resources.
    use crate::story_manager::StoryManagerRng;
    if let Some(mut rng) = world.try_resource_mut::<StoryManagerRng>() {
        let value = rng.pick(101);
        value as f32
    } else {
        // No RNG installed (unit worlds): a constant mid-range
        // value keeps `GetRandomPercent < N` gates decidable
        // and is documented here rather than hidden.
        50.0
    }
}

pub(super) fn get_quest_variable(condition: &Condition, world: &World) -> f32 {
    // GetQuestVariable(quest, variable): the quest script's local in its
    // 1-based `SLSD` slot (`param_2`), 0 until a GameMode run assigns it.
    world
        .try_resource::<crate::obscript_quests::ObScriptQuestLocals>()
        .map_or(0.0, |locals| locals.get(condition.param_1, condition.param_2 as u16))
}

pub(super) fn get_stage(condition: &Condition, world: &World) -> f32 {
    // GetStage(quest_form_id). The current quest stage lives
    // in the [`crate::quest_stages::QuestStageState`] resource.
    // `param_1` is the quest FormID.
    use crate::quest_stages::{QuestFormId, QuestStageState};
    let Some(state) = world.try_resource::<QuestStageState>() else {
        return 0.0;
    };
    state.get_stage(QuestFormId(condition.param_1)) as f32
}

pub(super) fn get_stage_done(condition: &Condition, world: &World) -> f32 {
    // GetStageDone(quest_form_id, stage). param_1 = quest
    // FormID, param_2 = stage. Returns 1.0 when the stage
    // has been reached, 0.0 otherwise. The Bethesda
    // idempotency primitive — "this milestone fired before."
    use crate::quest_stages::{QuestFormId, QuestStageState};
    let Some(state) = world.try_resource::<QuestStageState>() else {
        return 0.0;
    };
    let quest = QuestFormId(condition.param_1);
    let stage = condition.param_2 as u16;
    if state.get_stage_done(quest, stage) {
        1.0
    } else {
        0.0
    }
}

pub(super) fn is_scene_action_complete(condition: &Condition, world: &World) -> f32 {
    // param_1 is a global-space SCEN FormID and param_2 is the
    // authored action index. The runtime retains completion history
    // until the next start because completed actions are removed from
    // the active set when their end phase closes.
    use crate::scene::{ScenePlayer, SceneRegistry};
    let Some(scene_entity) = world
        .try_resource::<SceneRegistry>()
        .and_then(|registry| registry.scene_entity(condition.param_1))
    else {
        return 0.0;
    };
    world
        .get::<ScenePlayer>(scene_entity)
        .map_or(0.0, |player| {
            if player.completed_actions.contains(&condition.param_2)
                || player.active_actions.iter().any(|action| {
                    action.action_index == condition.param_2 && action.completed
                })
            {
                1.0
            } else {
                0.0
            }
        })
}

pub(super) fn is_hardcore(world: &World) -> f32 {
    if world.try_resource::<byroredux_core::ecs::resources::HardcoreMode>()
        .is_some_and(|mode| mode.enabled) { 1.0 } else { 0.0 }
}

pub(super) fn get_vm_script_variable(condition: &Condition, world: &World) -> f32 {
    // Unlike most CTDAs, the object being inspected is an explicit
    // function parameter rather than the Run-On entity. CIS2 carries
    // the variable name; the numeric CTDA param_2 bytes are an
    // editor/runtime artifact and must not be interpreted as an id.
    let Some(variable) = condition.param_2_text else {
        return 0.0;
    };
    // The lowered `SetReferenceVariable` effect and the two native activator
    // systems keep a reference script's variables on the entity...
    let on_entity = resolve_entity_by_global_form_id(world, condition.param_1).and_then(|target| {
        world
            .get::<crate::vm_state::ScriptVariables>(target)
            .and_then(|variables| variables.get(variable))
    });
    // ...but a reference script the Papyrus VM runs keeps them under the
    // reference's FormID in the saved quest-variable store (#5985), so it
    // outlives the cell and the entity, and a reference that is not loaded
    // still has its variables. Same store and spelling `GetVMQuestVariable`
    // reads.
    on_entity
        .or_else(|| crate::quest_variables::quest_variable(world, QuestFormId(condition.param_1), variable))
        .unwrap_or(0.0)
}

pub(super) fn get_vm_quest_variable(condition: &Condition, world: &World) -> f32 {
    // The quest is an explicit parameter (remapped to global space at
    // parse, #1666); CIS2 names the variable.
    let Some(variable) = condition.param_2_text else {
        return 0.0;
    };
    crate::quest_variables::quest_variable(world, QuestFormId(condition.param_1), variable)
        .unwrap_or(0.0)
}
