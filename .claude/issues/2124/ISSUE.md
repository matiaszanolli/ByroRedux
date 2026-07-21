# SCR-D6-NEW3-02: Quest-fragment cascade genuine-transition guard compares against the wrong variable — can drop or duplicate SetStage cascades

**Issue**: #2124
**Labels**: medium, bug
**Dimension**: Scripting Runtime Systems
**Untrusted-Input**: Yes — driven by authored quest-stage numbers and fragment effect lists from real `.pex`/VMAD data.
**Location**: `crates/scripting/src/fragment.rs:488-495` (`quest_fragment_dispatch_system`, cascade loop)
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 6)

## Description

The cascade re-queue guard is:
```rust
if adv.new_stage != stage {
    queue.push((adv.quest, adv.new_stage));
}
```
where `stage` is the stage of the **currently-dispatching** fragment (from the outer `while let Some((quest, stage)) = queue.pop()`), not `adv.previous_stage` (the actual pre-image already carried on the event) and not scoped to `adv.quest == quest`.

The doc comment's intent ("skip a no-op re-set of the same stage") is only correctly implemented for a fragment that re-sets its own currently-running stage — every other shape is a coincidental, wrong comparison:

- **False negative**: quest A (dispatching at stage `S`) sets a *different* quest B to a stage number that happens to numerically equal A's own stage `S`. The guard compares `S != stage` where `stage == S` → false → B's genuine transition is silently never queued.
- **False positive**: one fragment body issues two effects both resolving to the same `(quest, new_stage)`; the second's true no-op (`previous_stage == new_stage`) can still satisfy `adv.new_stage != stage` (comparing against the *original* dispatching stage) → that stage's fragment (e.g. an `AddItem`) re-runs a second time in the same cascade.

The correct check needs no outer-loop variables at all: `adv.previous_stage != adv.new_stage`.

## Evidence

`crates/scripting/src/fragment.rs:461-497` (full cascade loop); `crates/scripting/src/quest_stages.rs:112-118` (`set_stage` always returns the previous value and always inserts, even on a same-value re-set — the caller is the only place able to distinguish genuine vs. no-op). Existing tests (`dispatch_cascades_chained_set_stage`, `populate_from_script_binds_stages_to_the_right_fragments`) only exercise `adv.quest == quest` with one `SetStage` per fragment, where `stage` and `adv.previous_stage` coincide by construction — the bug is real but untested.

## Impact

Silent loss of a different quest's scripted side effects when stage numbers coincidentally collide across quests in the same cascade (plausible — quest stages cluster around small round numbers like 0/10/20 across many independently-authored quests) — or silent duplicate application of a stage's effects (duplicate item grants) when one fragment converges two effects on the same value. Both are content-correctness bugs with no crash and no log line.

## Suggested Fix

Replace `adv.new_stage != stage` with `adv.previous_stage != adv.new_stage`. Add regression tests for both the cross-quest stage-number collision case and the same-fragment double-`SetStage`-converging case, asserting the target fragment runs exactly once (or not at all, for the genuine no-op).
