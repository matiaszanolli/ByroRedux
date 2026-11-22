# #7169 — PAR-D6-2026-11-21b-01: the first cell loaded after an archive replacement streams no sharper mips: its floors from the new file are retired with the old ones

Labels: medium,import-pipeline,renderer,bug

- **Severity**: MEDIUM. A regression #7167 introduced: textures from the right file stay at floor mips, with no error.
- **Dimension**: I/O & Paths
- **Location**:
  - `crates/renderer/src/texture_registry/upload.rs`: `enqueue` stamps `epoch: self.stream_epoch`; `retire_stream_sources`; the `source.epoch == self.stream_epoch` gate in `apply_wanted`.
  - `byroredux/src/app_frame.rs`: the retire call after `refresh_provider`.
  - `byroredux/src/app_step.rs`: the cell-load `build_texture_provider` calls.
- **Status**: NEW (regression of #7167, closed)
- **Trigger Input**: a texture archive replaced on disk while the engine runs, then a cell load.
- **Description**: the replacement counter (`ARCHIVE_REPLACEMENTS`) moves only inside `Archive::open_shared`, which a cell load calls through `build_texture_provider` (`app_step.rs`) before it resolves and floors that cell's textures. Those floors are read from the new file but queued under the current `stream_epoch` N. On the next frame-loop pass, `refresh_provider` sees the moved counter, and `retire_stream_sources` advances the epoch to N + 1. Every floor queued since the counter moved therefore carries a stale epoch: it is never promoted and stays blurry until it is released. Before #7167 these textures promoted correctly; the commit closed the mixed-mip case for floors from the old file at the cost of floors from the new file.
- **Evidence**: the call order was traced by the reviewer and re-read by the coordinator (cell-load provider builds at `app_step.rs:1484` and the sites near it). The epoch is stamped at enqueue and compared only at promotion. The #7167 test inserts `StreamSource { epoch: reg.stream_epoch, .. }` by hand, so it never queues a floor between the counter moving and the retire. Not run end to end.
- **Impact**: after a mid-session archive swap (a mod manager redeploying, a user editing a BA2), the next cell's textures render at low resolution for as long as they stay loaded.
- **Related**: #7167, #7144
- **Suggested Fix**: stamp each queued floor with the archive generation (`archive_replacements()` read when the provider that reads it was built, passed through the enqueue). Refuse promotion only when a source's generation differs from the streamer provider's. Add a test that queues a floor after the counter moves and before the retire, and expects it to promote.
- **Also (INFO)**: `stats.deficits` counts retired sources wanted finer, so `deficits > 0, settled = 1` can persist after a replacement; `promotions_idle` does not block settling.

### LOW

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
