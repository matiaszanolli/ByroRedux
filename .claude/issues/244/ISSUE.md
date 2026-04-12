# Issue #244 — PERF-04-11-M8

**Title**: anim::import_sequence clones strings via .to_string() in hot loop
**Severity**: MEDIUM
**Dimension**: NIF Parse
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/nif/src/anim.rs:289-344`

## Summary
~9 `.to_string()` calls per ControlledBlock × ~20 blocks per sequence × 50 clips = ~9000 stray `String` allocs per cell load. The parser already stores names as `Arc<str>`. Change downstream channel types (`TransformChannel`, etc.) to store `Arc<str>` / `FixedString`.

## Fix with
`/fix-issue 244`
