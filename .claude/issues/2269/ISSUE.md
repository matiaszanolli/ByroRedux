# NEW-CONC-1: CinematicPresentationState↔QuestStageState lock order inverted between two add_exclusive systems

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-08-03.md` (finding `NEW-CONC-1`)
**Severity**: MEDIUM
**Dimension**: ECS Lock Ordering & Deadlock
**Location**: `crates/scripting/src/fragment.rs:605-610,642-660` (nested acquire inside `apply_effect`, called under a held `QuestStageState` write lock) vs `crates/scripting/src/cinematic.rs:251-290` (`dispatch_player_cinematic_animation_event`, sequential acquire in the opposite order)
**Labels applied**: `medium`, `sync`, `bug`

## Description

`quest_fragment_dispatch_system` (`fragment.rs:1035-1055`) acquires `(QuestStageState, QuestObjectiveState)` via `resource_2_mut` and holds both across its entire cascade loop, which calls `apply_effect` for each queued effect. Two of `apply_effect`'s arms — `Effect::SetSittingRotation` (line 606) and `Effect::RegisterPlayerAnimationEvent` (line 643) — nested-acquire `world.try_resource_mut::<CinematicPresentationState>()` while `QuestStageState`'s write guard is still held by the caller. This establishes lock order **QuestStageState → CinematicPresentationState**.

Separately, `dispatch_player_cinematic_animation_event` (`cinematic.rs:251-290`, called from `cinematic_animation_event_system`, a different `add_exclusive` system) acquires `CinematicPresentationState` first (scoped to a block, dropped at line 281), then acquires `QuestStageState` at line 283 — the reverse order.

Same finding class as already-open `CHARAL-D3-01` (#2153) and `SAVE-D3-02` (#2154), but sharper: a genuine order *reversal* between two systems on the identical resource pair — exactly what `BYRO_LOCK_ORDER_CHECK`'s cross-thread ABBA graph exists to catch, except undocumented at both sites.

## Evidence

`fragment.rs:1054-1055` (`resource_2_mut::<QuestStageState, QuestObjectiveState>()`, held through the `while let Some(...) = queue.pop()` loop starting line 1093) → `fragment.rs:606`/`:643` (`try_resource_mut::<CinematicPresentationState>()` reached from inside that loop via `apply_effects`→`apply_effect`). Contrast with `cinematic.rs:262` (`CinematicPresentationState` acquired, block-scoped, dropped at `:281`) then `cinematic.rs:283` (`QuestStageState` acquired after). Both `quest_fragment_dispatch_system` and `cinematic_animation_event_system` confirmed `add_exclusive` in `byroredux/src/boot.rs`.

Re-verified directly against current code prior to filing (2026-08-03): symbols, line ranges, and lock-hold shape all confirmed present as described.

## Impact

No live deadlock — both systems run serially on the main thread by construction (exclusive systems never overlap). Becomes a real cross-thread ABBA risk the moment either system is promoted to the parallel lane, or if a third path ever holds `CinematicPresentationState` while acquiring `QuestStageState` concurrently with `quest_fragment_dispatch_system`'s in-progress cascade.

**Trigger conditions**: Requires a scheduler change (either system moved to `add_to_with_access`/parallel lane) — not reachable in the current exclusive-only scheduling.

## Related

#2126 (`SCR-D6-NEW3-03`, closed, same finding class, established the doc-comment convention this new code didn't inherit), #2153 (`CHARAL-D3-01`, open, same class, different resource pair), #2154 (`SAVE-D3-02`, open, same class, different resource pair), #313/#1410 (the TypeId-sorted / `BYRO_LOCK_ORDER_CHECK` machinery this pattern depends on).

## Suggested Fix

Preferred — route the two `apply_effect` arms through a queued side-effect (matching the existing `MotionTypeChangeRequest`/component-marker pattern used by `SetVehicle`/`SetMotionType`) instead of a direct nested resource acquisition. Cheaper alternative — add a `#2126`-style doc comment to both sites cross-referencing each other and the exclusive-scheduling dependency.
