# #7230: GAME-2026-11-22-07: Several NOT_SAVED reasons promise a load reset that an in-process load never does, and the drain guard matches only two literal phrases

**Labels**: low,gameplay,save-load,test-gap,bug,game:oblivion

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-07**), audited at HEAD 2b40f76b6.

- **Severity**: LOW
- **Dimension**: 7 (state-restore claims; guard blind spot)
- **Domain**: gameplay · **Game**: Oblivion
- **Trigger**: An in-process load in an Oblivion session.
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:463-465` (`FallTracker`, `CastState`, `BowState`), `:802` (`LevelNotices`), `:805` (`StanceTimers`), the guard at `:1263-1285`; `byroredux/src/skill_use_emit.rs:249-257,321-338`; `crates/scripting/src/player_cast.rs:181-189,422-424`; `byroredux/src/bow.rs:204-212`
- **Status**: NEW (GP-04 and GP-06 are the consequential cases; this is the remainder and the guard)
- **Description**: These resources are process-lifetime, and the drain does not touch them. Their cold-start-only reasons go unchecked because the guard looks only for "a load discards" and "The load drain ends". Effects:
  - `LevelNotices.announced` keeps the outgoing session's pending-level count, so loading a save with more pending levels fires a spurious "rest and meditate" notice on the first frame. `a_loaded_pending_level_is_not_announced_again` covers only a cold start.
  - `CastState.impact` keeps a stale target `EntityId`, which `cast.status` prints.
  - `BowState.held` / `cooldown`, `CastState.cooldown_remaining` and `StanceTimers` carry sub-second timers across the load.
- **Evidence**: None of these names appears in the drain body.
- **Impact**: Cosmetic or sub-second. The real cost is that the guard cannot catch the next one; it already missed `ActiveProjectiles` and `FallTracker`.
- **Related**: #6574, #6508, GP-04, GP-06
- **Suggested Fix**: Reset `LevelNotices`, `CastState`, `BowState` and `StanceTimers` in the drain. Widen the guard: a NOT_SAVED reason that says "a load" followed by a reset verb (starts / restarts / re-seeds / resets / forgets) must name a drain line or a restore step, or carry an explicit "cold start only" marker.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
