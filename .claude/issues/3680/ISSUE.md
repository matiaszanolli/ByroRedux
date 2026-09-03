# #3680 — PERF-D1-2026-08-30-04: the lock tracker materialises its held_others snapshot before the detector's own enabled check

**Severity**: LOW · **Dimension**: CPU Hot Paths
**Location**: `crates/core/src/ecs/lock_tracker.rs` (`track_read`, `track_write`)

## Fix

Implemented the issue's suggested fix exactly: added
`global_order::is_enabled()` (a thin wrapper over the existing `ENABLED`
atomic, `pub(super)` alongside the existing `set_enabled_for_tests`) and
hoisted it above the `held_others` borrow/filter/collect in both
`track_read` and `track_write` — the `#[cfg(debug_assertions)] { ... }`
block became `#[cfg(debug_assertions)] if global_order::is_enabled() {
... }`. `record_and_check`'s own internal `ENABLED.load()` check stays as
defense-in-depth (harmless — it's now unreachable from these two call
sites, but the function is `pub(super)` and could gain another caller),
and its `held_others.is_empty()` check is now genuinely the *second*
early-out inside the already-enabled path, not doing double duty as the
disabled-by-default fast path too.

Note: this issue explicitly named #3696 (ECS-D1-02, the recursive-read
correctness gap on the same lines) as something that "should land
together" with this fix since both touch the identical block — #3696 was
already fixed earlier in this session. Re-verified the two fixes compose
cleanly: `track_read`'s `#3696` filter (`.filter(|(id, _)| **id !=
type_id)`) sits inside the newly-hoisted `if global_order::is_enabled()`
guard from this fix, and the full lock-order test suite (including
#3696's own added recursive-cycle scenario) still passes unchanged.

## SIBLING (issue's own checklist item)

`record_and_check` has exactly two callers — `track_read` and
`track_write` — both fixed; no other site builds a `held_others`-shaped
snapshot before an enabled check.

## LOCK_ORDER (issue's own checklist item)

No `RwLock` scope changed. The fix only adds a cheap `is_enabled()`
condition *around* the existing borrow/collect/call sequence — the
sequence itself, and everything it does when actually entered, is
byte-for-byte unchanged.

## TESTS (issue's own checklist item)

Added `crates/core/tests/lock_tracker_allocation_bounds.rs` — its own
test binary (mirroring `crates/nif/tests/heap_allocation_bounds.rs`'s
isolation pattern) with a minimal counting `#[global_allocator]` wrapper
over `System` (no new `dhat` dependency or feature-gate needed, since a
differential "did this cost more allocation than that" comparison is all
the property requires). Measures the marginal allocation cost of
acquiring a `Transform` read lock with two other read locks already held
vs. the identical acquisition in isolation — under the default-disabled
detector, the two must be byte-for-byte equal.

Verified the guard actually catches the regression (this session's
established quality bar): reverted both `track_read`/`track_write` to
the unconditional-collect shape, reran — the test failed with exactly
the predicted `1 vs 0` allocation delta, then restored the fix and
confirmed a clean, stable pass (repeated 4×, no flakiness).

## Verification

- `cargo check -p byroredux-core --tests`: clean.
- `cargo test -q -p byroredux-core`: 727 lib tests + all integration
  suites passing, 0 failing (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7094 passing, 0
  failing**.

## Note on the issue's own "not measured" caveat

The issue explicitly stated "I did not measure the wall-clock cost and
it is unknown." This fix doesn't add a wall-clock benchmark either —
the allocation-count differential test is the more precise instrument
for this specific claim (ENABLED's doc promises a *fixed number of
operations*, not a *time budget*), and it now pins that number exactly:
zero marginal allocations for a nested acquisition versus an isolated
one, matching the "one relaxed load" the doc comment has claimed since
#823.
