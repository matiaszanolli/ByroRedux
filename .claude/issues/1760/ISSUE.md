# TD8-002/003: two dead pub fn in mesh.rs (oriented_quad, fullscreen_quad_vertices)

_Filed 2026-06-26 as #1760 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1760` for live state)._

**Severity**: LOW · **Dimension**: 8 — Dead Code
**Location**: `crates/renderer/src/mesh.rs:1170-1209` (`oriented_quad`) + re-export `crates/renderer/src/lib.rs:9`; `crates/renderer/src/mesh.rs:1211-1241` (`fullscreen_quad_vertices`)
**Status**: NEW · **Audit**: TD8-002 + TD8-003 (consolidated — two dead `pub fn` in mesh.rs)

## Description
Two dead public mesh-builder functions (verified 0 callers including tests/examples):

1. **`oriented_quad`** — introduced `7b8c0752` (Cornell harness) but `cornell.rs` ended up using `uv_sphere` + `box_vertices_colored`. `grep oriented_quad` → 2 hits (def + the `pub use` re-export only). The sibling `quad_vertices` has 3 live callers — confirms the pattern is real, this variant is not.
2. **`fullscreen_quad_vertices`** — introduced `340f1fbc` (M20 UI); `grep` → 1 hit (def only), not in the re-export list. The live sibling `fullscreen_quad_ui_vertices` (emits `UiVertex`) is the one used (resources.rs); the plain-`Vertex` version is refactor residue.

## Impact
~70 LOC of unused mesh-builder + one orphaned public re-export. Misleads readers into thinking they are part of the live mesh-primitive API. ByroRedux has no external consumers, so the re-export is pure surface rot.

## Suggested Fix
Delete `pub fn oriented_quad` (mesh.rs:1170-1209) and drop `oriented_quad` from the `pub use mesh::{…}` list in lib.rs:9. Delete `pub fn fullscreen_quad_vertices` (mesh.rs:1211-1241).

## Completeness Checks
- [ ] **SIBLING**: no test/example/bench references either fn (re-grep after delete)
- [ ] **TESTS**: `cargo build -p byroredux-renderer` clean (no unresolved re-export)
