# SF-D3-AUDIT-02: read_primitive_string omits the reference's trailing-NUL trim

**Severity**: LOW
**Labels**: low, import-pipeline, legacy-compat, bug
**Location**: `crates/sfmaterial/src/reader.rs:535-539` (`read_primitive_string`)
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF-D3-AUDIT-02)

## Description
Gibbed reads inline CDB strings with `trimNull=true`; the Rust port reads exactly `len` bytes with no NUL trimming, so a length-prefixed string whose window includes a terminating/embedded NUL would yield a `String` with an embedded `\0`, diverging from the reference. Not reachable in Phase 1 (no inline strings are read from the retained tree yet); vanilla data produced clean names in this run, so risk is latent, not active.

## Suggested Fix
Truncate at the first `0x00` within the read window before the lossy UTF-8 decode, matching the reference semantics.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
