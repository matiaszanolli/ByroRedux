# NIFAL-D6-06: docs still cite import/collision.rs — function moved to import/collision/shape.rs post-#1876 split

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Collision · **Tier Violated**: (doc)
**Location**: `docs/engine/nifal.md:202,216`, `docs/engine/nif-parser.md:528`, `docs/engine/architecture.md`
**Status**: NEW

## Description

`docs/engine/nifal.md:202,216`, `nif-parser.md`, and `architecture.md` still
cite `import/collision.rs::resolve_shape` — the function lives in
`import/collision/shape.rs` post-`#1876` split, and the limitations table is
at the top of `import/collision/mod.rs`. Doc-only, no behavior impact.

## Evidence

`grep -n "import/collision.rs" docs/engine/nifal.md docs/engine/nif-parser.md`
returns hits at the lines above; `crates/nif/src/import/collision/` is a
directory (`mod.rs`, `shape.rs`, `ragdoll.rs`), not a single file.

## Impact

Doc-only — no behavior impact, but stale paths mislead future contributors
navigating from the spec to the code.

## Suggested Fix

Update all three docs to point at `import/collision/shape.rs::resolve_shape`
and `import/collision/mod.rs` for the limitations table.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only fix — no behavior change to pin)

## Filed as

GitHub issue #2301, labels: low, nif-parser, documentation.
