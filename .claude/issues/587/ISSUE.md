# #587: FO4-DIM2-05: Zero integration tests for BA2 reader

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/587
**Labels**: import-pipeline, medium, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 2)
**Severity**: MEDIUM
**Location**: `crates/bsa/` — no `tests/` directory; only 11 synthetic `#[cfg(test)] mod tests` in `ba2.rs:611`

## Description

`crates/bsa/` ships 11 synthetic unit tests (path normalization, `/dev/null` rejection, `linear_size_for` at three fixed sizes, DDS header layout, zlib round-trip, LZ4 round-trip, LZ4 corrupt-data). **Zero** tests touch a real FO4 BA2. Real-data exercise happens in:
- `crates/nif/tests/common/mod.rs` — NIF parsing uses BA2 via GNRL, but never asserts extracted-byte correctness.
- `crates/bgsm/tests/parse_all.rs` — gated on `BYROREDUX_FO4_DATA`, exercises Materials BA2 extraction but only asserts BGSM parse success.
- `crates/bsa/examples/ba2_debug.rs` — not a test.

Uncompressed GNRL (`packed_size == 0`) branch has **no vanilla coverage** — that branch has zero archive in the wild that exercises it.

Session-7's 458,617-file brute-force sweep was external (not committed as a CI guard).

## Evidence

`find crates/bsa -name '*.rs'` returns only `src/ba2.rs`, `src/lib.rs`, `src/archive.rs`, and four `examples/*.rs` — no tests directory.

## Impact

- No regression guard for v1/v7/v8 version dispatch.
- No byte-equality check for GNRL extractions (uncompressed + zlib-compressed NIF).
- No byte-equality check for DX10 extractions (cubemap, BC7, BC5 normal map).
- `packed_size == 0` uncompressed branch has zero coverage anywhere in the crate.

## Suggested Fix

Add `crates/bsa/tests/ba2_real.rs` gated on `BYROREDUX_FO4_DATA`:
1. Open `Fallout4 - Meshes.ba2` (v8 GNRL), extract `meshes/armor/poweramor/t60/t60body.nif`, assert first 4 bytes = `"Game"` (NIF magic).
2. Open `Fallout4 - Textures1.ba2` (v7 DX10), extract a known cubemap, assert DDS magic + miscFlag set.
3. Synthesize a 1-file uncompressed-GNRL BA2 (`packed_size = 0`) to lock that branch.
4. Commit session-7 brute-force sweep over `Fallout4 - Meshes.ba2` as a CI guard (assert zero errors across all 34,995 NIFs).

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: `crates/bsa/tests/bsa_real.rs` analogue for BSA v103/v104/v105 readers
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: This *is* the test work — success criterion is 4 new gated integration tests.

## Related

- Carry-forward of AUDIT_FO4_2026-04-17 L4 / L5 (unfiled previously).
