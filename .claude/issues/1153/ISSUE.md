# TD7-051: CLAUDE.md claims Vertex is '25 floats'; actual is 19 floats + 4 u32 + 8 u8

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-17.md` — Dimension 7 (Stale Documentation)

## Severity
**LOW** — single-line doc drift in the project's main reference doc.

## Location
`CLAUDE.md:110`

## Description
The Vertex quick-reference reads:
> `Vertex (position + color + normal + uv + bone_idx + bone_wt + splat0/1 + tangent), 9 attribute descriptions, 100 B / 25 floats`

The size (100 B) and attribute count (9) are correct. The float count is wrong: actual is **19 floats + 4 u32 (bone indices) + 8 u8 (splat weights)** — bone indices are integers, splat weights are bytes.

## Proposed Fix
Update line 110 to:
> `Vertex (position + color + normal + uv + bone_idx + bone_wt + splat0/1 + tangent), 9 attribute descriptions, 100 B = 19 floats + 4 u32 (bone indices) + 8 u8 (splat weights)`

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Cross-check `crates/renderer/src/vertex.rs` doc-comment for the same drift; cross-check ROADMAP.md if it mentions Vertex stride
- [ ] **DROP**: N/A
- [ ] **TESTS**: N/A (doc only)
