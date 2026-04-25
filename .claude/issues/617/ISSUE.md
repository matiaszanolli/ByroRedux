# SK-D2-06: BSA v105 (LZ4 + 24-byte folder records + u64 offsets) has no unit-test coverage

## Finding: SK-D2-06

- **Severity**: MEDIUM
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`
- **Location**: [crates/bsa/src/archive.rs:533-722](crates/bsa/src/archive.rs#L533) (existing test module)

## Description

Every `#[ignore]`'d on-disk test in `archive.rs` points at FNV (v104/zlib). The audit's exact scope — v105 + LZ4 frame format + 24-byte folder records + u64 offsets — has zero unit-level coverage. End-to-end works empirically (#569 covers the on-disk full-archive sweep), but a regression in any v105-specific code path would only surface against on-disk archives, not in CI.

## Suggested Fix

Two complementary fixes:

1. **Synthetic v105 fixture** — small in-memory BSA (a few KB) with 2-3 LZ4-frame-compressed entries, embed-name on, covering:
   - LZ4 frame decode (`lz4_flex::frame::FrameDecoder`)
   - 24-byte folder record layout
   - u64 file offsets
   - embed-name prefix skip
   - per-file compression toggle XOR
2. **Sweetroll roundtrip test** — gated behind a `cfg(feature = "skyrim-data")` flag; reads `Skyrim - Meshes0.bsa` from a fixed Steam path and asserts a sample NIF extracts to a known SHA-256.

## Related

- #569 (open): SK-D2-01 — full-archive extraction sweep.
- This issue is the unit-level complement to that integration sweep.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: After (1) lands, audit if BA2 v3 LZ4 has the same coverage gap.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: This entire issue is the test fixture.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
