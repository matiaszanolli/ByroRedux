# #5449: PERF-D1-2026-10-08-03: `eat_sleep_system` retries a whole-world furniture gather every frame, per actor, whenever an arrived actor cannot be seated

**Labels**: low,performance,gameplay,ai,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5449

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-08.md` — `PERF-D1-2026-10-08-03` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/eat_sleep.rs:164-223` (`seat_at_marker`), `:53-60` (fresh `actors` Vec); `byroredux/src/systems/sandbox.rs:161-181` (`collect_marker_seats`); registration `byroredux/src/boot/schedule/post_update.rs:129`
- **Status**: NEW. Arrived with `00f580e09` (M42 Eat/Sleep).
- **Description**: the module docs say seating is one-shot because `Seated` skips the actor on later ticks. That holds only on success. An arrived actor that is not seated (no matching marker in the cell, or every marker reserved, or `pick_nearest_seat` finds none within the radius) returns from `seat_at_marker` without recording anything and tries again next frame.
  - Each retry calls `collect_marker_seats`, which walks every `Furniture` entity in the world (not just within the radius) and composes a world transform per accepted marker, into a fresh `Vec`.
  - Sleep with no sleep markers does the gather twice (sleep predicate, then the sit fallback).
  - The cost is per arrived-unseated actor, so it multiplies. `sandbox_seat_system` handles the same situation with a persistent `SandboxScratch` (#2033) and one gather per frame behind an `any_unseated` gate (#3354).
  - The no-clip case is cheap (`SandboxSitClip` resource read, then return).
- **Evidence**: `eat_sleep.rs:169-183` (the gather runs before `pick_nearest_seat`), `:204-206` (the `None` seat returns silently).
- **Impact**: in the failure case, N arrived-unseated actors × F furniture × M markers per frame. Bounded by the number of Eat/Sleep actors in that state and uncommon in unmodified content, but unbounded in time. Unmeasured; no quantitative guard exists.
- **Related**: #3354, #2033, `systems/sandbox.rs` (the pattern to mirror).
- **Suggested Fix**: share one gather per frame across all Eat/Sleep actors using a persistent scratch, and record a per-actor "no seat" state that retries on a coarse cadence (for example once per game minute, like the package system).

## Completeness Checks
- [ ] **SIBLING**: `sandbox_seat_system`'s scratch + `any_unseated` gate pattern reused rather than duplicated
- [ ] **TESTS**: A regression test pins this specific fix
