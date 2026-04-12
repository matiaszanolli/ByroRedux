# Issue #241 — PERF-04-11-M4

**Title**: Alpha-blended geometry sorted front-to-back, not back-to-front
**Severity**: MEDIUM (correctness-adjacent)
**Dimension**: Draw Call Overhead
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `byroredux/src/render.rs:187-195`

## Summary
The draw-command sort within the alpha-blend group has no depth term. Overlapping transparent surfaces blend in arbitrary order, producing artifacts on stained glass / water / dense foliage. Add a `depth: u32` field, sort transparent draws back-to-front.

## Fix with
`/fix-issue 241`
