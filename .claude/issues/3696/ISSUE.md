# #3696 — ECS-D1-02: the recursive-read fast path skips record_and_check, so a re-entrant read can silently close an ABBA cycle

**Severity**: LOW · **Dimension**: Lock Ordering & Deadlock
**Location**: `crates/core/src/ecs/lock_tracker.rs` (`track_read`, the `recursive_read` branch)

## Fix

Moved the `#[cfg(debug_assertions)]` `global_order::record_and_check` block
above the `if recursive_read { ... return; }` early return, per the
issue's own suggested fix. A recursive read adds no new *outgoing* edge
(same type, same thread — the module doc's existing justification stays
correct for that half), but it's still an *incoming* observation: if this
thread already holds some other type `H` and a prior acquisition recorded
`T -> H`, re-reading `T` while `H` is held is exactly the "acquire T while
H held" pattern that closes the `H -> T -> H` cycle — and pre-fix, the
early return skipped the reachability probe (#2675) entirely for that
case.

`type_id` needed explicit exclusion from `held_others` for this to work:
on the non-recursive path it's naturally absent (the row hasn't been
inserted into `LOCKS` yet), but a recursive read's row **is** already in
the map — without filtering it out, every recursive read would present a
trivial self-loop (`T` "held while acquiring" `T`) and panic
unconditionally, not just on a real cycle. Added `.filter(|(id, _)| **id
!= type_id)` to the `held_others` construction.

Preserved the #2384 property the original comment calls out: the check
still runs before any mutation (both the recursive `read_count` bump and
the fresh-acquisition insert), so a panic here leaves `LOCKS` exactly as
it was before the call — no orphaned half-acquired state either way.

## SIBLING (issue's own checklist item — "`track_write`'s equivalent path checked for the same gap")

`track_write` has no equivalent gap: it has no recursive/re-entrant
branch at all — a same-type write-after-write or write-after-read always
panics in the checks above `record_and_check`, so by the time execution
reaches that block, any existing entry for `type_id` (if the checks above
didn't already panic) is guaranteed absent or already invalid. There's no
early-return path in `track_write` that skips the check the way
`track_read`'s recursive branch did.

## LOCK_ORDER (issue's own checklist item)

No `RwLock` scope changed — this is a reorder of two existing operations
(the check, and the recursive branch's mutation) within `track_read`
itself, not a change to what's held or for how long.

## TESTS (issue's own checklist item — "a re-entrant read that closes a B->A edge must panic under BYRO_LOCK_ORDER_CHECK")

Extended the existing `global_graph_detector_end_to_end` test (which
already force-enables the detector for its own duration via
`global_order::set_enabled_for_tests(true)`, so no new
`BYRO_LOCK_ORDER_CHECK` env-var plumbing was needed) with a new scenario,
following its established `Restore`-guard / `catch_unwind` pattern
exactly: hold `Recur1`, acquire `Recur2` (records `Recur1 -> Recur2`),
then re-read `Recur1` while `Recur2` is still held — this must panic,
since it's the literal `B -> A` closing case the issue describes.

Verified the guard actually catches the regression (this session's
established quality bar): temporarily moved the check back to its
pre-fix position (after the recursive-read return), reran — the new
scenario failed with exactly the expected assertion message, then
restored the fix and confirmed a clean pass again.

## Verification

- `cargo check -p byroredux-core --tests`: clean.
- `cargo test -q -p byroredux-core --lib`: 727 tests passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7090 passing, 0
  failing** (unchanged — this fix extended an existing test's body rather
  than adding a new `#[test]` function).
