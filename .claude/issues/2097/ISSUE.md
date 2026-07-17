# LZ4-01: LZ4 decompress relies on undocumented-safe dependency behavior the crate itself disclaims as "may panic"

**Severity**: LOW
**Labels**: low, dependencies, import-pipeline, bug
**Location**: `crates/bsa/src/ba2.rs:692-696` (comment), `:717-724` (call site)
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (LZ4-01)

## Description
A comment asserts the LZ4 branch "inherently size-checks" and hard-errors on a size mismatch, but pinned `lz4_flex 0.11.6`'s own docs state the `decompress` function "may panic" if `min_uncompressed_size` undershoots the true decompressed size — a stronger guarantee than the dependency's public contract promises. Empirical fuzzing (constructed LZ4 payloads, undersized from 1 byte to 0) found zero panics on the currently pinned version — not an active bug, but an unpinned assumption that could silently regress on a future `lz4_flex` upgrade.

## Impact
None today. A future dependency bump could crash the engine on a malformed/adversarial v3 BA2 chunk record, with no code change on this side to explain why.

## Suggested Fix
Wrap the call in `catch_unwind` and convert a caught panic into the existing `Err` path, or pin the safety claim to `lz4_flex 0.11.6` with a version-gated regression test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
