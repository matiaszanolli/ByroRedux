# #5908: PAR-D5-2026-10-15-04: #5466's replacement text says the CDB index is "built lazily on the first lookup" and is "not built at discovery", but #5291 prefetches it at discovery

- **Labels**: low,documentation,doc-rot,nifal,game:starfield
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**:
  - `byroredux/src/asset_provider/material/cdb.rs:324`. This is the INFO log printed for every discovered CDB: "(the material index is built lazily on the first lookup)".
  - `cdb.rs:9`: module doc "the lazily built `MaterialIndex` (see `provider.rs`)". The index is built in `cdb.rs`.
  - `provider.rs:213-216`: "built lazily on the first `.mat` lookup … not at discovery — a session that loads no Starfield content never pays the ~2 s / ~470 MB build".
  - Contradicted by `cdb.rs:288-291`, where `discover_starfield_cdbs` calls `prefetch_cdb_material_index` for every valid CDB, and by `cdb.rs:86-90` ("~247 MB … the ~470 MB 224a19372 recorded is stale").
- **Status**: NEW. 4b289dce1 (#5466, closed) rewrote this text after dd759b762 (#5291) had already made the build eager. The new log line is stale on arrival, which is the same "INFO log printed every session" defect #5466 was filed for.
- **Trigger Input**: any session that discovers a valid `materialsbeta.cdb`.
- **Description**: the operator-facing log and two doc sites describe the pre-#5291 lazy model. A Starfield session pays the build at discovery on a worker thread whether or not any `.mat` merge happens. The peak figure quoted in `provider.rs` is the stale ~470 MB rather than the re-measured ~247 MB.
- **Evidence**: `cdb.rs:203-224` spawns `sf-cdb-index` from discovery, and `cdb.rs:131-136` itself says the build is paid "by the background prefetch … when it gets there first".
- **Impact**: doc and log accuracy only. Someone tracing a memory or latency question is told the build cannot happen in a session without `.mat` lookups.
- **Related**: #5466, #5291, #5666 (the prefetch ordering residual, still open).
- **Suggested Fix**: say "built in the background at discovery (#5291); a lookup that arrives first builds it (or parks on the build)". Point the module doc at `cdb.rs`, and replace ~470 MB with the ~247 MB peak / ~115 MB resident figures.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
