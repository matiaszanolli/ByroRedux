# #1146 — ECS-D7-NEW-01: dead code in mg07_on_activate_system (papyrus_demo R5 follow-up)

Labels: bug, ecs, low, tech-debt
State: OPEN

## Source Audit
[`docs/audits/AUDIT_ECS_2026-05-16.md`](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/audits/AUDIT_ECS_2026-05-16.md) — Dimension 7 (script lifecycles / R5 demo)

## Severity
**LOW** — code quality only, no behavioral or perf impact.

## Location
`crates/scripting/src/papyrus_demo/mg07_door.rs:303-321, 394-395`

## Status
**NEW** — surfaced during the 2026-05-16 ECS audit (commit `58fe3ce4`).

## Description
Two unrelated dead-code blocks left from iterative development of the
MG07 R5 follow-up translation (commit `58fe3ce4`, R5 follow-up 3).

### Block 1 (lines 303-321) — pre-loop dead stage check

Acquires a `QuestStageState` resource read lock, performs a useless
`get_stage(QuestFormId(0))` lookup, assigns `stage_state_done_10 = true`
to a `bool` that's never read, and then `let _ = stage_state_done_10;`
to suppress an unused-variable warning. The real per-door stage
resolution happens inside the read-phase loop at line 367 — the
pre-loop block is a half-finished "hoist the stage check"
optimisation that was abandoned when the per-door quest FormID
requirement was discovered. The dead code was never cleaned up.

### Block 2 (lines 394-395) — unused pending Vec

Declares `let mut pending_my_door_activates: Vec<EntityId> = Vec::new();`
and immediately suppresses an unused-mut warning via
`let _ = &mut pending_my_door_activates;`. The Vec is never written
or read — the actual cross-reference activation is emitted by the
sibling `mg07_tick_system`, not by `mg07_on_activate_system`.

## Evidence

```rust
// mg07_door.rs:303-321 — dead block 1
pub fn mg07_on_activate_system(world: &World) {
    let player = world.resource::<PlayerEntity>().0;
    let stage_state_done_10: bool;
    {
        let stage_state = world.resource::<QuestStageState>();
        // ... comment explaining the abandoned optimisation ...
        let _ = stage_state.get_stage(QuestFormId(0));
        stage_state_done_10 = true; // unused; re-resolved per door
    }
    let _ = stage_state_done_10;
    // ...
}
```

```rust
// mg07_door.rs:394-395 — dead block 2
let mut pending_my_door_activates: Vec<EntityId> = Vec::new();
let _ = &mut pending_my_door_activates; // suppress unused-write warning
```

## Impact
- Pre-loop block does an extra resource-read lock acquire + drop +
  useless HashMap lookup every system invocation (negligible — single
  read lock, single hash miss).
- The `pending_my_door_activates` Vec is a one-time zero-capacity
  allocation per system invocation (also negligible).
- The real cost is **future-reader confusion** — both blocks look
  load-bearing at a glance and will distract anyone auditing the
  system later. The 2026-05-16 audit reviewer (me) was the first
  such reader; the cost is already realised once.

## Suggested Fix
Delete both blocks. The function then reads as:

```rust
pub fn mg07_on_activate_system(world: &World) {
    let player = world.resource::<PlayerEntity>().0;

    // Two-phase: collect (read), then apply (write).
    enum Outcome { /* ... */ }
    let mut outcomes: Vec<Outcome> = Vec::new();
    {
        // ... existing read-phase body ...
    }
    if outcomes.is_empty() {
        return;
    }
    // Phase 2 — apply.
    for outcome in &outcomes {
        // ... existing apply-phase body ...
    }
}
```

12 tests in `papyrus_demo/mg07_door/tests.rs` pass independently of
these blocks — they're functionally inert. The fix is a one-time
deletion; no regression test added needed (the existing 12 already
pin the system's observable behaviour).

## Completeness Checks
- [ ] **UNSAFE**: N/A — no unsafe in scope.
- [ ] **SIBLING**: Verify the four other R5 demo systems don't have
      similar leftover dead code (`rumble_on_activate_system`,
      `quest_advance_on_activate_system`, `dlc2_ttr4a_on_update_system`,
      `mg07_tick_system`).
- [ ] **DROP**: N/A — no Vulkan objects in scope.
- [ ] **LOCK_ORDER**: Block 1 acquires + drops a `QuestStageState`
      read lock for nothing — removing it strictly reduces lock
      pressure, no ordering risk.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Existing 12 MG07 tests pin behaviour through the
      planned deletion. No new test required.

## Related
- R5 follow-up 3 commit `58fe3ce4` (the introducing commit).
- AUDIT_ECS_2026-05-16 (this report's only finding).
