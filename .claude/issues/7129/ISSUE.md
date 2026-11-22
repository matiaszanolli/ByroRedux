# #7129: PAR-D2-2026-11-20-05: texture-streamer promotion failure has no retry backoff, logs a misleading per-frame warning, and a short read is a debug line

**Labels**: low,renderer,memory,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7129

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D2-05 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW (streaming is off by default until #6631)
- **Dimension**: Error Semantics
- **Location**: byroredux/src/texture_streamer.rs:171-198 (`step`), crates/renderer/src/texture_registry/upload.rs:506 (`apply_wanted`) / :1568 (`note_declined_changes`)
- **Status**: NEW
- **Description**: A pool read that returns `bytes: None` (an `Err` from `extract_dx10_mips`, a panic caught by #6810, or a true miss) hits `log::warn!("texture promotion read for handle N found no archive entry")` — wrong text for an extract error/panic, and `or_warn` has already warned once per entry. Nothing marks the handle failed: `retry_after_frame` is only set by `note_declined_changes` (GPU-side declines), so the next frame's `apply_wanted` re-requests the same promotion, `take_promotion_requests` re-spawns the read (a full chunk read + inflate on the pool) and `step` warns again — every frame while the texture stays wanted. When bytes arrive but `queue_promotion_bytes` returns false (payload shorter than the request: `payload_first_level` exact-length rule), only a `debug!` "was not queued (released or already there)" is logged, with the same re-request loop. The #6810 test documents "the handle can be requested again" but not at frame rate.
- **Evidence**: by reading (no GPU probe): `source.retry_after_frame` has exactly two writers (`note_declined_changes`, test fixtures); `apply_wanted` gates candidates only on `queued.contains_key` and `frame >= retry_after_frame`.
- **Impact**: a BA2 whose DX10 entry fails (short non-final chunk, corrupt deflate; vanilla measured zero, mods not) floods the log and burns pool CPU per frame once streaming is on by default.
- **Trigger Input**: streamed BA2 DX10 texture whose non-final chunk decodes short or whose zlib stream is corrupt.
- **Related**: #6810, #6631, #6737 (the floor-first sibling, closed)
- **Suggested Fix**: on a failed/short read call a registry method that sets `declined_target`/`retry_after_frame` (or a permanent `unreadable` mark), and word the warning by cause.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
