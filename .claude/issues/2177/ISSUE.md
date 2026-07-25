# 2177: PERF-D3-02: memory-budget.md vertex stride stale at 100 B — actual is 104 B since cd2b5fe4

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2177
**Labels**: bug, low, performance

---

## Severity
LOW

## Dimension
GPU Memory Pressure (Dim 3) — `/audit-performance` 2026-07-25

## Location
`docs/engine/memory-budget.md`

## Description
`memory-budget.md` still cites the vertex stride as 100 B. Commit `cd2b5fe4` changed `Vertex`'s color field from `vec3` to `vec4`, widening the struct to 104 B (26 floats), confirmed by the test-pinned `assert_eq!(size_of::<Vertex>(), 104)` in `crates/renderer/src/vertex.rs:320`.

## Impact
Doc-rot only; every mesh-buffer VRAM estimate derived from the doc's 100 B figure undercounts by 4%.

## Related
D6-02 (filed separately — same stale 100 B figure in `CLAUDE.md`, recommend fixing both in one pass); PERF-D4-02, PERF-D3-01 (same root cause, same recommended documentation pass).

## Suggested Fix
Update the vertex stride figure to 104 B and re-derive any dependent mesh-buffer size estimates in the doc.

## Completeness Checks
- [ ] N/A — documentation-only fix
