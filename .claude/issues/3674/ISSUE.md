# #3674 — PERF-D9-2026-08-30-01: between_frames_ms is sampled after draw_frame returns, so it silently absorbs the entire in-engine render path it exists to exclude

**Severity**: MEDIUM · **Dimension**: Telemetry & Origin Cost
**Location**: `byroredux/src/app_frame.rs` (`render_one_frame`)

## Fix

Implemented the issue's suggested fix exactly: captured the gap at
`rof_pre_t0`'s own scope (`render_one_frame`'s true start, before any of
this frame's own work — `build_render_data`, `draw_frame`, present — has
run), and used that captured `between_frames_ns` at the `CpuFrameTimings`
assignment site instead of re-reading `self.last_redraw_end.elapsed()`
there (which by that point in the function has already run this frame's
entire render path, systematically over-attributing that cost to "outside
the engine").

Also annotated the historical "501 ms `between_frames` gap" comment in
`app_events.rs` (cited by the issue's own Impact section as still
presenting the skewed figure without qualification) to note the
measurement predates this fix and a fresh one would read lower — kept as
motivation for the pre/scheduler/post split it introduces, not as a
currently-accurate number.

`last_redraw_end`'s own stamp (at the end of the function) was already
correct — untouched.

## TESTS (issue's own checklist item)

`render_one_frame` is a method on `App`, needing a real `VulkanContext`
(70+ loader fields, no safe test defaults) — this file's own established
convention for that exact situation (two sibling test modules,
`skin_dispatch_ran_rollback_scope_tests` and
`material_overflow_no_panic_tests`, both say so explicitly) is a static
source-position assertion via `include_str!`, not a live test. Added
`between_frames_capture_ordering_tests`, pinning that the capture happens
before `draw_frame` and that the `CpuFrameTimings` assignment reads the
captured local rather than re-deriving.

**Caught and fixed my own instance of the exact trap this file's
top-of-file comment warns about** ("each `find` would have matched the
needle literal inside the test module and passed while pinning
nothing"): my first draft searched the *whole* file via `include_str!`
for the assignment line, but that search string is byte-identical to the
`.find(...)` call's own argument — so it would ALWAYS match (inside the
test's own source) even with the production site deleted outright,
making the assertion structurally unable to fail. Caught by deliberately
reintroducing the bug and observing the test still pass. Fixed by scoping
the search to `&src[..module_start]` (everything before this test
module's own `mod` line), which only the production code sits within.
Re-verified: reintroducing the bug with the scoped version now correctly
fails with the expected message, and restoring the fix passes again.

## Verification

- `cargo check -p byroredux --tests`: clean.
- `cargo test -q -p byroredux --bin byroredux`: 1,873 tests passing, 0
  failing (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7095 passing, 0
  failing**.
