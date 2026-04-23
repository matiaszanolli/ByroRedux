# #571 — SK-D1-02: BSDynamicTriShape with data_size==0 silently imports zero triangles

**Severity:** LOW (dormant on shipped Skyrim SE content)
**Labels:** bug, low, nif-parser
**Source:** AUDIT_SKYRIM_2026-04-22.md
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/571

## Location
- `crates/nif/src/blocks/tri_shape.rs:484, 541-583`
- `crates/nif/src/import/mesh.rs:248-250`

## One-line
When `data_size == 0`, `parse` skips vertex+triangle reads; `parse_dynamic` populates vertices from the Vector4 array but leaves triangles empty. `extract_bs_tri_shape` bails silently. All vanilla facegen NIFs have non-zero `data_size`; malformed mod content would fail silently.

## Fix sketch
Add `log::warn!` when `data_size == 0` and `parse_dynamic` populates vertices — makes the silent failure audible.

## Next
`/fix-issue 571`
