# #566 — SK-D6-02: LGTM lighting template fallback not wired

**Severity:** MEDIUM
**Labels:** bug, medium, legacy-compat
**Source:** AUDIT_SKYRIM_2026-04-22.md
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/566

## Location
- `crates/plugin/src/esm/cell.rs:560-707` (LTMP not parsed)
- `crates/plugin/src/esm/records/mod.rs:111` (lighting_templates extracted but unused)
- `byroredux/src/cell_loader.rs` (no fallback path)

## One-line
CELL.LTMP sub-record (FormID → LGTM) is never parsed. Cells that omit XCLL (Solitude inns, Dragonsreach throne room, Markarth) render with engine default ambient.

## Fix sketch
1. Parse LTMP (4-byte FormID) in `parse_cell_group`.
2. Add `lighting_template_form: Option<u32>` to `CellData`.
3. In `load_cell`, when `cell.lighting.is_none() && lighting_template_form.is_some()`, synthesize `CellLighting` from `index.lighting_templates`.

## Depends on
- #561 (SK-D6-01) so multi-master cell loading sees the full record set

## Next
`/fix-issue 566`
