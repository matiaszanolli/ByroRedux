# Issue #3113: the new F5/F9 quicksave path calls SaveCommand::execute from the winit event handler, breaking the "sole caller is the exclusive lane" invariant its own comment names as the safety argument

- **Finding ID**: `ECS-2026-08-20-05`
- **Severity**: MEDIUM
- **Labels**: `medium,ecs,bug`
- **Source report**: `docs/audits/AUDIT_ECS_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3113

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3113 --json state`.

---

**Severity**: MEDIUM
**Dimension**: 1 — Lock Ordering & Deadlock / 7 — Component Lifecycles
**Source**: `docs/audits/AUDIT_ECS_2026-08-20.md` (`ECS-2026-08-20-05`)

**Location**: `byroredux/src/app_events.rs:285-302`, against `byroredux/src/save_io.rs:618-641`

## Description

`save_io::quicksave` is a one-line wrapper — `SaveCommand.execute(world, "")` — and this delta wired it
straight into `ApplicationHandler::window_event` on the F5 binding.

`SaveCommand::execute` carries an explicit, load-bearing comment stating that its very wide lock hold is
safe *only* because of who calls it (`save_io.rs:628-640`):

> `registry` (SaveRegistry) stays held through `save_world`/`encode` below, alongside the ~26
> component-storage + ~7 resource read locks `save_world`/`validate_world`/`validate_form_ids` take —
> the widest single-hold edge fan-out in the process. Safe today **only because command dispatch (the
> sole caller of `execute`)** runs on the exclusive `DebugDrainSystem` lane, so no parallel-lane system
> can ever form the other half of an ABBA cycle against it — same invariant as SCR-D6-NEW3-03 / #2126.
> Moving command dispatch off the exclusive lane, or adding a parallel system that also touches
> `SaveRegistry`, needs this re-derived. SAVE-D3-02 / #2154.

There is now a second caller, and it is not on that lane.

## Evidence

`byroredux/src/app_events.rs:288-296`:

```rust
let save_output = match save_action {
    Some(crate::interaction::InputAction::Quicksave) => {
        Some(crate::save_io::quicksave(&self.world))
    }
    Some(crate::interaction::InputAction::Quickload) => {
        Some(crate::save_io::quickload_latest(&self.world))
    }
    _ => None,
};
```

`byroredux/src/save_io.rs:618-620`:

```rust
pub fn quicksave(world: &World) -> CommandOutput {
    SaveCommand.execute(world, "")
}
```

## Impact

**No live deadlock.** winit `window_event` and `Scheduler::run` are both driven from the main thread, and
`run` joins its rayon batch before returning, so no ECS system is live while the handler executes.

But that is a *different* safety argument from the one the code documents, it is nowhere written down,
and nothing enforces it. The checked-in invariant now reads as satisfied while being factually false —
precisely the condition the #2154 comment was written to prevent. The equivalent hazard becomes real the
moment save is invoked from anywhere with a live scheduler: a scripted autosave system, a background
quicksave, or a debug-UI button dispatched off the drain lane.

## Related

- #2154 / SAVE-D3-02 (the invariant itself)
- #3022 (the P2-slice save work)

Not a duplicate of either — both predate this call site.

## Suggested Fix

Route the F5/F9 actions through the same deferred queue `quickload_latest` already uses
(`queue_load_slot`, `save_io.rs:832`) so the snapshot is taken on the drain lane. Then update the
`SaveCommand::execute` comment to name the *actual* set of callers rather than "the sole caller".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other caller of `ConsoleCommand::execute`
      reached from outside `DebugDrainSystem`, and `quickload_latest`'s own lock surface
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved — this is the
      widest single-hold fan-out in the process, so the ordering argument must be re-derived, not assumed
- [ ] **TESTS**: A regression test pins this specific fix
