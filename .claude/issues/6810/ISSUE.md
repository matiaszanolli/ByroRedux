# CONC-2026-11-08-7.1: TextureStreamer's pool job has no catch_unwind, so a panicking archive read aborts the process (sibling of #6233)

_Immutable snapshot of the issue as filed (GitHub is authoritative for state). Source: docs/audits/AUDIT_CONCURRENCY_2026-11-08.md. Labels: medium,concurrency,renderer,bug_

Found by `/audit-concurrency` 2026-11-08 (`docs/audits/AUDIT_CONCURRENCY_2026-11-08.md`, HEAD 0419437ab).

- **Severity**: MEDIUM — the rating rests on the #6233 precedent; no concrete reachable panic was demonstrated.
- **Dimension**: Worker Threads
- **Location**: `byroredux/src/texture_streamer.rs` — pool built in `TextureStreamer::new` with no `panic_handler`; the `self.pool.spawn(move || { provider.extract_texture_from(...); done.send(...) })` job in `step`

## Description
The M39 `TextureStreamer` (landed 2026-11-03, after the previous audit) runs archive re-reads on its own 2-thread rayon pool. `rayon::ThreadPool::spawn` hands a panicking job to the pool's panic handler; with none installed rayon-core aborts the process, and the workspace keeps `panic = "unwind"`. Every other archive-reading worker wraps its read: stream parse (`extract_guarded`), texture prefetch, terrain height (`spawn_guarded`, #6233). This one calls `TextureProvider::extract_texture_from` (BA2 DX10 mip re-read with zlib/LZ4 inflate of untrusted chunks, plus the plain `extract` fallback) with no guard.

## Evidence
`grep -n 'catch_unwind\|panic_handler' byroredux/src/texture_streamer.rs` returns nothing; no `panic_handler` anywhere in `byroredux/src` or `crates/*/src`. No test exists for a panicking read. The DX10 path returns `io::Error` and its slicing is bounds-safe as far as read, so reachability is unproven.

## Trigger Conditions
`render.texture_streaming` on; a streamed texture wanted at a finer level than resident; a BA2/BSA/loose entry whose re-read panics in a decoder (a modded or corrupt archive is enough). Fires on a pool thread mid-frame.

## Impact
Process abort with no unwind, `Drop` or save-ring flush. Secondary: without a guard the job never reaches `done.send`, so `in_flight` would keep the handle forever if a panic handler were later added.

## Suggested Fix
Wrap the read in `std::panic::catch_unwind(AssertUnwindSafe(..))`, map a panic to `bytes: None`, and always send the `FinishedRead` so `step` clears `in_flight` and logs the existing "found no archive entry" warning. Add a test next to `a_panicking_height_job_is_reported_failed`. (Alternative: a pool `panic_handler` that sends the `None` read; the per-job guard matches the other three workers.)

## Completeness Checks
- [ ] **SIBLING**: every rayon `spawn` that reads archives or untrusted bytes has a guard (stream parse, prefetch, terrain height already do)
- [ ] **TESTS**: a panicking job still delivers a `FinishedRead` with `bytes: None` and clears `in_flight`
