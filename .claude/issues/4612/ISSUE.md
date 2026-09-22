# PERF-D1-2026-09-21-04: The always-on HUD snapshot rebuilds the objective list every frame with no change key

**Labels**: bug, low, performance, gameplay, quests

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW · **Dimension**: 1 — CPU Hot Paths
**Location**:
- `byroredux/src/app_frame.rs:157-183`: the debug-UI-hidden branch of the per-frame `PanelSnapshot`, which is the normal play state
- `byroredux/src/objectives.rs:39-97`: `objectives::snapshot` (+ `quest_display_name` / `flatten_text` at `:101-124`)
- `byroredux/src/inventory.rs:197-221`: `vitals_snapshot`

**Status**: NEW (`db39fe004`, 2026-09-20)
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

Each frame with the operator overlay hidden, the snapshot does:
- an O(all quests) scan of `QuestStageState` into a `running` Vec;
- a Vec per running quest (`iter_quest(..).collect()`), plus a filtered re-collect;
- for each displayed objective, `quest_display_name` (`to_owned` / `format!`) and `flatten_text` (`replace` + `collect`);
- a sort, a truncate and a final collect;
- a `vitals` Vec from `vitals_snapshot`.

The result changes only on quest, objective or actor-value events. While a bench window is open, it is built and then discarded: `app_frame.rs:195-200` sets `vitals` and `objectives` to `None`.

The #1376 gate comment above this branch (`app_frame.rs:134-137`) is stale. It says "the interaction prompt is the only snapshot field populated while the operator overlay is hidden", but `db39fe004` added vitals and objectives to the hidden branch.

## Impact

Microsecond-scale CPU churn every frame on the always-on path: allocations and String building. It grows with the number of running quests that have displayed objectives.

## Related

- #1376: the hidden-overlay gate this branch extends.
- `docs/audits/AUDIT_GAMEPLAY_2026-09-21.md` cites this finding (per-frame HUD objective rebuild).

## Suggested Fix

Cache the `Vec<ObjectiveView>` behind a generation counter that `QuestStageState` / `QuestObjectiveState` mutations bump. Optionally, skip both builds while the bench window discards them, and update the stale #1376 comment.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D1-2026-09-21-04)

## Completeness Checks
- [ ] **SIBLING**: `vitals_snapshot` and `build_interaction_prompt` on the same branch are checked for the same unkeyed per-frame rebuild
- [ ] **TESTS**: A regression test pins that an unchanged quest/objective state reuses the cached list (for example, a generation-keyed cache hit)

