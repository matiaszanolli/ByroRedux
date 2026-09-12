# SF-2026-09-11-D3-02: probe_header — the only CDB path production code runs — aborts the entire Starfield PBR gate on any unknown chunk FourCC it never interprets

**Issue**: #4273 — https://github.com/matiaszanolli/ByroRedux/issues/4273
**Labels**: medium,import-pipeline,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 3 — CDB Material Database Correctness
**Location**: `crates/sfmaterial/src/reader.rs:142-149 (probe_header), :212-252 (index_chunks); crates/sfmaterial/src/chunk.rs:32-45 (ChunkType::from_raw)`
**Status**: NEW

## Description
`probe_header` is the only CDB code path production actually runs (a cheap presence-check that never touches the instance tree). It calls `index_chunks`, which calls `ChunkType::from_raw` on every chunk's FourCC — and `from_raw` returns `Err(Error::UnknownChunkType)` on any FourCC not in its fixed 10-entry match, immediately aborting the whole probe. `probe_header` never actually interprets a chunk's contents (it only counts them), so it does not need to recognize every FourCC to do its job, but the shared `index_chunks`/`from_raw` machinery forces it to.

## Evidence
`crates/sfmaterial/src/chunk.rs:32-45`: `ChunkType::from_raw` matches 10 known FourCCs and returns `Err(Error::UnknownChunkType { raw, index })` for anything else; `crates/sfmaterial/src/reader.rs:142-149` (`probe_header`) calls `p.index_chunks()?`, propagating that error unconditionally.

## Impact
Any CDB file (a future format revision, a mod-authored CDB, or a corrupted one) containing even one chunk type not in the current 10-entry vocabulary makes `probe_header` fail entirely, aborting the Starfield PBR gate for that file even though the probe only needed the chunk count/presence, not semantic understanding of every chunk. Not reachable on vanilla retail Starfield content today (all vanilla CDBs use only recognized chunk types), but a real risk for the CDB Phase 2 (#3398) reader.

## Related
Adjacent to #3398 (CDB Phase 2, the tracker this hardens).

## Suggested Fix
Give `probe_header`'s path through `index_chunks` a tolerant mode that records an unrecognized FourCC as an opaque/unknown chunk (with its raw value and size) rather than erroring, since the probe only needs count and presence, not semantic dispatch.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
