# #7167 — PAR-D6-2026-11-21-02: after an archive replacement the streamer keeps promoting textures that were floored from the old file

Labels: low,import-pipeline,bug

- **Severity**: LOW (silently mixed mips, but the trigger needs an archive replaced on disk while the engine runs)
- **Dimension**: I/O & Paths
- **Location**: `byroredux/src/texture_streamer.rs` `refresh_provider`; `byroredux/src/asset_provider/archive.rs` `open_shared` (the `ARCHIVE_REPLACEMENTS` bump)
- **Status**: NEW (residual of #7144, closed)
- **Description**: `refresh_provider` swaps the provider and nothing else. Resident textures floored from the old file are never re-validated, so their next promotion reads the new file, which is the two-file mixed-mip case #7144 set out to close. The counter also stays quiet in two cases: when the 1024-entry registry evicts an entry and the reopen lands in an empty slot, and when `ArchiveStamp::of` fails, which bypasses the registry. The read-before-build ordering is correct.
- **Suggested Fix**: record the generation on each stream source at floor time, and refuse (or re-floor) promotions for sources floored under an older generation. Bump the counter on an eviction-then-reopen as well, or compare each archive's own `ArchiveStamp`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
