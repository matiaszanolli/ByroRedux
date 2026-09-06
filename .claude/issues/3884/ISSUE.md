# #3884: TD8-2026-09-05-01: The whole FormId→Entity single-root index subsystem is dead, and both milestones its `#[allow(dead_code)]`s name as gates closed on 2026-08-31

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-01) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3884 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-01), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/cell_loader/persistent_ref_index.rs` (`resolve_persistent_ref`, `invalidate`), `byroredux/src/cell_loader/cell_root_ref_index.rs` (`resolve_cell_root_ref`, `invalidate`), `byroredux/src/cell_loader/form_id_root_index.rs` (`resolve`), `byroredux/src/components.rs` (`PersistentRefIndex`, `CellRootRefIndex` — struct + 4 field-level allows), `byroredux/src/boot.rs` (the two `insert_resource` calls)
- **Status**: NEW (successor condition to #3455, CLOSED 2026-08-28)
- **Effort**: small (≤2 h)
- **Age**: `PersistentRefIndex` landed with EX-09/#2370; `CellRootRefIndex` + `form_id_root_index` split out later as its sibling. #3455 last re-justified them on 2026-08-28.

**Description**
Three modules (501 lines total, ~186 of them production), two ECS `Resource`s, two live `boot.rs` insertions and **8 `#[allow(dead_code)]` attributes** implement an `O(1)` FormId→Entity lookup scoped to a single `CellRoot`. Nothing in production has ever called any of it.

This is the second time the "landed ahead of its consumer" justification has been checked and found expired. #3455 (2026-08-27) established the rule for this exact code: EX-14/15 (#2369) had closed without wiring the index, so the comments were rewritten to name **EX-16 (#2372) as the one live gate**, and `persistent_ref_index.rs`'s module doc wrote down its own deletion condition verbatim:

> `//! #3455 — EX-14/15 (#2369) was the other named consumer and closed`
> `//! 2026-08-26 without wiring the index, so **EX-16 (#2372) is the only live`
> `//! gate**. […] If EX-16 reaches persistent refs by another route, delete this`
> `//! module, the PersistentRefIndex resource and the boot.rs insertion together —`
> `//! form_id_root_index::resolve stays live via CellRootRefIndex.`

**EX-16 (#2372) closed 2026-08-31T14:46:47Z**, and it too shipped without wiring the index. The stated deletion condition is now satisfied on the module's own terms.

Worse, the escape hatch in that same sentence is **false**: `form_id_root_index::resolve` does *not* stay live via `CellRootRefIndex`, because `CellRootRefIndex` is equally dead. A future auditor who trusts that line would delete half the subsystem and leave the other half — the exact failure mode `_audit-common.md`'s "a fact that rots becomes a false premise" note warns about.

**Evidence**
```
$ gh issue view 2369 --json state,closedAt   → CLOSED 2026-08-31T16:01:41Z   (EX-14/15)
$ gh issue view 2372 --json state,closedAt   → CLOSED 2026-08-31T14:46:47Z   (EX-16)

$ grep -RIn "resolve_persistent_ref\|invalidate\|resolve_cell_root_ref" --include="*.rs" crates byroredux tools
  → definitions + same-module tests only; zero production call sites

$ grep -RIn "form_id_root_index::resolve" --include="*.rs" byroredux
  byroredux/src/components.rs:1522            # doc comment
  byroredux/src/cell_loader/persistent_ref_index.rs:27   # doc comment (the false claim)
  byroredux/src/cell_loader/persistent_ref_index.rs:59   # inside the dead wrapper
  byroredux/src/cell_loader/cell_root_ref_index.rs:32    # inside the dead wrapper

$ grep -RIn "PersistentRefIndex\|CellRootRefIndex" --include="*.rs" byroredux | grep -v tests | grep -v components.rs
  byroredux/src/boot.rs:524:    world.insert_resource(crate::components::PersistentRefIndex::new());
  byroredux/src/boot.rs:525:    world.insert_resource(crate::components::CellRootRefIndex::new());
  # + the two `use` lines inside the dead modules themselves
```
`wc -l`: `persistent_ref_index.rs` 217, `cell_root_ref_index.rs` 180, `form_id_root_index.rs` 104.

**Impact**
Two `Resource`s occupy slots in every live `World` and are enumerated by the save-registry completeness list (`byroredux/src/save_io/registry_completeness_tests.rs`, `CellRootRefIndex` row) without ever holding data. Three modules with full test suites must be kept compiling and reviewed on every `cell_loader` refactor. The false "stays live via `CellRootRefIndex`" claim actively misdirects the next reader's deletion decision.

**Related**: #3455 (CLOSED — established the review rule this finding applies), #2369 / #2370 / #2372 (all CLOSED), #3833 (same "dead accessor kept for a future consumer" pattern, in the renderer)

**Suggested Fix**
Delete the three modules, both resource definitions and their two `boot.rs` insertions, plus the `mod` declarations in `cell_loader.rs`, the `CellRootRefIndex` row in `registry_completeness_tests.rs`, and the cross-references in `components.rs`. `World::find_by_form_id` / `resolve_entity_by_global_form_id` remain the live lookups. If a per-REFR index becomes necessary, the ~60-line `form_id_root_index::resolve` walk is trivially re-derivable from `git log`. If instead the team wants to keep it as scaffolding, the module docs must first be corrected — a new tracking issue must exist and the `CellRootRefIndex` escape-hatch sentence must be deleted, since it is untrue as written.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
