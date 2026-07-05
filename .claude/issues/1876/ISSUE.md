# #1876 — TD1-2026-07-05-01: crates/nif/src/import/collision.rs is 2587 LOC (no open split issue)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1876
**Labels**: bug, nif-parser, low, tech-debt
**Filed via**: /audit-publish docs/audits/AUDIT_TECH_DEBT_2026-07-05.md

---

- **Severity**: LOW
- **Dimension**: 1 (File Complexity)
- **Location**: `crates/nif/src/import/collision.rs`
- **Status**: NEW
- **Age**: crossed 2000 pre-2026-07-03 (noted-but-unfiled at 2370 LOC); grew 2370→2587 via `ae083d69` (the #1832 zero-mass-Dynamic collision fix + its diagnostic logging).

## Description

4th-largest file in the workspace (2587 LOC), no open split issue. The
block-parser side already split by bhk shape family under
`crates/nif/src/blocks/collision/` (mod / collision_object / rigid_body /
ragdoll / shape_primitive / shape_compound / shape_mesh / compressed_mesh /
constraints / phantom_action); the *import* side (`resolve_shape_inner`,
`extract_from_classic` / `extract_from_np` / `extract_from_phantom`,
`extract_ragdoll`, and the new diagnostic helpers) has not.

## Evidence

`wc -l crates/nif/src/import/collision.rs` → 2587 (threshold is the
Session-34 split target of 2000).

## Impact

Every collision-import edit — a recurring hot path, touched twice in the
2026-07-05 session alone — pays the whole-file review/merge tax.

## Related

Sibling of the closed Session-34/35 module splits; no open issue. Mirrors
the existing `crates/nif/src/blocks/collision/` split axis. Distinct from the
renderer split issues #1857/#1749/#1858.

## Suggested Fix

Split by shape family / responsibility, mirroring
`crates/nif/src/blocks/collision/` — e.g.
`import/collision/{mod, shape_resolve, rigid_body, ragdoll, diagnostics}.rs`.
Keep each `#[cfg(test)]` module (dispatch / cycle / coord / ragdoll-extract
tests) with its owning submodule.

## Completeness Checks
- [ ] **SIBLING**: `extract_collision` public entry point + `pub` re-exports preserved so `crates/nif/src/import/mesh/*` call sites are unaffected
- [ ] **TESTS**: the existing dispatch/cycle/coord/ragdoll-extract test modules move with their code and still pass (`cargo test -p byroredux-nif`)
