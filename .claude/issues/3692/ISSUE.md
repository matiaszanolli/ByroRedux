# #3692 — PERF-D9-2026-08-30-04: between_frames is the only CpuFrameTimings field the console cpu_ms: line omits

**Severity**: LOW · **Dimension**: Telemetry & Origin Cost
**Location**: `byroredux/src/systems/debug.rs` (`cpu_breakdown`)

## Prerequisite

The issue itself explicitly said "Fixing this without finding 01 first
would print a number that means the wrong thing" — `PERF-D9-2026-08-30-01`
(#3674) was the fix that made `between_frames_ms` measure the correct
thing (previously it silently absorbed this frame's own render-path cost;
see #3674's own writeup). Fixed #3674 first, earlier in this session, then
this one.

## Fix

Added `between_frames={:.0}` to `cpu_breakdown`'s format string, per the
suggested fix, using `t.between_frames_ms` (now correct post-#3674).
Also added the doc-comment note the issue itself suggested: the printed
buckets nest rather than sum (`atw_post` ⊇ its own pre/scheduler
siblings' remainder, `rof_post_draw` ⊇ the tail of `render_one_frame`),
and `between_frames` is specifically called out as the one bucket that is
NOT nested inside another printed field — it's the only one that can
answer "is this frame's cost outside the engine entirely."

## TESTS (issue's own checklist item)

Added `cpu_breakdown_prints_between_frames`: constructs a
`CpuFrameTimings::default()` with `between_frames_ms` set to a
distinguishing value and asserts the formatted line contains it.

Verified the guard actually catches the regression (this session's
established quality bar): reverted the format string and argument list
back to the 13-field pre-fix shape, reran — the test failed with exactly
the expected message, then restored the fix and confirmed a clean pass
again.

## Verification

- `cargo check -p byroredux --tests`: clean.
- `cargo test -q -p byroredux --bin byroredux`: 1,874 tests passing, 0
  failing (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7096 passing, 0
  failing**.
