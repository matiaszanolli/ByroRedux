# 3277: SCR-D6-2026-08-24-01: quest_fragment_dispatch_system's tail QuestStageAdvancedBatch write is the one non-defensive producer among five same-frame writers

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_SCRIPTING_2026-08-24.md` (SCR-D6-2026-08-24-01)

## Description

`quest_advance.rs:463-466`'s own comment states the invariant every other writer to `QuestStageAdvancedBatch` follows: append the whole producer batch while holding the storage write lock, since another same-frame producer may already have populated the sink. `quest_fragment_dispatch_system`'s own tail (`fragment.rs:1928-1931`) does not follow it — unconditional `insert()`, no `get_mut`-and-extend check. Harmless historically because this system was the last writer registered; two new same-frame producers (`quest_alias_readiness_stage_system`, `scene_fragment_dispatch_system`, both landed 2026-08-23) now run immediately before it.

## Location

`crates/scripting/src/fragment.rs:1928-1931` (the defect); contrast with `crates/scripting/src/papyrus_demo/quest_advance.rs:467-473`, `crates/scripting/src/quest_stages.rs:947-953`/`1129-1137`, `crates/scripting/src/fragment.rs:1441-1446` — all correctly defensive.

## Evidence

```rust
// fragment.rs:1928-1931 — the one non-defensive writer
if let Some(mut q) = world.query_mut::<QuestStageAdvancedBatch>() {
    q.insert(player_entity, QuestStageAdvancedBatch(chained));
}
```

## Impact

None observable today (no consumer reads the post-dispatch marker state). Becomes a silent data-loss bug the moment a same-frame consumer is added after `quest_fragment_dispatch` in the schedule.

## Related

Not a duplicate of #1864 (CLOSED, intra-call case; this is cross-system).

## Suggested Fix

Apply the same `get_mut`-then-`extend`-else-`insert` pattern the other five writers use. Consider a shared `push_quest_stage_advances` helper.

## Completeness Checks
- [ ] **SIBLING**: Matches the get_mut-then-extend pattern of the other five writers
- [ ] **TESTS**: A regression test with two same-frame producers asserting both batches survive
