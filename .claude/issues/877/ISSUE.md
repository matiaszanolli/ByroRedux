# Issue #877 (OPEN): NIF-PERF-13: pre_parse_cell runs extract_mesh inside rayon closure — BSA mutex serializes I/O across workers

URL: https://github.com/matiaszanolli/ByroRedux/issues/877

---

## Description

`byroredux/src/streaming.rs:364-368` calls `tex_provider.extract_mesh(&path)` inside the rayon `into_par_iter` worker closure. `BsaArchive` and `Ba2Archive` both wrap their `File` in `Mutex<File>` (`crates/bsa/src/archive.rs:119`, `crates/bsa/src/ba2.rs:78`).

With N rayon workers concurrently calling `extract_mesh`, all but one block on the file mutex during the actual `read_at`. Only the parse + import work parallelizes fully — and only after the mutex release.

This is the follow-up the 2026-05-04 audit (#830) flagged.

## Evidence

```rust
// streaming.rs:364-398 — inside par_iter().map() closure
let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    let Some(bytes) = tex_provider.extract_mesh(&path) else {       // ← BSA Mutex<File>
        log::debug!("[stream-worker] NIF not in BSA: '{}'", path);
        return None;
    };
    let scene = match byroredux_nif::parse_nif(&bytes) { ... };
    // ... parse + import work that DOES parallelize cleanly
}))
```

## Why it matters

For NIFs where `extract_mesh` is the dominant cost (small NIFs with tiny block counts, common in dense interior cells), workers spend most of their wall-clock time queued on the BSA mutex. Estimated ~10–20% additional speedup over the current 6–7× from #830.

## Proposed Fix

Two-phase pre-parse:

```rust
// Phase 1 — serial extract (one worker, no contention):
let extracted: Vec<(String, Option<Vec<u8>>)> = model_paths
    .into_iter()
    .map(|p| (p.clone(), tex_provider.extract_mesh(&p)))
    .collect();

// Phase 2 — parallel parse + import on the (path, bytes) pairs:
let results = extracted.into_par_iter().map(|(path, bytes)| {
    let bytes = match bytes { Some(b) => b, None => return (path, None) };
    // existing parse + import block …
}).collect();
```

Removes BSA mutex contention from rayon worker critical path; workers spend 100% of their wall-time on CPU-bound work.

## Cost Estimate

Wall-clock impact is the win (allocation impact is neutral — same total `Vec<u8>` bytes flow through). Needs a streaming-cell wall-clock benchmark to quantify (out-of-band measurement, not dhat).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other rayon `par_iter` sites for similar mutex contention (e.g. texture pre-load if/when parallelized)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A — no RwLocks crossed in the closure
- [ ] **FFI**: N/A
- [ ] **TESTS**: Cell-streaming integration test must not regress; add a wall-clock benchmark on a fixture FNV / SE exterior grid before/after

## dhat Gap

Not applicable — the cost is in wall-clock contention, not allocation count. Out-of-band benchmark is the right tool here, separate from the dhat-infra gap.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06.md` (NIF-PERF-13)
- Follow-up to: #830 (closed) — single-threaded → rayon-parallel pre-parse
- Related: #854 (panic guard inside the worker closure — must be preserved across the refactor)
