# Issue #3497 — SAVE-D1-2026-08-27-03: the SAVE-D1-12 completeness guard's `SCAN_ROOTS` cannot notice a new crate — `crates/sdk` is unscanned

Source audit: `docs/audits/AUDIT_SAVE_2026-08-27.md`
Filed: 2026-08-27 (HEAD `969d81c8`)
Labels: medium, save-load, test-gap, bug

---

Audit: `docs/audits/AUDIT_SAVE_2026-08-27.md` (SAVE-D1-2026-08-27-03)
Severity: **MEDIUM** · Dimension 1 — Snapshot Completeness & Determinism
Data-Loss Class: latent silent-drop (no loss today — `StudioSession` is authoring-tool state, correctly not save-worthy)

## Location
- `byroredux/src/save_io/registry_completeness_tests.rs:362-369` — `SCAN_ROOTS`
- `crates/sdk/src/studio.rs:120` — `impl Resource for StudioSession {}`

## Status
NEW — made possible by `21a840d5` ("feat: introduce byroredux-sdk"), the first new workspace crate to define ECS state since the guard was written.

## Description
The guard's scan set is:

```rust
const SCAN_ROOTS: &[&str] = &[
    "../crates/core/src",
    "../crates/scripting/src",
    "../crates/physics/src",
    "../crates/audio/src",
    "../crates/plugin/src",
    "../byroredux/src",
];
```

It has a strong self-defence against a root *moving* (`collect_rs_files` panics on an unreadable directory — *"moved — update SCAN_ROOTS"* — and a `!found.is_empty()` assert catches the impl-line shape changing) but none at all against a root that was never added. `crates/sdk/src/studio.rs:120` declares `impl Resource for StudioSession {}`, and `StudioSession` is neither registered in `build_save_registry` nor listed in `NOT_SAVED_BY_DESIGN`. The guard is green because it simply never looks there (`grep -c sdk byroredux/src/save_io/registry_completeness_tests.rs` → 0).

`StudioSession` itself is correctly excluded on the merits — it is a Studio authoring document holding `Vec<EntityId>` / `Option<EntityId>` / a `BTreeMap<EntityId, TransformValue>`, all session-local identity, installed only when the Studio host is active (`byroredux/src/app_events.rs:163-168` opens the Studio panel when it is present). So there is no live data loss. The defect is that the ledger's *coverage* silently shrank relative to the workspace, in exactly the way the ledger exists to prevent.

## Evidence
`grep -rn --include='*.rs' "^impl Component for \|^impl Resource for " crates/ | grep -vE "^crates/(core|scripting|physics|audio|plugin|save)/"` returns four hits: `crates/sdk/src/studio.rs:120` (`StudioSession`), `crates/debug-ui/src/lib.rs:179` (`DebugUiState`), and `crates/renderer/src/vulkan/allocator.rs:49,70` (`AllocatorResource`, `GpuMemoryBudget`). The last three are unambiguously renderer/overlay infrastructure and predate the guard; `StudioSession` is the new one, and it is the only one of the four that carries a *document* rather than a device handle. `_audit-common.md`'s crate list is 25 entries against the guard's six roots.

## Impact
The SDK is described in `_audit-common.md` as *"the first tooling API surface"* and has no owner audit skill of its own. If Studio grows a document field that is genuinely game state (a persisted scene edit, a per-asset material override the engine should reload), it will land unnoticed by the one guard whose job is to notice exactly that. The cost of the miss compounds: the guard's green run is cited in this report and every prior one as "the completeness ledger", so an unscanned crate is not merely unchecked, it is affirmatively reported as checked.

## Related
#2295 / #3166 (the guard and its last `SCAN_ROOTS` widening); `21a840d5`; the "ByroRedux SDK — no dedicated owner" row in `_audit-common.md`'s un-owned-subsystems table; #3457 (the sibling doc-rot instance — `_audit-common.md`'s Project Layout gives `crates/sdk` no row).

## Suggested Fix
Replace the hardcoded list with a discovery step — enumerate `crates/*/src` from the workspace root and subtract an explicit, reasoned `NOT_SCANNED` set (`renderer`, `debug-ui`, `ui`, `save` itself, the parser-only crates) — so adding a crate forces a deliberate classification instead of silently widening the blind spot. Failing that, add `"../crates/sdk/src"` now and give `StudioSession` a `NOT_SAVED_BY_DESIGN` entry ("Studio authoring document holding session-local `EntityId`s; the edited world state it describes is saved through the normal component columns").

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the other three out-of-root `impl Resource` sites (`DebugUiState`, `AllocatorResource`, `GpuMemoryBudget`) get an explicit classification too
- [ ] **TESTS**: A regression test pins this specific fix (a new crate defining ECS state must fail the guard until classified)
