# CHAR-SD-02: Two prose descriptions of the Skyrim NPC derivation contradict the code they describe

Labels: low,documentation,doc-rot,character,game:skyrim

## Description
#5922 flipped the Skyrim skill-leftover tie order and #5914 added the class pool term, but two prose descriptions still state the old behaviour:
- `crates/plugin/src/esm/records/actor_value_derive.rs:410-416` (doc of `skyrim_class_skill_points`): "actor-value index (lower first)"; the body (`:430-432`) sorts by descending index.
- `docs/engine/charal-skyrim-ruleset.md:726-729` (the "LOCKED" capture paragraph, the authority for constants): "distributes `10 x (level - 1)` points across Magicka and Stamina using their class weights". The code and the same document's line 612 split them across Health, Magicka and Stamina by all three `CLAS DATA` weights (`skyrim_class_pool_points` indexes all three `weights`).

Whoever re-derives from the capture will drop Health's weighted share. The code side of both is the verified one (UESP *CLAS* lines 104-107; `charal-skyrim-ruleset.md:733-737` records that baked `DNAM` values settle the higher-index-first tie).

## Evidence
grep for "lower first" and "across Magicka and Stamina" at the cited lines; both still present at current HEAD.

## Impact
Documentation only; shipped numbers match the baked data per the commits' own oracle.

## Related
#5914, #5922.

## Suggested Fix
Correct the function doc to "higher index first" and the capture paragraph to "Health, Magicka and Stamina by their class weights, Health also +5 per level".

Source: docs/audits/AUDIT_CHARACTER_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Every other site that repeats the stale wording is updated in the same change
