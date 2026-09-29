# #5141: TOOL-D1-2026-09-29-02: the native pause menu stops the debug-server drain; timed-out commands then run in a burst when the game resumes

**Labels**: medium, bug, tech-debt

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Debug Trust Boundary (exposure: developers using byro-dbg while paused, and any smoke that opens a native menu page)

## Location
- `byroredux/src/app_events.rs` (`about_to_wait`: `if !simulation_paused { self.scheduler.run(&self.world, dt); }`, two sites)
- `crates/debug-ui/src/lib.rs` (`DebugUiState::simulation_paused`)
- `crates/debug-server/src/lib.rs` (`DebugDrainSystem` is a `Stage::Late` exclusive inside that scheduler)
- `crates/debug-server/src/system.rs` (drain loop; only the screenshot path checks `cancel`)

## Description
The gating has existed since `5d47f0735` (2026-08-15); the 09-22 baseline missed it.
- While the Pause, Settings or Inventory page is open, `about_to_wait` skips `scheduler.run` entirely, so the drain system never runs.
- The listener still accepts and queues requests (up to 64). Each client gets "timeout waiting for engine response" after 30 s, or its REPL exits after 10 s (TOOL-D1-01).
- When the menu closes, the drain runs every queued command at once. Only screenshots honour the abandonment flag.
- A `SetField`, `cell.load`, `quest.setstage` or `inv.add` that the user retried after a timeout therefore fires once per attempt, long after the client was told it failed.
- `766e1746e` deliberately kept the dialogue page out of `simulation_paused` for this reason ("a running world keeps the debug-server drain alive so a route smoke can assert on the presented response").

## Evidence
`simulation_paused = game_menu.visible && page != Dialogue` → `if !simulation_paused { self.scheduler.run(..) }`. In the drain loop, `evaluator::evaluate(world, &self.registry, &cmd.request)` runs for every drained non-screenshot command with no `cmd.cancel` check.

## Impact
The pause menu is the most natural moment to inspect the world, and the debugger is dead exactly then. Duplicate, delayed mutations corrupt the state the developer is trying to debug.

## Related
TOOL-D1-2026-09-29-01, #1007 (cancel flag, screenshot-only), `766e1746e`. Label gap: debug server has no own label → `tech-debt`.

## Suggested Fix
Run the debug drain even while the simulation is paused (e.g. run `DebugDrainSystem` or a paused-mode drain outside the gated `scheduler.run`). Separately, skip any drained command whose `cancel` flag is already set before calling `evaluate`.

Validated at HEAD 9fcfdc3fc: both `if !simulation_paused` gates in `app_events.rs` still wrap `scheduler.run`; the drain loop's `cancel` check is screenshot-only.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other exclusive systems that must run while paused)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
