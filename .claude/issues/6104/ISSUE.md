# CONC-SD-02: no test makes any `ParseInputBudget` limit bind

Labels: low,concurrency,test-gap,bug

**Severity**: LOW (coverage gap; the code reads correct) · **Dimension**: Worker Threads & Thread-Safety Bounds (Dim 7)
**Location**: `byroredux/src/streaming/pre_parse.rs:439-482` (`ParseInputBudget::acquire`, `ParseInputPermit::drop`) and `:511-572` (`parse_nif_pipeline`'s parallel branch); the fixture `parse_model_keys` at `:576-598` and `streaming_tests.rs:746` (`pre_parse_parallel_branch_uses_the_dedicated_pool`)

## Description
The admission control is a `Mutex` + `Condvar` pair on the worker thread, waited on while pool tasks release permits. The only test that crosses `PRE_PARSE_RAYON_MIN` builds its inputs from an empty `TextureProvider`, so every `declared_bytes` is `0` and `max_tasks` is `pool_threads * 2` with no bytes in play. The byte cap (`STREAM_PARSE_INPUT_BYTES`), the `tasks >= max_tasks` wait, the "an oversized input is admitted alone" rule and the permit release on a panicking task are all untested. This is a deadlock-capable primitive whose failure shape (a worker parked forever, so streaming silently stops) is invisible to every other guard.

## Evidence
`grep -rn 'ParseInputBudget\|STREAM_PARSE_INPUT_BYTES\|STREAM_PARSE_MAX_TASKS\|ParseInputPermit' byroredux/src` returns only `pre_parse.rs`. No open issue (searched "budget", "admission", "pre_parse", "condvar"); #5715 is the unrelated stale-salvage hitch. The code itself was read as correct: single waiter (the coordinator thread, not a pool thread), the predicate admits when `tasks == 0`, the permit is moved into the task closure and drops on unwind, poisoned locks recover.

## Impact
None today. A later edit to the wait predicate (it has three clauses) or to the permit lifetime (permit moved out of the task closure) would not fail a test.

## Suggested Fix
A unit test over `ParseInputBudget` alone (no archives): (1) `max_tasks` permits taken, a thread blocks on the next and wakes when one drops; (2) an oversized request is admitted only when `tasks == 0` and blocks the next; (3) a permit moved into a panicking `catch_unwind` closure is released. Pure CPU, milliseconds.

Source: docs/audits/AUDIT_CONCURRENCY_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
