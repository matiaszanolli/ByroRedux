# REG-2026-08-20-D2-01: #3089's two guards pin the pool constructor and rayon's own install - never the call site

**Issue**: #3211 — https://github.com/matiaszanolli/ByroRedux/issues/3211
**Severity**: MEDIUM
**Labels**: `medium,sync,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-20.md` § REG-2026-08-20-D2-01 (Dimension 2 — Guard existence & liveness).

**Severity**: MEDIUM
**Location**: `byroredux/src/streaming_tests.rs` — `stream_parse_pool_leaves_headroom_for_the_frame_pool` (`:578`), `stream_parse_pool_runs_tasks_on_its_own_dedicated_threads` (`:599`). Fix site: `byroredux/src/streaming.rs:1318`.

## Description

**#3089** (MEDIUM, `sync`/`performance`, closed 2026-08-19 by `060718cb`) is about **contention**: the cell-stream worker's Phase 2 fan-out was dispatching into rayon's *global* pool, which the ECS scheduler's `Stage::Update` parallel batch also uses.

The fix is **one line**, at `streaming.rs:1318`:

```rust
stream_pool.install(|| extracted.into_par_iter().map(parse_one_nif).collect())
```

**Neither guard touches it.**

- `stream_parse_pool_leaves_headroom_for_the_frame_pool` constructs `build_stream_parse_pool()` directly and asserts its thread count.
- `stream_parse_pool_runs_tasks_on_its_own_dedicated_threads` constructs a pool directly and asserts that `pool.install(…)` runs on a `byro-stream-parse-*` thread — **which is a property of `rayon::ThreadPool`, not of this repo's code.**

**Delete the fix and both tests stay green.** Reverting `streaming.rs:1318` to the pre-fix `extracted.into_par_iter().map(parse_one_nif).collect()` reinstates the exact contention #3089 describes, and nothing fails.

## Evidence (verified at HEAD `bb0b92f2`)

```
$ grep -an "pre_parse_cell\|stream_pool" byroredux/src/streaming_tests.rs
10:    build_stream_parse_pool, classify_payload, …
471,478,495,545:  pre_parse_cell_panic_safe   ← a #854 guard, unrelated
578:fn stream_parse_pool_leaves_headroom_for_the_frame_pool()
599:fn stream_parse_pool_runs_tasks_on_its_own_dedicated_threads()

$ grep -rn "stream_pool" --include='*.rs' byroredux crates | grep -i test
(no output)  ← no test in the workspace names stream_pool or reaches Phase 2
```

Production `stream_pool` sites: `streaming.rs:1051` (construction), `:1071` / `:1187` (threading it through), `:1318` (**the only `install`** — the fix).

Both tests do `let pool = build_stream_parse_pool();` and never touch `pre_parse_cell`.

## Impact

The regression this restores is **invisible to `cargo test` by construction** — a thread-pool routing choice with no functional output. The guard is therefore the *only* possible detector, and it detects nothing.

#3089's own framing is the failure mode that silently returns:

> *"defeating the whole point of running cell parsing on its own thread in the first place"*

The delta's streaming surface is one of its two hottest (`streaming.rs` is heavily edited across the 335-commit window), which raises the odds of a refactor quietly dropping the `install` wrapper.

## Suggested Fix

Give `pre_parse_cell` an **observable**:

1. Have `parse_one_nif` (or a thin wrapper) record `std::thread::current().name()` into the existing `StreamingWorkerTimings`.
2. Assert in `streaming_tests.rs` that a fan-out above `PRE_PARSE_RAYON_MIN` reports **only** `byro-stream-parse-*` names.

That reaches the actual `install` at `:1318` and fails on its removal. The two existing tests are worth keeping — they pin the pool's *shape* — but they cannot substitute for a guard on its *use*.

## Related

- **#3089** (`CONC-2026-08-16-01`) — the fix this fails to guard; `060718cb`
- **#877** / **#1262** — the two-phase pre-parse split the pool sits inside
- **#862** — the cache snapshot in the same function
- The `esm/records/tests.rs` grep-blindness finding filed from this same report — the sweep's companion "the guard cannot do its job" shape

## Completeness Checks
- [ ] **REACHES-THE-FIX**: Reverting `streaming.rs:1318` to a bare `into_par_iter()` makes the new guard **fail** — verify by actually reverting it locally before committing
- [ ] **LOCK_ORDER**: Threading a thread-name observable through `StreamingWorkerTimings` does not add a lock acquisition inside the rayon closure
- [ ] **SIBLING**: The other `stream_pool` threading sites (`:1071`, `:1187`) are checked for the same "pinned by construction, not by use" gap
- [ ] **TESTS**: The new assertion is gated so it is meaningful above `PRE_PARSE_RAYON_MIN` and does not flake on low-core CI machines
