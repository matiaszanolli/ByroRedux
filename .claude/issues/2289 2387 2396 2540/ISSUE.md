# Issue Batch: 2289, 2387, 2396, 2540

All four are LOW-severity test-coverage-gap findings from audit sweeps. No behavior is broken;
each asks for regression tests pinning an invariant that today is only correct "by inspection".

## #2289 — SCR-D5-NEW5-02 (scripting)
`crates/scripting/src/translate/effects.rs` — ~half of the ~26 new effect primitives (SetOpen,
SetPlayerRestrained, SetPlayerControls/DisablePlayerControls/EnablePlayerControls,
SetPlayerAiDriven, SetHudCartMode, PlayIdle, SetVehicle, TetherToHorse, SetMotionType's decline
path, SetSittingRotation, ExitCart, PlayerImodAnimation/PlayerFurnitureAnimation,
EvaluatePackage, Wait, StartScene/StopScene) have a positive-path test but no decline-path
(`?`/arg-count/arg-type guard) test.

Fix: one `assert_eq!(lower_fragment(&body), None)` decline test per untested primitive.

## #2387 — ECS-D1-04 (ecs)
`crates/core/src/ecs/lock_tracker.rs:505-571` — the cross-rayon-worker ABBA deadlock-detection
guarantee has no real multi-thread test. The existing `global_graph_detector_end_to_end` test
runs single-threaded, calling `track_read`/`untrack_read` directly, bypassing `World` entirely.

Fix: add a `#[cfg(debug_assertions)]` test building a real `World` with two registered storages,
driving `query::<A>() → query::<B>()` on one `std::thread` and `query::<B>() → query::<A>()` on
another (barrier-synchronized, `catch_unwind` both sides) under `set_enabled_for_tests(true)`,
asserting exactly one side panics with "cross-thread deadlock risk".

## #2396 — ECS-D2-NEW-02 (ecs)
`crates/core/src/ecs/packed.rs:256-288` — `PackedStorage::remove_entities_erased`'s two
load-bearing invariants (ascending sort order after merge-compaction; `TRACK_CHANGES` dirty
marking via hand-inlined `self.dirty.push`) have no dedicated test. This is the only removal
route used by cell unload (`unload.rs:245` → `despawn_batch`).

Fix: two tests in `packed.rs`'s test module — (1) `iter()` order still ascending after removing a
scattered victim set from a >3-element storage; (2) on a `TRACK_CHANGES` fixture, `take_dirty()`
contains exactly the removed ids.

## #2540 — SCR-D5-NEW10-02 (scripting)
`crates/scripting/src/translate/effects.rs:529,541,552` — the `u16`→`i32` widen for
`SetObjective{Displayed,Completed,Failed}`'s index field (`i32::try_from(int_arg(args, 0)?).ok()?`)
is a correct fix (matches `QuestObjective::index`'s documented i32-on-FO3/FNV representation) but
has no test exercising a negative index or an i32-overflow decline. Explicitly folds into #2289's
tracking.

Fix: one test per `SetObjective*` primitive for negative-index-lowers-correctly and
overflow-declines cases.

## Domain classification
- #2289, #2540 → **scripting** → `byroredux-scripting` (crate path is `crates/scripting`; note:
  need to confirm actual crate name via Cargo.toml)
- #2387, #2396 → **ecs** → `byroredux-core`

## Plan
All four are additive test-only changes, no production code path changes expected (except
possibly routing `packed.rs`'s inline `self.dirty.push` through `mark_dirty` per #2396's
suggestion, which is optional/mechanical). Implement together since they're small and related in
pairs (2289+2540 same file; 2387+2396 same crate).
