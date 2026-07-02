# CONC-D4-02: DebugDrainSystem registered after the access-report/SystemList snapshot — omitted from sys.accesses/systems

_Filed as #1788 from `docs/audits/AUDIT_CONCURRENCY_2026-07-02.md`._

**Severity**: LOW · **Dimension**: Scheduler Access Declarations · Source: `AUDIT_CONCURRENCY_2026-07-02` (CONC-D4-02)

## Location
`byroredux/src/main.rs:1071` (snapshot) vs `byroredux/src/main.rs:1083` (registration); `crates/debug-server/src/lib.rs:33`.

## Description
`App::new` builds the scheduler, then `install_runtime_registries` (main.rs:1071) snapshots `scheduler.access_report()` and `scheduler.system_names()` into the `SchedulerAccessReport`/`SystemList` resources. Only afterwards does `byroredux_debug_server::start(&mut scheduler, …)` (main.rs:1083) add `DebugDrainSystem` via `add_exclusive(Stage::Late, drain_system)`. The drain system therefore never appears in the `sys.accesses` rows or the `systems` listing.

## Evidence
Install order main.rs:1070→1071→1083; `sys.accesses` reads the frozen resource, not a live report (world_info.rs:229-234).

## Impact
Introspection completeness only. `DebugDrainSystem` is exclusive, so it is never paired by the analyzer and the three startup asserts are unaffected (it did not exist when they ran; exclusive+undeclared is permitted by design, #1237). Checklist item "exclusive systems are listed in the report" fails for exactly this one system. Occurs on every debug-mode launch.

## Related
None.

## Suggested Fix
Either move the `SchedulerAccessReport`/`SystemList` snapshot after `debug_server::start()` (registration order permitting), or have `sys.accesses` note "+ debug-server drain (registered post-snapshot)." Cosmetic.

## Completeness Checks
- [ ] **SIBLING**: `systems` and `sys.accesses` outputs both reflect the fix (both read the same snapshot)
- [ ] **TESTS**: A `byro-dbg` `systems` check counts the Late-stage exclusive `DebugDrainSystem` row
