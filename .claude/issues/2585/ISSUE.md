# SK-D5-LZ4-LOW-02: Post-decompression size-mismatch check is warn, not surfaced to any caller-visible metric

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2585
**Finding ID**: SK-D5-LZ4-LOW-02

**Severity**: LOW
**Dimension**: BSA v105 (LZ4)
**Location**: `crates/bsa/src/archive/extract.rs:154-164`
**Status**: NEW (observation — not exercised on real data; the full sweep produced zero such warnings across all 65,637 files)

## Description
A declared/actual size mismatch after LZ4 frame decode logs `log::warn!` but returns `Ok` regardless — a deliberate, documented design choice (mirrors the BA2 zlib path). Recorded only because no `nif_stats`-style counter exists for the BSA layer either, so a future audit doesn't have to re-derive that this is intentional.

## Impact
None currently observed. Would only matter on a malformed/modded archive, surfacing downstream as a confusing NIF/DDS parse error rather than a clear BSA-layer diagnostic.

## Suggested Fix
None required now; pipe into a future parse-rate-style gate if one is added for the BSA extraction layer.

## Completeness Checks
- [ ] **TESTS**: N/A — deliberate design choice, no action required
