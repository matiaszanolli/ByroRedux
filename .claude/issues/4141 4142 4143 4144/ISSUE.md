## #4141 [OPEN] SAVE-D2-2026-09-11-02: save_type_sources() discovery roots are a stale hardcoded 5-crate list
labels: bug, medium, save-load

### SAVE-D2-2026-09-11-02: `save_type_sources()`'s discovery roots are a stale hardcoded 5-crate list, not the dynamic scan the sibling completeness guard was rewritten to use after proving the static-list approach misses real crates

- **Severity**: MEDIUM
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: Latent — a future `#[serde(default)]` or silent field-shape change on a save-participating type registered from outside the 5 scanned roots would bypass both SAVE-D2 shape/serde-default guards entirely, reproducing exactly the silent-corruption class those guards exist to prevent.
- **Location**: `byroredux/src/save_io/serde_default_guard_tests.rs:45-79` (`save_type_sources()`) vs. `byroredux/src/save_io/registry_completeness_tests.rs:51-84` (`discover_scan_roots()`)
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: `save_type_sources()` still uses five hardcoded roots (`byroredux/src`, `crates/core/src`, `crates/plugin/src`, `crates/scripting/src`, `crates/physics/src`) plus four explicit non-turbofish edges. This covers every currently-registered type today, but it is the exact shape of guard `registry_completeness_tests.rs` no longer trusts for itself: `#3497` replaced that file's own hardcoded `SCAN_ROOTS` with `discover_scan_roots()` — a live enumeration of every `crates/*/src` directory — specifically because the static list "has no defense against a root never being added," and doing so immediately surfaced four real types the old list had missed. `serde_default_guard_tests.rs`'s `save_type_sources()` was never given the same treatment. `crates/sdk` in particular (per this repo's `CLAUDE.md`: no owner audit skill) is a plausible future home for a save-participating type; if one lands there or in any other unscanned crate, both shape/serde-default guards will silently never scan it.

**Evidence**: Root-list comparison confirmed directly during publish — `save_type_sources()` hardcodes exactly the 5 roots named above at `serde_default_guard_tests.rs:50-56`; `registry_completeness_tests.rs:470` comment directly documenting the prior static-list miss.

**Impact**: No live corruption today (nothing currently registered lives outside the 5 roots), but the guard-coverage gap is structural and will not fail loudly when the next save-participating type lands in an unscanned crate.

**Related**: `#3497` (the sibling fix this guard was never given).

**Suggested Fix**: Extract `discover_scan_roots()` into a shared location both test modules call, so `save_type_sources()` scans every workspace crate the same way.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — e.g. a variant of `discover_scan_roots_finds_every_workspace_crate_and_byroredux` run against `save_type_sources()`'s output
- [ ] **SIBLING**: Confirm no third copy of the same hardcoded-roots pattern exists elsewhere in `save_io/*_tests.rs`


## #4142 [OPEN] SAVE-D2-2026-09-11-03: no round-trip test for Effect::SetLocked/SetLockLevel
labels: bug, low, save-load, test-gap

### SAVE-D2-2026-09-11-03: no save/load round-trip test exercises `Effect::SetLocked`/`Effect::SetLockLevel` — the specific v22 shape change — through an actual serialize/deserialize cycle

- **Severity**: LOW
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: Test-gap only.
- **Location**: `byroredux/src/save_io/round_trip_tests.rs:757` (`fragment_execution_queue_survives_save_load_round_trip_and_resumes`)
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: The one existing `FragmentExecutionQueue` round-trip test queues `Effect::Wait`/`ProviderCall`/`SetHudCartMode` — never `SetLocked`/`SetLockLevel`, the two variants that motivated the v22 bump. Confirmed via `grep -n "SetLocked\|SetLockLevel" byroredux/src/save_io/round_trip_tests.rs` — no hits.

**Impact**: Low — both are plain-data variants using already-proven-serializable types, so a serde failure is unlikely, but the checklist's round-trip-coverage bar is unmet for the exact shape change `FORMAT_MAJOR` exists to protect.

**Related**: SAVE-D2-2026-09-11-01 (same commit/variants).

**Suggested Fix**: Add `SetLocked`/`SetLockLevel` values to the existing round-trip test or a sibling.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — a `FragmentExecutionQueue` round-trip test queuing `Effect::SetLocked`/`Effect::SetLockLevel` and asserting the restored values match


## #4143 [OPEN] SAVE-D2-2026-09-11-04: ReferenceEnableState has no save/load serde round-trip test
labels: bug, low, save-load, test-gap

### SAVE-D2-2026-09-11-04: `ReferenceEnableState` is registered (with a correct `ValidateFn`) but has no save/load serde round-trip test — only a source-order text-scan test for restore ordering

- **Severity**: LOW
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: Test-gap only.
- **Location**: `byroredux/src/save_io.rs:501` (registration); `byroredux/src/save_io/live_reload_tests.rs:407` (the #3789 ordering test — a text-position check, not a data round trip)
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: No test constructs a populated `ReferenceEnableState`, saves it, decodes it, and asserts the restored value matches. `crates/save/tests/round_trip.rs`'s crate-level fixture doesn't include it either. Confirmed via `grep -n "ReferenceEnableState" byroredux/src/save_io/round_trip_tests.rs byroredux/src/save_io/live_reload_tests.rs` — the only hits are in `live_reload_tests.rs`, all doc-comment/ordering-check text, none a data-level assertion.

**Impact**: Low — no evidence of an actual serde defect, but a `FormId`-keyed map is exactly the shape class worth extra scrutiny per the Dimension 2 checklist.

**Related**: None.

**Suggested Fix**: Add a data-level round-trip test mirroring the pattern used for `FragmentExecutionQueue`/`PapyrusProviderContinuationQueue`/`CinematicPresentationState`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — populate `ReferenceEnableState`, run it through save/decode, assert equality


## #4144 [OPEN] SAVE-D4-2026-09-11-02: validate_cinematic_entity_refs doc comment stale (four checks, AnimationPlayer only)
labels: documentation, low, save-load, doc-rot

### SAVE-D4-2026-09-11-02: `validate_cinematic_entity_refs`'s doc comment still enumerates "four reference-class checks" and describes `validate_animation` as covering only `AnimationPlayer` — both stale

- **Severity**: LOW
- **Dimension**: 4 — Validation Gates
- **Data-Loss Class**: none (doc rot — no functional impact)
- **Location**: `byroredux/src/save_io.rs:725-736`
- **Status**: NEW. Introduced at `90ae915c` (#2535) when `validate_world` genuinely had four checks; never updated across every subsequent addition, including this cycle's `validate_animation` extension (#3791).
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: `validate_world` runs seven core checks today (confirmed directly: `validate_hierarchy`, `validate_equipment`, `validate_saved_entity_references`, `validate_animation`, `validate_inventory_instances`, `validate_progression_state`, `validate_material_finiteness` — `crates/save/src/validate.rs:71-79`), not four; `validate_animation` covers `AnimationPlayer`, `AnimationStack`, and `Seated.animation_restore.clip_handle`, not "only `AnimationPlayer`" as the comment claims seven lines from the very fix (#3791) that changed it.

**Impact**: Documentation-only; risk is to a future auditor/contributor taking the enumeration at face value.

**Related**: None open.

**Suggested Fix**: Update the comment to name the two hazards this function adds without re-enumerating `validate_world`'s internals (the enumeration is what keeps going stale).

## Completeness Checks
- [ ] **TESTS**: No test change needed — comment-only fix


