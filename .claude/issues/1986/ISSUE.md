**Source:** FO4 compatibility audit — Dimension 1 (M49 Precombined Geometry / CSG), `docs/audits/AUDIT_FO4_2026-07-13.md`
**Severity:** LOW · **Status when filed:** NEW, CONFIRMED against current code

## Description
The PSG addressing model in the CSG reader assumes chunk `i` occupies uncompressed bytes `[i*65536, (i+1)*65536)` — `read_psg` derives `idx = pos / CSG_CHUNK_SIZE`, `local = pos % CSG_CHUNK_SIZE` directly from that invariant, and the spec states every chunk inflates to exactly 65,536 bytes except the final chunk. `chunk_bytes` rejects only *over-size* inflated chunks; it never asserts a **non-last** chunk equals exactly `CSG_CHUNK_SIZE`. A corrupt or truncated CSG whose interior chunk inflates short would make every subsequent PSG offset point into the wrong chunk-local position.

## Evidence
- `crates/bsa/src/csg.rs:238`: the guard is one-sided — `if raw.len() > CSG_CHUNK_SIZE { … Err }`. No lower/exact bound for interior chunks.
- `crates/bsa/src/csg.rs:186-187`: `read_psg` trusts `CSG_CHUNK_SIZE`-granular addressing (`idx = pos / CSG_CHUNK_SIZE`, `local = pos % CSG_CHUNK_SIZE`) with no cross-check against the actual inflated length of interior chunks.
- The comment at `csg.rs:166`/`:220` documents the "all chunks but the last are exactly `CSG_CHUNK_SIZE`" invariant, but it is asserted nowhere.

## Impact
Malformed-input robustness only. On all validated real data every non-final chunk is exactly 65,536 (zlib 64 KiB blocks), so this never fires on vanilla/DLC content. When it *would* fire, the mis-addressed bytes decode as garbage `u16` indices and are **rejected downstream** by the #1533 decode-time `index >= num_verts` guard (object skipped → per-REFR fallback), so it fails closed rather than rendering corruption. Hence LOW, not MEDIUM.

## Suggested Fix
In `chunk_bytes`, when `idx < self.chunks.len() - 1`, assert `raw.len() == CSG_CHUNK_SIZE` and return `io::ErrorKind::InvalidData` otherwise — turns a silent wrong-data path into an explicit reject, matching the existing over-size guard.

## Related
#1533 (decode-time index guard, the mitigating backstop); `docs/engine/fo4-csg-format.md` chunk-inflate-size invariant.

## Completeness Checks
- [ ] **SIBLING**: check `total_uncompressed_len` math (`csg.rs:175`) stays consistent if the exact-size assert is added
- [ ] **TESTS**: a unit test feeds a short interior chunk and asserts `chunk_bytes`/`read_psg` returns `InvalidData` instead of silently mis-addressing
