# Issue #245 — PERF-04-11-M9

**Title**: NiUnknown::data allocated via read_bytes then never read
**Severity**: MEDIUM
**Dimension**: NIF Parse
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/nif/src/blocks/mod.rs:576, :592`; `lib.rs:168`

## Summary
Unknown-block skip path calls `read_bytes` which copies the block into a fresh `Vec<u8>` that nobody reads. ~200 KB wasted per 200-NIF cell. Replace with `stream.skip(size)` on all three call sites; gate the data copy behind a debug flag.

## Sibling
#248 (same `NiUnknown` struct redesign — bundle them)

## Fix with
`/fix-issue 245`
