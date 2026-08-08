# SF-D3-01: index_chunks pre-reserves from unvalidated on-disk u32 chunk count, aborts process on corrupt CDB

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2614
**Finding ID**: SF-D3-01

**Severity**: HIGH
**Dimension**: 3 (CDB Material Database)
**Location**: `crates/sfmaterial/src/reader.rs:172-179` (`index_chunks`)
**Status**: NEW

## Description
`index_chunks` pre-reserves a `VecDeque` sized directly from an unvalidated
on-disk `u32` chunk count: `chunk_count = (chunk_count_incl_beth - 1) as
usize; VecDeque::with_capacity(chunk_count)`. A `0xFFFF_FFFF` read requests
~103 GB *before* the per-chunk `ChunkOverflow` guard a few lines below ever
runs. `with_capacity` on a request this large panics with "capacity
overflow" or calls `handle_alloc_error` → `abort()` — not catchable by the
caller.

## Evidence
```rust
// crates/sfmaterial/src/reader.rs:172-179
let chunk_count_incl_beth = self.read_u32()?;
if chunk_count_incl_beth < 1 {
    return Err(Error::EmptyChunkList);
}
let chunk_count = (chunk_count_incl_beth - 1) as usize;
let mut chunks = VecDeque::with_capacity(chunk_count);  // <-- unvalidated capacity
```
**On the live path today**: `register_starfield_cdb` calls exactly
`parse_header()` + `index_chunks()` at every cell load. Any file at
`materials\**\materialsbeta.cdb` inside a loaded BA2 starting with `BETH`
(mod-shipped, partially-downloaded, or bit-rotted) kills the engine at
cell-load with no log line, defeating the "malformed payload is warned and
dropped" contract the surrounding code documents and
`discovered_cdbs_accumulate_in_load_order` tests for — that test only
exercises `b"not a cdb"`, rejected earlier by `peek_magic`, so the reserve
path is untested. Gibbed's reference (`ComponentDatabaseFile.cs`) has no
equivalent exposure — a `Queue<Chunk>` with no pre-reserve fails on the
first stream read past EOF instead of pre-allocating.

## Impact
Corrupt/truncated CDB → process abort instead of `Err`, on the live
cell-load path. This is one half of "the CDB allocation-safety pair" —
SF-D3-02 is the latent sibling, currently unreachable but activating the
moment #2359/#1289 Phase 2 lands.

## Suggested Fix
`chunks.reserve(chunk_count.min(self.bytes.len() / 8))` (each chunk costs
≥8 bytes of header) or drop `with_capacity` entirely, matching the
reference. The existing EOF/`ChunkOverflow` checks then produce a proper
`Err` instead of an abort.

## Related
SF-D3-02 (same class, latent — fix in the same patch, cheaper together).

## Completeness Checks
- [ ] **TESTS**: Add a fuzzed-`chunk_count` (e.g. `0xFFFF_FFFF`) test asserting `Err`, not panic/abort
