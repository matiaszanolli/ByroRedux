//! M47.2 — the Papyrus interpreter's per-frame budget (#5763).
//!
//! Every interpreter system (`reference_script_event_system`,
//! `quest_fragment_dispatch_system`, `fragment_continuation_system`,
//! `script_update_system`) is an exclusive `Update` system, so whatever the
//! VM spends in a frame is serial main-thread time. The 100 000-statement cap
//! in `vm.rs` bounds one run, not a frame: before this budget one runaway
//! handler cost 13.7 ms, several in a frame added up, and an `OnCellLoad`
//! burst over thousands of references ran to completion in the frame it was
//! raised.
//!
//! The unit is a **statement**, not a clock reading: a count is deterministic
//! (the tests assert on it) and a run's statement cost does not depend on
//! what else the machine is doing. A run also pays [`RUN_START_COST`] when it
//! starts, which stands for the fixed work around it (the scheduler snapshot,
//! the script lookup, the VMAD clone): the census put that at ~1.8 µs against
//! ~0.14 µs a statement.
//!
//! When the frame's allowance is gone:
//! - a run that has not started is **deferred** — queued unchanged on the
//!   [`FragmentExecutionQueue`] with no delay, ahead of nothing else, and
//!   started on a later frame;
//! - a run in progress **yields** at the next statement boundary: it
//!   suspends exactly as `Utility.Wait` does (same `VmResume` frames) with a
//!   zero delay, and carries the statements it has already used so the
//!   per-run cap still applies to the whole logical run.
//!
//! A handler that never finishes therefore neither starves the frame nor
//! runs forever: it is spread over frames, in queue order behind any event
//! raised earlier, and killed with a warning at the same 100 000 cumulative
//! statements the cap always allowed.
//!
//! What the budget cannot split: a statement inside a function called *for
//! its value* (a suspended frame has no way to hand a value back), and any
//! run whose frames hold an array or struct local (the saved continuation
//! stores scalars only — the same limit `Utility.Wait` has). Those finish in
//! the frame they started in, bounded by the per-run cap, and are charged to
//! the budget so the work queued behind them defers.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::script_instance::ScriptInstanceData;

use super::state::{FragmentExecutionQueue, FragmentResumeCondition, PendingFragmentExecution};
use crate::quest_stages::QuestFormId;
use crate::translate::effects::Effect;

/// Statements the interpreter may spend in one frame. At the census rate
/// (~0.14 µs a statement) this is ~1.4 ms; the real cells measured spend a
/// twentieth of it.
pub const DEFAULT_FRAME_BUDGET: usize = 10_000;

/// What starting a run costs, in statements.
pub const RUN_START_COST: usize = 16;

/// The interpreter's allowance for the current frame. Transient: rebuilt
/// every frame, never saved.
#[derive(Debug, Clone)]
pub struct VmFrameBudget {
    limit: usize,
    remaining: usize,
    spent: usize,
    deferred: u32,
    yielded: u32,
    last: VmFrameUse,
}

/// What the previous frame used, for diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VmFrameUse {
    pub spent: usize,
    /// Runs that had not started and were queued for a later frame.
    pub deferred: u32,
    /// Runs that started and suspended at a statement boundary.
    pub yielded: u32,
}

impl Resource for VmFrameBudget {}

impl Default for VmFrameBudget {
    fn default() -> Self {
        Self::with_limit(DEFAULT_FRAME_BUDGET)
    }
}

impl VmFrameBudget {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            remaining: limit,
            spent: 0,
            deferred: 0,
            yielded: 0,
            last: VmFrameUse::default(),
        }
    }

    /// Statements left this frame.
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// What was used so far this frame.
    pub fn used(&self) -> VmFrameUse {
        VmFrameUse {
            spent: self.spent,
            deferred: self.deferred,
            yielded: self.yielded,
        }
    }

    /// What the previous frame used.
    pub fn last_frame(&self) -> VmFrameUse {
        self.last
    }

    pub(crate) fn charge(&mut self, statements: usize) {
        self.spent += statements;
        self.remaining = self.remaining.saturating_sub(statements);
    }

    pub(crate) fn note_deferred(&mut self) {
        self.deferred += 1;
    }

    pub(crate) fn note_yielded(&mut self) {
        self.yielded += 1;
    }

    /// A new frame: the allowance is whole again.
    pub fn reset(&mut self) {
        self.last = self.used();
        self.remaining = self.limit;
        self.spent = 0;
        self.deferred = 0;
        self.yielded = 0;
    }
}

/// The frame has no interpreter allowance left. `false` when the world has
/// no budget (a test or tool that never installed one runs unbudgeted).
pub(crate) fn exhausted(world: &World) -> bool {
    world
        .try_resource::<VmFrameBudget>()
        .is_some_and(|budget| budget.remaining() == 0)
}

/// How many runs the rest of this frame's allowance can start (each costs at
/// least [`RUN_START_COST`] and a statement); unbounded when there is no budget.
pub(crate) fn runs_startable(world: &World) -> usize {
    world
        .try_resource::<VmFrameBudget>()
        .map_or(usize::MAX, |budget| budget.remaining() / (RUN_START_COST + 1))
}

/// Queue `effects` — an `Interpret` run that has not started, or a suspended
/// one — to run on a later frame. When the world has no continuation queue
/// the effects come back (`Err`), so a caller without one runs the work now
/// instead of dropping it.
pub(crate) fn defer(
    world: &World,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    effects: Vec<Effect>,
) -> Result<(), Vec<Effect>> {
    let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() else {
        return Err(effects);
    };
    // A full queue drops the newest tail (#5984): the work is shed, not run
    // unbudgeted, so this still reports success.
    queue.push(PendingFragmentExecution {
        context,
        vmad: vmad.cloned(),
        effects,
        remaining_seconds: 0.0,
        resume_when: FragmentResumeCondition::DelayElapsed,
    });
    drop(queue);
    if let Some(mut budget) = world.try_resource_mut::<VmFrameBudget>() {
        budget.note_deferred();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_budget_is_spent_and_renewed() {
        let mut budget = VmFrameBudget::with_limit(100);
        assert_eq!(budget.remaining(), 100);
        budget.charge(30);
        budget.charge(90);
        assert_eq!(budget.remaining(), 0, "an overshoot saturates");
        assert_eq!(budget.used().spent, 120);
        budget.note_deferred();
        budget.note_yielded();
        budget.reset();
        assert_eq!(budget.remaining(), 100);
        assert_eq!(budget.used(), VmFrameUse::default());
        assert_eq!(
            budget.last_frame(),
            VmFrameUse {
                spent: 120,
                deferred: 1,
                yielded: 1
            }
        );
    }
}
