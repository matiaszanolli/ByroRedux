//! #5797 — the one Papyrus-call dispatch tail every interpreter system
//! shares.
//!
//! Ten dispatchers had grown the same hand-copied tail: budget check →
//! `defer` → `apply_fragment_guard_free` → player lookup →
//! `push_quest_stage_advances`. The budget was a property of whichever
//! dispatcher remembered to copy the gate (#5763 reached four; hits,
//! distance, package, perk and terminal ran unbudgeted), so the next
//! dispatcher defaulted to spending the whole frame. Both halves now
//! live here.

use super::budget;
use super::effects::apply_fragment_guard_free;
use crate::quest_stages::{QuestFormId, QuestStageAdvanced};
use crate::translate::effects::Effect;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::script_instance::ScriptInstanceData;

/// Dispatch one `Effect::Interpret` call under the frame budget: when the
/// allowance is spent the call queues for a later frame and yields no
/// advances this frame; when there is no continuation queue the effects
/// come back from `defer` and run now, exactly as before the budget
/// existed.
pub(crate) fn run_papyrus_call(
    world: &World,
    owner: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    effect: Effect,
) -> Vec<QuestStageAdvanced> {
    let effect = if budget::exhausted(world) {
        match budget::defer(world, owner, vmad, vec![effect]) {
            Ok(()) => return Vec::new(),
            Err(mut effects) => effects.remove(0),
        }
    } else {
        effect
    };
    apply_fragment_guard_free(world, std::slice::from_ref(&effect), owner, vmad)
}

/// The shared epilogue: hand the run's quest-stage advances to the player
/// sink. A world without a player entity drops them. The player id is
/// copied out of a statement-scoped temporary so the `PapyrusPlayerEntity`
/// guard is released before `push_quest_stage_advances` takes the batch
/// storage (#3580's lock-order rule).
pub(crate) fn flush_advances(world: &World, advances: Vec<QuestStageAdvanced>) {
    if advances.is_empty() {
        return;
    }
    let Some(player) = world
        .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
        .map(|player| player.0)
    else {
        return;
    };
    crate::quest_stages::push_quest_stage_advances(world, player, advances);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The epilogue's contract, pinned at its one home: no player entity
    /// drops the advances silently instead of panicking, and empty input
    /// is a no-op (the two guards every hand-copied tail re-spelled).
    #[test]
    fn flush_advances_drops_silently_without_a_player() {
        let world = byroredux_core::ecs::world::World::new();
        flush_advances(&world, Vec::new());
        flush_advances(
            &world,
            vec![QuestStageAdvanced { quest: QuestFormId(1), previous_stage: 0, new_stage: 10, stage_set: true }],
        );
    }
}
