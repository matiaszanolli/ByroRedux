# #3891: TD8-2026-09-05-08: Seven production `#[allow(unused_imports)]` on `cell_loader.rs`'s re-export blocks suppress the compiler's only dead-re-export detector — in a binary crate that has no external API surface to protect

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-08) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3891 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-08), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/cell_loader.rs` — the `pub use transition::{…}`, `pub(crate) use nif_import_registry::{…}`, `pub(crate) use refr::{…}`, `pub use exterior::{…}`, `pub(crate) use load::resolve_cell_lighting`, `pub use load::{…}`, `pub(crate) use object_lod::{…}` blocks
- **Status**: NEW
- **Effort**: small (≤2 h)

**Description**
`cell_loader.rs` carries 16 `#[allow(unused_imports)]` attributes. Eight are `#[cfg(test)]`-paired (test-visibility imports for child `mod`s — out of scope per the skill's cfg(test) exclusion), one more is a prose mention inside a comment. The remaining **seven sit on production re-export blocks** (`transition`, `nif_import_registry`, `refr`, `exterior`, `load::resolve_cell_lighting`, `load::{…}`, `object_lod`), justified by:

> `// Public re-exports — keep the existing crate::cell_loader::FOO call sites`
> `// … #[allow(unused_imports)] because not every re-exported item is consumed`
> `// by this crate's own binary — several only show up in external crates`
> `// (tests, other workspace members) or as the public API surface.`

`byroredux` is a **binary crate**. It has no external crates and no public API surface — the justification's second and third clauses cannot be true, and the first ("tests") is what the eight `cfg(test)` allows already cover. The net effect is that the one lint that can detect a dead re-export is switched off across most of the module's export surface.

Auditing what it hides: of the 37 names re-exported through those seven blocks, **three are dead re-exports** (referenced nowhere outside their defining module, in production or test):

| Re-exported name | Defining module | Consumers outside it |
|---|---|---|
| `load_interior_cell` | `transition.rs` | none — and the function itself is dead (TD8-2026-09-05-02) |
| `CellLoadPhaseTimings` | `load.rs` | none |
| `OneCellLoadInfo` | `exterior.rs` | none |

The rest are live and must stay. Two near-misses worth recording so a future sweep does not over-delete: `QueuedDoorTransition` / `QueueDoorTransitionError` appear at no call site by name but are the `Result` type of the live `queue_door_transition`, and `resolve_cell_lighting`'s re-export **is** load-bearing — `cell_loader/lgtm_fallback_tests.rs` reaches it through `use super::*`.

**Evidence**
```
$ grep -c '#\[allow(unused_imports)\]' byroredux/src/cell_loader.rs   → 16
# brace-walk pairing: 8 immediately preceded by #[cfg(test)], 1 is a prose mention,
# 7 sit on production re-export blocks (lines 89, 109, 113, 125, 130, 132, 144)

# per-name consumer count, excluding the defining module, cell_loader.rs itself, and test files:
  0  CellLoadPhaseTimings      0  OneCellLoadInfo      0  load_interior_cell
  1+ every other re-exported name
```

**Impact**
Structural, not cosmetic: this is *how* `load_interior_cell` survived from 2026-05-21 to today without a single compiler complaint, and it will hide the next one identically. Three dead re-exports today, unbounded tomorrow.

**Related**: TD8-2026-09-05-02 (the dead function this blanket concealed), #1322 / #2431 (dead re-exports found by hand in other crates, both CLOSED)

**Suggested Fix**
Delete the three dead re-exports, then remove the seven production `#[allow(unused_imports)]` attributes and let `cargo check` name whatever is left; re-add narrowly (per-name, with a reason) only where the compiler actually complains and the name is genuinely needed for a child test module's `use super::*`. Correct the justifying comment: it describes a library crate, and this is a binary.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
