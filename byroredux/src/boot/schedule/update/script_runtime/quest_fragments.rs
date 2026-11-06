//! Quest fragment dispatch, terminal stages and fragment continuations.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// Quest fragment dispatch, terminal stages and fragment continuations.
pub(super) fn register_quest_fragments_systems(scheduler: &mut Scheduler) {
    // SCR-D6-NEW-02 (#1768) — the runtime scripting systems that were
    // registered (component/resource) but never scheduled. Both ride
    // the same exclusive-in-Update lane as the demo dispatchers above:
    // exclusive registration holds the world serially, so the fragment
    // system's resource-lock sequence never composes with a parallel
    // neighbour. `quest_fragment_dispatch_system` is `&World`-only, so
    // it needs the dt-dropping wrapper; `recurring_update_tick_system`
    // already has the `(&World, f32)` shape and is added directly.
    fn quest_fragment_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::quest_fragment_dispatch_system(world)
    }
    // Dispatch quest fragments right after the advance that emits the
    // `QuestStageAdvanced` markers, before end-of-frame cleanup drains
    // them (populated live from parsed QUST VMAD fragments, #1739 / `8a70b81a`).
    scheduler.add_exclusive(Stage::Update, quest_fragment_dispatch);
    // QSDT Complete/Fail Quest flags and NAM0 successor quests apply after
    // the stage's own fragment has observed the transition.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::quest_terminal_stage_system,
    );
    // Latent Utility.Wait tails resume after their authored delay. Running
    // immediately after fresh fragment dispatch lets one timing path serve
    // both newly-suspended and already-pending continuations.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::fragment_continuation_system,
    );
}
