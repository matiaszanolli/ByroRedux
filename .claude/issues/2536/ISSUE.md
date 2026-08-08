# SAVE-D1-18: The #2295 completeness guard's source scan has zero visibility into byroredux/src -- the binary crate where GameTimeRes itself lives

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2536
**Finding ID**: SAVE-D1-18

**Severity**: MEDIUM
**Dimension**: 1 — Snapshot Completeness & Determinism
**Data-Loss Class**: none (the guard gap doesn't itself lose data — it removes the tripwire that would catch a *future* gap, recurring on a new axis from the now-closed SAVE-D1-12)
**Location**: `byroredux/src/save_io.rs:1945-1949` (`SCAN_ROOTS` inside `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`)
**Status**: NEW

## Description
The `#2295` guard (replacing the old NPC-spawn-only guard closed as SAVE-D1-12) is a real improvement — it recursively scans every `.rs` file under three `crates/` directories for `impl Component for X`/`impl Resource for X` and requires each `X` registered or allowlisted. But `SCAN_ROOTS` is:
```rust
const SCAN_ROOTS: &[&str] = &[
    "../crates/core/src/ecs/components",
    "../crates/scripting/src",
    "../crates/physics/src",
];
```
This never includes `byroredux/src/` itself — the binary crate this very test lives in. `GameTimeRes` (`byroredux/src/components/game_time.rs:117`, registered today), `PlayerPose`, `CurrentCellContext`, `SaveState`, and `PendingSaveLoadSlot` all live entirely outside the scan. 45 `impl Component for`/`impl Resource for` lines exist under `byroredux/src/` today, none of which any guard inspects.

## Evidence
Confirmed directly: `grep -rn "^impl Component for\|^impl Resource for" byroredux/src/` returns 45 matches, none reachable by `SCAN_ROOTS`. `GameTimeRes` — a type this audit was asked to verify as newly registered — is one of them: correctly registered, but not because the guard would have caught its absence.

## Impact
Any future save-relevant `Resource`/`Component` added directly to `byroredux/src/` (as `GameTimeRes` was) ships with zero automated tripwire, relying entirely on the author remembering the hand-add — the same discipline gap that produced the original SAVE-D1-08/09/10 findings, now on a different scope axis (file location instead of spawn-time-vs-runtime).

## Suggested Fix
Add a fourth scan root, `"../byroredux/src"`, to `SCAN_ROOTS`. Will require populating `NOT_SAVED_BY_DESIGN` with the ~40 remaining unregistered `byroredux/src/` types using the same one-line-reason convention — most already have adequate doc comments to lift a reason from. Budget as a follow-up with the same per-type care the original #2295 pass gave its first 85 entries.

## Completeness Checks
- [ ] **TESTS**: Guard test still passes after adding the scan root, with every newly-discovered type registered or allowlisted with a reason
