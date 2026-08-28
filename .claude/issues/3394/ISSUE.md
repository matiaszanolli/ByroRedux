# Issue #3394 — SF-2026-08-27-D1-03: decompress_chunk_lz4_undersized_hint_never_unwinds skips hint 0 on a false premise

Filed: 2026-08-27 by `/audit-publish` from `docs/audits/AUDIT_STARFIELD_2026-08-27.md`

Labels: `low,bug,test-gap,import-pipeline,game:starfield,legacy-compat`

> Immutable snapshot of the issue as filed (TD10-001 / #1156).
> GitHub is authoritative for current state: `gh issue view 3394 --json state`.

---

Found by `/audit-starfield` — [`docs/audits/AUDIT_STARFIELD_2026-08-27.md`](docs/audits/AUDIT_STARFIELD_2026-08-27.md), Dimension 1, delta review of `1b521305`.

- **Severity**: LOW
- **Location**: `crates/bsa/src/ba2.rs:1643-1667` (comment at `:1647`); premise contradicted by `crates/bsa/src/safety.rs:78-89`
- **Status**: NEW

## Description

The new fuzz-lite test `decompress_chunk_lz4_undersized_hint_never_unwinds` justifies starting its hint sweep at `1` with:

```rust
// 0 is rejected upstream by `checked_chunk_size_usize`; start at 1 and …
```

`checked_chunk_size_usize` only rejects values **above** `MAX_CHUNK_BYTES`; `0` passes straight through. `unpacked_size == 0` with a non-zero `packed_size` is therefore fully reachable from a malformed archive — and it is precisely the most-undersized hint possible, i.e. the one case the test was written to probe and the one it excludes.

## Evidence

```rust
// crates/bsa/src/safety.rs:78-89
pub fn checked_chunk_size_usize(size: usize, label: &str) -> io::Result<usize> {
    if size > MAX_CHUNK_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{label} size {size} exceeds safety cap {MAX_CHUNK_BYTES} \
                     — archive is corrupt or hostile"),
        ));
    }
    Ok(size)          // 0 → Ok(0)
}
```

Reachability: `read_dx10_records` calls `checked_chunk_size(unpacked_size, …)` (`ba2.rs:633`), which accepts `0` on the same rule; `extract_dx10` (`ba2.rs:861`) takes the decompress branch whenever `packed_size != 0`. So `decompress_chunk(packed, 0, Lz4Block)` is reachable.

The good news is that the behaviour is safe: `vec![0; 0]` + `SliceSink` returns `Err(OutputTooSmall)`. An independent scan of 19,656 vanilla v3 DX10 records during this audit found **0** chunks with `unpacked_size == 0`, so no vanilla archive exercises it — but the test exists for the hostile-input case, not the vanilla one.

## Impact

Documentation is factually wrong about a safety helper, and the boundary the test most wanted to cover is the one it silently omits. No runtime defect.

## Suggested Fix

Change the sweep to `[0usize, 1, 2, 8, 32, actual_payload.len()]` and correct the comment to say `0` is *accepted* by `checked_chunk_size_usize` and reaches the codec, where the safe decoder rejects it as `OutputTooSmall`.

## Related

`SF-2026-08-27-D1-01` (#3392) — same commit; that finding covers why "the safe decoder" is a feature-dependent property rather than a fixed one, which is worth resolving first since it changes what this test is actually asserting. #586 / #2356 (the `safety.rs` cap family).

## Completeness Checks
- [ ] **SIBLING**: check whether other callers document `checked_chunk_size*` as rejecting `0`
- [ ] **TESTS**: the extended sweep genuinely exercises hint `0` through `decompress_chunk`
