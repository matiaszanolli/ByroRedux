# #1788 — CONC-D4-02: DebugDrainSystem registered after the access-report/SystemList snapshot — omitted from sys.accesses/systems

**Severity**: LOW · **Domain**: ecs (`byroredux` binary — scheduler wiring in `main.rs`)
**Location**: `byroredux/src/main.rs:1071` (snapshot) vs `byroredux/src/main.rs:1083`
(registration); `crates/debug-server/src/lib.rs:33`

`App::new` builds the scheduler, then `install_runtime_registries` snapshots
`scheduler.access_report()` and `scheduler.system_names()` into the
`SchedulerAccessReport`/`SystemList` resources. Only afterwards does
`byroredux_debug_server::start(&mut scheduler, …)` add `DebugDrainSystem` via
`add_exclusive(Stage::Late, drain_system)`. The drain system therefore never
appears in the `sys.accesses` rows or the `systems` listing — `sys.accesses`
reads the frozen resource snapshot, not a live report.

Impact: introspection completeness only. `DebugDrainSystem` is exclusive, so
it's never paired by the conflict analyzer; the three startup asserts are
unaffected. Occurs on every debug-mode launch.

Suggested fix: either move the `SchedulerAccessReport`/`SystemList` snapshot
after `debug_server::start()` (registration order permitting), or have
`sys.accesses` note "+ debug-server drain (registered post-snapshot)."
Cosmetic.

Completeness checks called out in the issue:
- SIBLING: `systems` and `sys.accesses` outputs both reflect the fix (both read the same snapshot)
- TESTS: a `byro-dbg` `systems` check counts the Late-stage exclusive `DebugDrainSystem` row

---

# #1789 — CONC-D6-01: Stale context/mod.rs line-number citations in acceleration/mod.rs::destroy() comments

**Severity**: LOW · **Domain**: renderer (`byroredux-renderer`)
**Location**: `crates/renderer/src/vulkan/acceleration/mod.rs:251-252,292-293`

`AccelerationManager::destroy()`'s doc comments cite `context/mod.rs:1300`,
`context/mod.rs:1859`, and `context/mod.rs:2093` as the locations of the
`device_wait_idle()` calls that make the immediate (non-deferred) destroys in
this function safe. Those line numbers predate the #1670/#1671 (`0409b6d6`)
and #1749 (`26439046`) refactors; the actual `device_wait_idle()` calls in the
current tree are at `context/mod.rs:2521` (`flush_pending_destroys`) and
`context/mod.rs:2836` (`Drop::drop`). The referenced invariant itself (drain
`pending_destroy_blas` + `skinned_blas` unconditionally, because an upstream
`device_wait_idle` already covers any in-flight reference) is still correct
and still held by both call sites — only the citation is stale.

Impact: none functionally — documentation/traceability defect only.

Suggested fix: update the two comment blocks in `acceleration/mod.rs` to cite
`context/mod.rs::flush_pending_destroys` / `context/mod.rs::Drop::drop` by
name/anchor rather than by line number (refactor-resistant).

Completeness checks called out in the issue:
- SIBLING: both comment blocks (:251-252 and :292-293) re-cited by name/anchor, not line number
- TESTS: N/A — documentation-only; verify with `grep -n "device_wait_idle" context/mod.rs`
