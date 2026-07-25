# 2182: D6-02: CLAUDE.md still documents the pre-cd2b5fe4 100-byte vertex layout

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2182
**Labels**: documentation, low, tech-debt

---

## Severity
LOW

## Dimension
Skinning & BLAS Cost (Dim 6) — `/audit-performance` 2026-07-25

## Location
`CLAUDE.md` (Vertex row in the Workspace Structure table)

## Description
`CLAUDE.md`'s Vertex row still describes "100 B (19 f32 + 4 u32 + 8 u8)". Commit `cd2b5fe4` widened the color field from `vec3` to `vec4`, making the current struct 104 B (test-pinned: `assert_eq!(size_of::<Vertex>(), 104)`, `crates/renderer/src/vertex.rs:320`).

## Impact
Doc-rot only, but `CLAUDE.md` is the project's primary onboarding reference read before every session.

## Related
PERF-D3-02 (filed separately — same stale 100 B figure in `memory-budget.md`, recommend fixing both together).

## Suggested Fix
Update the `CLAUDE.md` Vertex row to "104 B (20 f32 + 4 u32 + 8 u8)" (or the exact correct field breakdown) matching the current struct.

## Completeness Checks
- [ ] N/A — documentation-only fix
