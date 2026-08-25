# Issues 2271, 2372, 2376, 2384

## #2271 (LOW, safety) — No Miri coverage for ECS cached-pointer aliasing model
`crates/core/src/ecs/query.rs:23-144` — QueryRead/QueryWrite/ComponentRef cache a raw
pointer from the guard at construction; soundness relies on convention (guard never
re-borrowed after construction), not compiler enforcement. No miri job in CI.
Suggested fix: add `cargo +nightly miri test -p byroredux-core` CI job scoped to ecs
module, OR restructure with PhantomData so re-deriving a pointer post-construction is
a compile error. Domain: ecs → byroredux-core.

## #2372 (MEDIUM, epic) — EX-16: Integrate REGN, NAVM, ambient audio, AI w/ ext streaming
Plan-level epic. Acceptance spans: REGN-driven ambient/fog/weather/ground-cover/encounter
priority, NAVM tile load/unload with cross-cell path connectivity, actor/package
suspend/migrate/resume across stream boundaries, audio emitter crossfade+reclaim,
debug telemetry, boundary/soak tests. NOT a scoped bug fix — multi-system feature work.

## #2376 (HIGH, epic) — EX-06/07: Exterior boundary benchmark + deadline-bounded streaming
Plan-level epic. Acceptance: deterministic 2+-cell-boundary-crossing benchmark harness,
per-cell p50/p95/max timings across emit/parse/apply/unload/LOD/frame, convert
attempt-count LOD budgeting to wall-clock deadline budgeting across NIF finalize/
static placement/terrain-water-precombine/texture-mesh-upload/BLAS/LOD, hitch detection,
run on FNV + one newer-engine worldspace. NOT a scoped bug fix — benchmark infra + budget
rearchitecture across renderer+streaming.

## #2384 (LOW, bug) — ABBA panic orphans thread-local tracker row
`crates/core/src/ecs/lock_tracker.rs` track_read/track_write mutate LockState
(read_count/has_write) BEFORE calling global_order::record_and_check, which can panic
(ABBA). Panic fires before TrackedRead/TrackedWrite is constructed, so the #137/#2149
RAII release-on-drop guard never runs — the row survives the unwind. Fix: reorder so
the check happens before the mutation (compute held_others from pre-insert state), or
wrap mutation in a scope guard that rolls back on unwind. Domain: ecs → byroredux-core.

## Domain classification
- #2271, #2384 → `ecs` → `byroredux-core` — concrete, scoped fixes.
- #2372, #2376 → multi-system feature epics, not single-site bugs. Flagging for scope
  discussion with user rather than attempting partial "fixes."

## Resolution

### #2384 — already fixed, closed without a new commit
`track_read`/`track_write` in `crates/core/src/ecs/lock_tracker.rs` already call
`global_order::record_and_check` (the panicking ABBA check) *before* mutating
`LockState`/inserting the row — exactly the suggested fix — landed in `5428e872`
prior to this session. A genuine `catch_unwind`-based regression test already exists
(`global_graph_detector_end_to_end`, asserting `is_clean()` after a caught ABBA panic,
not the module's manual `LOCKS.with(...).clear()` workaround). Verified passing at HEAD.

### #2271 — added scoped Miri CI job
Went with the CI-job option (not the PhantomData restructuring, which can't actually
work here — the wrapper types must keep the real guard alive to hold the RwLock; there
is no marker-type substitute for that). Added `ecs-query-miri` to
`.github/workflows/ci.yml`, running `cargo miri test -p byroredux-core --lib
ecs::world::tests -- --skip resource_visible_to_system_via_scheduler`.

- Scoped to `ecs::world::tests` (not the whole crate/workspace) because a full-crate
  Miri run hits an unrelated, unfixable incompatibility: `resource_visible_to_system_
  via_scheduler` drives `Scheduler::run`, and rayon's work-stealing goes through
  crossbeam-epoch's platform-thread pinning, which Miri's concurrency model doesn't
  support (confirmed locally — full-module run aborts inside `crossbeam_epoch::
  default::with_handle`). Skipping just that one test keeps the other 83 tests in the
  module running clean under Miri (~53s locally), which is exactly the set that
  constructs and exercises QueryRead/QueryWrite/ComponentRef end to end.
- **Verified the job actually catches a regression** (the issue's own TESTS
  completeness check): temporarily reintroduced a #35/#1367-class violation in
  `QueryWrite::storage_mut` (re-deriving `&mut` from `self.guard` before using the
  cached pointer) and confirmed Miri fails immediately with a precise Stacked Borrows
  diagnostic naming both the invalidating retag site and the original cache site.
  Reverted before committing — `git diff` on `query.rs` is clean.
- No SAFETY-comment changes needed — no restructuring of the cached-pointer fields was
  done, so the existing `#1367` SAFETY comments on `query.rs:64,135,143` and
  `ComponentRef::deref` still accurately state the guard-outlives-pointer invariant.

## Epics flagged for scope discussion (not attempted)
#2372 and #2376 are plan-level epics (acceptance criteria spanning REGN/NAVM/AI/audio
integration, and a cross-system streaming-benchmark + deadline-budget rearchitecture,
respectively) — not single-site bugs. Attempting a partial implementation and closing
either as "fixed" would misrepresent the scope. See conversation for the question
posed to the user.
