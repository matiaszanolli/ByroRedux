# Issue #254 — PERF-04-11-L9

**Title**: NifStream::read_sized_string allocates String unconditionally
**Severity**: LOW
**Dimension**: NIF Parse
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/nif/src/stream.rs:195-199`

## Summary
`read_string` returns `Arc<str>` (fixed in #55), but its cousin `read_sized_string` still always allocates `String`. Used by header parsing and pre-20.1 inline content. Return `Arc<str>` for consistency.

## Fix with
`/fix-issue 254`
