# #7131: PAR-D2-2026-11-20-07: BA2 chunk-read and inflate EOF errors carry no entry or chunk context

**Labels**: low,import-pipeline,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7131

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D2-07 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: crates/bsa/src/ba2.rs:1038 (`read_chunk_payload`), `decompress_chunk` zlib arm
- **Status**: NEW (every other BA2 error names the entry, e.g. the #4662 short-chunk message and the zlib over-run message)
- **Description**: A record whose chunk offset/size runs past the file, or a truncated deflate stream, surfaces as the bare `UnexpectedEof: failed to fill whole buffer` / `incomplete deflate stream` — no path, chunk index, offset or archive. Open does not bound record offsets by file length (only the name table, #4670), so a truncated BA2 fails per entry at extract time with these. Consumers going through `or_warn` add the entry path, but not which chunk; direct callers (tools, `extract_dx10_mips` used by the streamer) get nothing.
- **Evidence**: probe `fuzzba2` (8,000 mutated/truncated FO4 DX10 BA2s) collected distinct `extract` / `extract_dx10_mips` errors: `UnexpectedEof: failed to fill whole buffer` and `UnexpectedEof: incomplete deflate stream`, versus named messages for every other failure.
- **Trigger Input**: truncated BA2 (record table intact, data cut) — the #4670 real case `cuwp - textures.ba2` shape.
- **Suggested Fix**: map `read_exact_at` / inflate EOF in `read_chunk_payload`/`decompress_chunk` to an error naming `path`, chunk index, `offset` and the file length.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
