# #7144: PAR-D6-2026-11-20-04: A promotion read is not pinned to the winner the floor read came from, and the streamer's provider never re-validates its archives

**Labels**: low,renderer,memory,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7144

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D6-04 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: (a) a BA2 DX10 record in the winning archive whose last chunk is corrupt but whose other chunks inflate (or the reverse); (b) an archive replaced on disk after streaming was first enabled
- **Location**: `byroredux/src/asset_provider/texture.rs:185-208` (`extract_for_registry`, `extract_texture_from`), `byroredux/src/asset_provider/archive.rs:500-525`, `byroredux/src/app_frame.rs:85-88`, `byroredux/src/texture_streamer.rs:131-141`
- **Status**: NEW
- **Description**: Each read is an independent `find_map` over `texture_archives.iter().rev()`, and `or_warn` turns any non-`NotFound` error into `None`, so a failed read in the winner falls through to the next archive. The floor read needs only the last chunk and the promotion read needs the chunks from the target level, so they can fail independently: the floor is served from winner W and the promotion from a lower-precedence L (or the reverse), giving one resident texture whose low mips come from one file and high mips from another (a mod override against vanilla). Separately, the streamer's `TextureProvider` is built once, on the first frame streaming is on (`streamer.is_none()`), from `Archive` handles taken then. Cell-load providers are rebuilt per transition and so re-check `ArchiveStamp` through `open_shared`; the streamer's handles do not. After an archive is replaced mid-session, floors come from the new file and promotions from the old fd and table, which is the #5663 same-length-rewrite hazard on the one holder that never re-opens.
- **Evidence**: `extract_texture_from` and `extract_for_registry` both end in `.or_else(|| self.extract(path))`; `or_warn` warns once per path per archive and then stays silent; no code compares the archive that served the floor with the one that serves the promotion. In the healthy case the order is identical, so this needs corruption or an on-disk change.
- **Impact**: visual (mixed-source mips) only on a damaged or replaced archive; bounds checks still fail rather than return garbage for a shorter replacement, but a same-length replacement is served from stale offsets.
- **Related**: #5663, #4658
- **Suggested Fix**: record which archive index served the floor in the registry's promotion request and have the promotion read only that archive (a failure then leaves the floor, with one warning); build the streamer's provider through the same `open_shared` revalidation a cell transition gets, or drop and rebuild it when a cell-load provider reports a non-reused archive.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
