# #7365 — CONC-2026-11-23-06

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-11-23.md` (finding CON-06).

- **Severity**: LOW
- **Dimension**: 3 (canonical order is the arbiter for new multi-lock code)
- **Location**: `docs/engine/ecs.md:571-750` (unchanged since the baseline: `git log 3334bbe48..HEAD -- docs/engine/ecs.md` is empty)
- **Status**: NEW
- **Description**: the policy is the stated arbiter for hand-ordered holds, but the orders fixed in this window live only in code comments and test names: `InventoryCatalog` before `Inventory` (CONC-4, `extensions/capture.rs:265`, and now held to the end of `capture_entity_projections`); `WalkSpeed` before `GlobalTransform` (CONC-1, `eat_sleep.rs:284`); `CreatureActor` before `Furniture` / `Seated` / `GlobalTransform` (ace99c164, `sandbox.rs:337-343`); `RapierHandles` -> `WaterContact` and `WaterContact` -> `ActorVitals`, so `ActorVitals` must not be held across `RapierHandles` (#7315, `trap_hits.rs:68-79`); `ActorValues` -> `ActorValueNames` (`read_caster`, `player_cast.rs:213`; the reverse is the closed #7063 cycle); `FormIdComponent` before `FormIdPool` appears only in the skill. The next author of a gather has nothing to consult but the skill's anecdotes.
- **Evidence**: the doc's clusters stop at `Transform … StringPool`, the cinematic read-onlys, the render-skip sinks, `CharacterRuleset -> ActorValues`, animation, scene/quest and physics shapes.
- **Trigger Conditions**: none; documentation.
- **Impact**: the same cycle shapes keep being reintroduced by new code (the skill lists 16 of them). Low, no runtime effect.
- **Verification Path**: `rg "InventoryCatalog|WalkSpeed|CreatureActor|WaterContact" docs/engine/ecs.md` returns nothing for the lock section.
- **Suggested Fix**: add a short "settled pairs" table to the policy with the six directions above and the test that pins each.
- **Dimension agent ID**: CON-D3-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other record-time latches, other temp stems, other exclusive rows)
