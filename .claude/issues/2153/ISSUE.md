# 2153: CHARAL-D3-01: pool_regen_tick_system holds a 3-deep nested lock stack whose safety rests on undocumented exclusive scheduling

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2153
**Labels**: bug, ecs, low

---

## Severity
LOW

## Dimension
ECS Lock Ordering & Deadlock — `/audit-concurrency` 2026-07-25

## Location
`crates/core/src/character/regen.rs:120-150`

## Description
The only new CHARAL system touching `World` builds a hold-stack of three distinct locks by sequential acquisition rather than a TypeId-sorted paired accessor: `PoolRegenConfig` (read) is held through `try_resource_mut::<PoolRegenAccumulator>()`, then through `try_resource::<CharacterRuleset>()`, then through `query_mut::<ActorValues>()` and the per-actor loop. Correct today only because the system is registered `add_exclusive(Stage::Update, ...)` — a dependency living in a different crate and unstated in `regen.rs`. Same finding class as the already-closed #2126 (`SCR-D6-NEW3-03`), whose fix was a documented "nested-lock safety depends on exclusive scheduling" comment that this new code didn't inherit. Also same class as the currently-open #2130 (`quest_advance_system`, a different site).

## Evidence
Held set at the `query_mut::<ActorValues>()` call (`regen.rs:137`): `{PoolRegenConfig(R), CharacterRuleset(R), ActorValues(W)}`. Confirmed against current code: `regen.rs:121` (`try_resource::<PoolRegenConfig>`), `:134` (`try_resource::<CharacterRuleset>`), `:137` (`query_mut::<ActorValues>`), all in one function, no drops between.

## Impact
No live deadlock. The risk is a future maintainer moving this system to the parallel lane or adding a system that acquires `ActorValues` before `CharacterRuleset` — either creates a genuine ABBA only caught under `BYRO_LOCK_ORDER_CHECK=1` or as a production hang.

## Trigger Conditions
Only reachable once `PoolRegenConfig` is actually inserted (currently `build_character_ruleset` returns `None` for it, so the system short-circuits). Deadlock additionally requires the scheduler change described above.

## Related
#2126 (closed, same finding class), #2130 (open, same finding class, different site — not a duplicate, different file/system).

## Suggested Fix
Preferred — drop `config` early (copy the AVIF ids into locals before the `CharacterRuleset` acquire), reducing the hold-stack from 3 to 2. Alternative — port the #2126 doc block verbatim onto `pool_regen_tick_system`.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
