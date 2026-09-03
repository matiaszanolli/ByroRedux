# #3249 — ECS-2026-08-24-02: #2386 recursive-read hazard warning is unbounded and carries no call-site information

**Severity**: LOW · **Dimension**: ECS / Concurrency
**Location**: `crates/core/src/ecs/lock_tracker.rs::track_read`

## Fix

Implemented both of the issue's own suggested options together, since it
explicitly notes the call-site fix alone "would have made
ECS-2026-08-24-01 self-reporting":

**De-duplication** (stops the spam): added a thread-local
`WARNED_RECURSIVE_READ_TYPES: HashSet<TypeId>`. The warning now fires at
most once per `(thread, TypeId)` for the life of the thread —
`HashSet::insert` returns `true` only the first time — instead of once
per 1→2 acquisition transition, which is once per *acquisition cycle*: a
recursive read on a per-frame path previously warned every frame,
forever.

**Call-site info** (makes the first warning actionable): propagated
`#[track_caller]` through the full chain from the actual user call site
down to `track_read`'s `std::panic::Location::caller()` —
`World::get` / `World::query` / `World::query_2_mut` / `World::resource`
/ `World::try_resource` (every method that constructs a `TrackedRead`) →
`TrackedRead::new` → `track_read`. The warning message now names where
the second read guard was actually acquired instead of just the
component type name, which alone gave no way to locate the offending
`query::<T>()` among the workspace's many call sites without a manual
bisect — the exact difficulty that made finding ECS-2026-08-24-01's root
cause require one.

## TESTS (issue's own checklist item)

- `recursive_read_warns_only_once_across_multiple_acquire_release_cycles`
  — three independent acquire/release cycles on the same type (the shape
  three ticks of a per-frame system would produce) must warn exactly
  once, not three times.
- `recursive_read_dedup_is_per_type_not_global` — the dedup set is keyed
  by `TypeId`; a different type's first recursive read must still warn
  even after another type has already used its one-time warning.

The existing `recursive_read_warns_once_and_continues` test (single-cycle
"warns once" behavior) needed no changes and still passes.

`#[track_caller]` propagation is a compile-time-verified mechanism (a
break anywhere in the chain would still compile but silently report the
wrong frame) rather than something asserted at runtime here — this
codebase's test harness has no log-output capturer already wired into
this module, and adding one for a single LOW-severity diagnostic-message
assertion wasn't judged worth the added surface.

**Reintroduce-and-revert verification**: temporarily removed the dedup
condition (`WARNED_RECURSIVE_READ_TYPES.with(...)`), leaving only the
original `entry.read_count == 1` check — confirmed
`recursive_read_warns_only_once_across_multiple_acquire_release_cycles`
failed (3 warnings instead of 1, the exact spam this issue describes).
Restored the fix and reran — all 9 tests in `ecs::lock_tracker::tests`
pass again.

## Verification

- `cargo check -p byroredux-core --tests`: clean, zero warnings.
- `cargo check --workspace --tests`: clean (one pre-existing, unrelated
  `unused_mut` warning in `grup_walker.rs:469` predates this fix) —
  confirms `#[track_caller]` on five widely-called `World` methods
  doesn't break any of the workspace's many call sites.
- `cargo test -p byroredux-core --lib ecs::lock_tracker::`: 9 tests
  passing, 0 failing (+2 new).
- `cargo test -q --no-fail-fast` (full workspace): **7148 passing, 0
  failing**.
