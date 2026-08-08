# SAVE-D2-19: SAVE_TYPE_SOURCES was not updated for six new save-participating source files -- guard has zero visibility into ~23 serde-derived types

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2537
**Finding ID**: SAVE-D2-19

**Severity**: MEDIUM
**Dimension**: 2 — Registry & (De)serialization Fidelity
**Data-Loss Class**: none today (latent silent-drop risk — same mechanism as the historical `#1714`/`#2181` bug class, currently un-triggered)
**Location**: `byroredux/src/save_io.rs:2644-2671` (`SAVE_TYPE_SOURCES` const); missing: `crates/core/src/ecs/components/material.rs`, `crates/core/src/ecs/components/collision.rs`, `crates/scripting/src/papyrus_demo/mod.rs`, `crates/scripting/src/cinematic.rs`, `crates/scripting/src/player_control.rs`, `crates/scripting/src/fragment.rs`
**Status**: NEW

## Description
`serde_default_on_saved_struct_requires_format_major_bump` (#1714/#2181) exists to catch a save-participating struct gaining `#[serde(default)]` without a `FORMAT_MAJOR` bump — exactly the drift class `schema_fingerprint()` structurally cannot see (type-key-only). Its coverage is a hand-maintained `SAVE_TYPE_SOURCES` file list, not a directory walk — unlike its sibling `#2295` registry-completeness guard, which does recursively scan. Cross-checking `SAVE_TYPE_SOURCES` against every type wired into `build_save_registry` this cycle found six source files (carrying ~23 serde-derived types, registered by the `#2378`/`#2379`/`#2380`/`#2381`/`#2382`/`c5202627` commit sequence) never added to the list. This is not hypothetical: the identical failure already happened once and was fixed as `#2015`/SAVE-D2-03 ("registered ActorValues for save but never added actor_values.rs to SAVE_TYPE_SOURCES") — it has now recurred across six files in the very next round of registrations.

## Evidence
Confirmed directly: `SAVE_TYPE_SOURCES` (`save_io.rs:2644-2671`) lists 26 source paths; none of `material.rs`, `collision.rs`, `cinematic.rs`, `player_control.rs`, `fragment.rs` (and `papyrus_demo/mod.rs`) appear. `grep -n "derive(.*Serialize"` on the six files shows 23 sites total that `SAVE_TYPE_SOURCES` never references — the guard silently skips all of them.

## Impact
If any future edit adds `#[serde(default)]` to a field on any of these six files' types, the guard test continues to pass green while an old save silently default-fills the changed field on load instead of failing loudly or triggering a `FORMAT_MAJOR` bump. Blast radius is zero today, but the false assurance is real.

## Related
`#2015`/SAVE-D2-03 (the identical failure, first occurrence — `actor_values.rs` missing from the same list).

## Suggested Fix
Add the six missing paths to `SAVE_TYPE_SOURCES` (mirroring the `#2015` fix exactly). Longer-term, replace the hand-maintained list with the same recursive-directory-scan pattern `#2295`'s guard already uses (`SCAN_ROOTS` + `collect_rs_files`) — reusing that machinery makes this class of gap structurally impossible to reintroduce a third time.

## Completeness Checks
- [ ] **TESTS**: `serde_default_on_saved_struct_requires_format_major_bump` re-run after adding the six paths, confirms it now scans all 23 additional types
- [ ] **SIBLING**: Consider replacing the hand-maintained list with `SCAN_ROOTS`-style directory walk to prevent a third recurrence
