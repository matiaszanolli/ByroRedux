# #3707 — ECS-D10-07: write_root_motion's zero-motion early return latches the previous tick's delta

**Severity**: LOW · **Dimension**: Animation Runtime
**Location**: `byroredux/src/systems/animation.rs` (`write_root_motion`)

## Fix

`write_root_motion` early-returned on `motion == Vec3::ZERO`, leaving
`RootMotionDelta` holding whatever the last non-zero tick wrote — since
the write is an assignment (`rm.0 = motion`), not an accumulation, a
genuinely stationary tick needs to overwrite the component with zero, not
skip it. The only drain, `cinematic_root_motion_system`, zeroes it only
for actors currently awaiting `ExitCartEnd`, so a stale delta from an
entity's previous motion window could survive indefinitely and surface as
a one-frame position pop the next time that window opened.

Removed the early return per the issue's own suggested fix — the function
now writes unconditionally (including `Vec3::ZERO`), keeping the existing
`query_mut`/`get_mut` guards so an entity without the component stays
untouched exactly as before.

## SIBLING (issue's own checklist item)

Checked both call sites (`byroredux/src/systems/animation.rs:758` — the
player path, `:997` — the stack path). Both simply pass a computed
`root_motion: Vec3` that already defaults to `Vec3::ZERO` when no accum
root is animated this tick; neither needed any change to stay correct
under the new unconditional-write contract.

## TESTS (issue's own checklist item — "a regression test pins that a zero-motion tick clears a previously written delta")

Added `root_motion_tests` (`byroredux/src/systems/animation.rs`), calling
`write_root_motion` directly against a minimal `World`:

- `zero_motion_tick_clears_a_previously_written_delta` — the literal
  regression test the issue asked for: seed `RootMotionDelta` with a
  non-zero value, call with `Vec3::ZERO`, assert it's cleared.
- `non_zero_motion_still_writes_through` — the existing behavior stays
  correct.
- `entity_without_component_is_left_alone` — confirms the storage-presence
  guard still no-ops cleanly for an entity that never had the component,
  which is what lets `write_root_motion` run unconditionally on every
  animated entity rather than only ones known in advance to carry root
  motion.

Verified the guard actually catches the regression (this session's
established quality bar): reintroduced the early return, reran — the
first test failed with exactly the described stale-value symptom
(`left: Vec3(3.0, 0.0, 4.0), right: Vec3(0.0, 0.0, 0.0)`), then reverted
and confirmed a clean pass again.

## Verification

- `cargo check -p byroredux --tests`: clean.
- `cargo test -q -p byroredux --bin byroredux`: 1,868 tests passing, 0
  failing (+3 new).
- `cargo test -q --no-fail-fast` (full workspace): **7089 passing, 0
  failing**.
