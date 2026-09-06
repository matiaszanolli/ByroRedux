# #3885: TD8-2026-09-05-02: `load_interior_cell` is a dead `pub fn` behind a dead re-export — the same synchronous-superseded-by-resumable-job pattern as #2266/#3747, one file over

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-02) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3885 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-02), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/cell_loader/transition.rs` (`load_interior_cell`, ~line 558–620), `byroredux/src/cell_loader.rs` (the `pub use transition::{ load_interior_cell, … }` block), `byroredux/src/cell_loader/load.rs` (an orphaned doc-comment cross-reference)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: `a7cc9184`, 2026-05-21 ("M40 Phase 2 Stage 3b: interior↔exterior cell-swap orchestration")

**Description**
`load_interior_cell` is a bare `#[allow(dead_code)] pub fn` — no justifying comment at all, unlike every other allow in this file's neighbourhood. It performs a synchronous, unbudgeted interior cell load. It was superseded by `InteriorCellApply::begin(…)` + `advance(…)` in the same file, which is what `byroredux/src/app_step.rs` actually drives. `byroredux` is a **binary crate**, so `pub` here reaches nothing: there is no external consumer and never can be.

This is structurally identical to #2266/#3747 (`spawn_npc_entity` / `spawn_prebaked_npc_entity`): an older synchronous entry point left tagged `allow(dead_code)` when the resumable job API landed, kept alive only by doc comments that point at it.

**Evidence**
```
$ grep -RIn "load_interior_cell" --include="*.rs" crates byroredux tools
  byroredux/src/cell_loader.rs:91          #  the `pub use` re-export
  byroredux/src/cell_loader/transition.rs:379  #  doc comment "Used by [`load_interior_cell`] and …"
  byroredux/src/cell_loader/transition.rs:429  #  doc comment
  byroredux/src/cell_loader/transition.rs:559  #  the definition
  byroredux/src/cell_loader/load.rs:163        #  doc comment
  →  zero call sites
```
The live path, for contrast: `InteriorCellRequest` is consumed at `byroredux/src/app_step.rs:912`, which feeds `InteriorCellApply::begin` (`transition.rs`), not `load_interior_cell`.

**Impact**
~60 LOC of unreachable code plus three doc comments that describe it as a live caller of `reposition_camera` / `finish_interior_cell_load`, misrepresenting who actually drives those helpers. Any future change to the interior-load contract must be made twice or silently diverge.

**Related**: #2266 / #3747 (same pattern, CLOSED), TD8-2026-09-05-08 (the `allow(unused_imports)` blanket that hides the dead re-export)

**Suggested Fix**
Delete `load_interior_cell`, drop it from the `pub use transition::{…}` list, and reword the three doc comments to name `InteriorCellApply::begin` / `finish_interior_cell_load` — the functions that actually call the helpers those comments describe.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
