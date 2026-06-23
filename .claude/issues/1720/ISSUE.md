# SAVE-D1-02: live apply_deltas path never restores the saved StringPool — latent dangling-symbol trap

Labels: bug low tech-debt 

- **Severity**: LOW
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: corruption-on-load (latent only)
- **Location**: `byroredux/src/save_io.rs:556-563` (live path = `restore_resources` + `apply_deltas`, no StringPool restore); contrast `crates/save/src/driver.rs:83` (`restore_world` does restore it)

## Description
The clear/restore path (`restore_world`) re-installs `StringPool::from_dump` so `FixedString` symbols resolve. The live load path deliberately does NOT (it overlays onto a reloaded cell that owns its own pool). This is **safe today** because every `MUTABLE_DELTA_COLUMNS` entry was verified free of `FixedString` fields. But there is no guard: adding any `FixedString`-bearing component (e.g. `Name`) to `MUTABLE_DELTA_COLUMNS` would overlay symbol indices that mean nothing in the reloaded pool — silent string corruption.

## Evidence
Live path calls only `restore_resources` (resources) + `apply_deltas` (components); no `StringPool::from_dump`. Delta columns grepped for `FixedString` in serialized fields — none.

## Impact
None today; a footgun for a future maintainer extending the delta set.

## Suggested Fix
Document the invariant ("delta columns must not carry `FixedString`/`EntityId`/session-handle fields") at `MUTABLE_DELTA_COLUMNS`, ideally with a compile-time or test guard.

## Completeness Checks
- [ ] **SIBLING**: The same invariant covers the `EntityId` hazard from SAVE-D6-01
- [ ] **TESTS**: A guard test fails if a `FixedString`-bearing column is added to `MUTABLE_DELTA_COLUMNS`
