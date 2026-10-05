# #5292: TOOL-D1-2026-10-05-01: The #5141 paused-frame drain is keyed by a duplicated string literal and its "found" result is discarded

Labels: low,tech-debt,bug,test-gap
Filed from: docs/audits/AUDIT_TOOLING_2026-10-05.md

**Source**: `docs/audits/AUDIT_TOOLING_2026-10-05.md` (TOOL-D1-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: developer only (regression-guard quality)
- **Location**: `byroredux/src/app_events.rs:980-981`; `crates/debug-server/src/system.rs:207-209`; `crates/core/src/ecs/scheduler.rs:545-558` (`run_exclusive_named`)
- **Status**: NEW (test-gap)
- **Description**: The paused path calls `self.scheduler.run_exclusive_named(&self.world, dt, "debug_drain_system")` and ignores the returned `bool`.
  - The name is the hand-typed copy of `DebugDrainSystem::name()`'s `"debug_drain_system"`.
  - The only test, `run_exclusive_named_runs_only_the_named_system`, registers its own `CountingSystem` under a third copy of the literal.
  - No test ties the engine call to the debug-server's system name.
- **Evidence**: `grep -rn debug_drain_system` returns exactly `app_events.rs:981`, `system.rs:208` and two lines inside the scheduler test. There is no shared constant.
- **Impact**: If `DebugDrainSystem::name()` is renamed, `run_exclusive_named` returns `false` and nothing reports it. The #5141 bug comes back unseen: commands queued under the pause menu time out, then fire in a burst on unpause (cancelled ones are now skipped, so the remaining harm is the timeouts). The `false` result is also legitimate in a build without the `debug-server` feature, which is presumably why it is ignored.
- **Related**: #5141, `b7bc84722` (where the engine hunk actually landed)
- **Suggested Fix**: Export `pub const DRAIN_SYSTEM_NAME: &str` from `byroredux-debug-server`, use it in both `name()` and the engine call, and add `debug_assert!` on the result under `#[cfg(feature = "debug-server")]`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
