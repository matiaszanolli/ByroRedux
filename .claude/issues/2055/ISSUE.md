# TD1-006: records/actor.rs crossed 2000 LOC — parse_npc is a 332-line, 29-arm sub-record match

**GitHub Issue**: #2055
**Labels**: low,import-pipeline,legacy-compat,tech-debt,bug

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/plugin/src/esm/records/actor.rs:505-836` (`parse_npc`)

## Description
`parse_npc` interleaves 4+ separately-gated data groups (identity/faction, inventory, runtime FaceGen, FO4 pre-baked FaceGen, actor-value properties) in one 29-arm match — the shape that produced the closed #1996 divergent-branch bug.

## Evidence
Confirmed live: `crates/plugin/src/esm/records/actor.rs` is 2140 LOC total; `pub fn parse_npc(` starts at line 505, matching the report's claimed location exactly.

## Impact
Highest-traffic ESM record parser; every placed NPC touches it.

## Related
#1996 (closed) — precedent for why the split has real correctness value.

## Suggested Fix
Extract each data group into a `parse_npc_<group>` helper called from a slim dispatch loop; extract the 960-line test module separately.

**Age**: file created 2026-04-07, last touched 2026-07-15.
**Effort**: medium (group extraction) + trivial (test split)

## Completeness Checks
- [ ] **SIBLING**: #1996 is the precedent bug class this exact file/function shape produced before — verify the split doesn't reintroduce a similar divergent-branch risk
- [ ] **TESTS**: A regression test pins the #1996 fix through the refactor (same NPC record produces identical parsed output before/after the split)
