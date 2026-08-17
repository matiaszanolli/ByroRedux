# SAVE-D2-02: SAVE_TYPE_SOURCES missing two files, wrong module for a third

**Issue**: #3025
**Severity**: MEDIUM
**Dimension**: 2 — format/versioning tripwires
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_SAVE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAVE_2026-08-16.md` (Dimension 2 — format/versioning tripwires).

**Location**: `byroredux/src/save_io/serde_default_guard_tests.rs`:14-54 (`SAVE_TYPE_SOURCES`)

## Description

`SAVE_TYPE_SOURCES` — the file list the `serde(default)` guard scans — is missing two defining files and points at the **wrong module** for a third. This is the **fourth recurrence** of the #2015 class (a hand-maintained source list drifting behind the code it is supposed to cover).

## Evidence

The list names `"../crates/scripting/src/scene.rs"`, but `scene.rs` is a thin re-export; the save-participating type lives in `crates/scripting/src/scene/quest_alias.rs`, which carries a `#[cfg_attr(feature = "save", serde(skip, default))]` at :88 that the guard therefore never sees.

Re-verified 2026-08-17: `scene/quest_alias.rs` appears nowhere in the 32-entry list.

Combined with #3020 — where the scanner cannot parse the `cfg_attr` form at all — the guard is blind along **two independent axes**: it looks at the wrong files, and it cannot recognise the attribute even in the right ones.

## Impact

The `FORMAT_MAJOR` tripwire's coverage is smaller than it appears. Every future save type added to a module not in the list is silently unguarded, and the failure mode is a save-format compatibility break that ships undetected.

## Suggested Fix

Fix the three entries. Then remove the hand-maintained list as a class of bug: derive the scanned set from the save registry (`save_io.rs`'s `register_*` calls) or walk the directories, so a new save type cannot be added without being covered.

Given this is the fourth recurrence, the list itself is the defect — not its current contents.

## Related

- #2015 (the original instance of this class), plus its two prior recurrences
- #3020 (SAVE-D2-2026-08-16-01 — the other axis of the same guard's blindness; fix together)

## Completeness Checks
- [ ] **ROOT-CAUSE**: The hand-maintained list is derived or generated, not just corrected a fourth time
- [ ] **SIBLING**: Fixed alongside #3020 — either alone leaves the guard blind
- [ ] **RE-EXPORT-TRAP**: The list points at defining modules, not thin re-export shims
- [ ] **TESTS**: Adding a save type in a new module fails the guard until covered

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3025 --json state` when live state is needed.*
