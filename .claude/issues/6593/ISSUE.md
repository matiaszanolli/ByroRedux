# PAR-D1-02: `machine_weight` still ignores a blender child's `BoneWeights::bound` table, so 64 inlines of one bone-binding-heavy sub-behavior are 64 deep copies (+1.5 GiB, 2 s on the main thread)

Labels: medium,import-pipeline,game:fo4,game:skyrim,performance,bug
Source: docs/audits/ (2026-10-31 report)

**Severity**: MEDIUM (bounded memory amplification reachable from a mod file; not an OOM on the 6 GB RT baseline, but it stalls the main thread and the copies are retained in `ActorGraphCache`)
**Dimension**: Parsers / Size Discipline
**Source**: `docs/audits/AUDIT_PARSERS_2026-10-31.md` PAR-D1-02
**Status**: a residual of #6479 / #6212 (both closed); the same defect class as PAR-D1-01 through a table #6479 did not enumerate.

## Title (full)
`machine_weight` still ignores a blender child's `BoneWeights::bound` table, so 64 inlines of one bone-binding-heavy sub-behavior are 64 deep copies (+1.5 GiB, 2 s on the main thread).

## Location
`crates/hkx/src/behavior.rs`. The weight is `machine_weight` (~:889-910), `clip_weight` (~:861), `modifier_weight` (~:871). The table is `BoneWeights.bound` (~:581), filled by `read_bone_weights` (~:1285-1307). `for_each_clip_mut` / `for_each_modifier_mut` (~:586-613) walk blender children but expose neither the child nor its bone weights.

## Description
#6479 weighs a unit by the leaf tables the decode charged a step each. `read_bone_weights` charges `len` (<= `MAX_BONES` 1024) plus `count` (<= `MAX_VARIABLES` 4096) and then pushes one `(bone, variable)` pair per `boneWeights` binding. Nothing deduplicates the pairs, so one child can own up to 4,096 of them (64 KiB), and that table is deep-copied with the cached graph on every inline. The weigh sees only what `for_each_clip_mut` / `for_each_modifier_mut` visit, so a blender child whose generator is a sequence or a nested blender contributes 0 (and a sequence child contributes 1 through the flattened `content.sequences`), however large its `bound`. The 64-resolve cap is then the only limit on the multiplier.

## Trigger Input
A Fallout 4 (packfile v11) or Skyrim sub-behavior with one `hkbBlenderGenerator` of ~390 children that all point at one `hkbBoneWeightArray` (1,024 weights) whose binding set carries 4,096 `boneWeights` bindings (variable index >= 0, `offsetInArrayPlusOne` in 1..=1024). File size about 160 KiB of bindings plus 390 child entries. Decode cost about 5,120 steps per child, about 2.0 M in total, at the `MAX_GRAPH_STEPS` ceiling (derived from `read_bone_weights`, not measured on a real packfile). Weight about 390 against `MAX_INLINED_WEIGHT` 2 M. A root graph that names it from 64 states inlines it 64 times.

## Evidence
Probe `bwprobe` (in-memory graph, the real `inline_references`, 64 references to one heavy graph; N children each with 4,096 pairs and a `Sequence` generator):

| Children | Resolved / inlined | One copy | RSS growth | Time |
|---|---|---|---|---|
| 390 | 64 / 64 | 24.4 MiB | +1,567 MiB | 2.0 s |
| 1,000 (beyond the step budget; shows linearity) | 64 / 64 | 62.5 MiB | +4,017 MiB | 5.1 s |

The 390-child row is what the step budget admits; the 1,000-child row is not reachable from a real packfile.

## Impact
`populate_actor_graphs` -> `build` runs on the main thread (`byroredux/src/asset_provider/actor_graph.rs:106-115`). One FO4 mod behavior stalls the engine about 2 s and pins about 1.5 GiB for as long as the actor graph is cached.

## Suggested Fix
- Count `bound.len()` per blender child in `machine_weight`, plus the child itself (needs a `for_each_blend_child_mut` visitor, or a `Generator::weight()` method that recurses blenders and selectors).
- Or charge each inline the decode's own `Walk::steps`, recorded on the graph, which covers every table by construction and cannot drift as new readers are added. More robust, since this is the second time a hand-enumerated list missed a table.
- Either way, deduplicate `bound` by bone at decode (a bone has one driving variable).
- Pin it with the `bwprobe` shape as a default-lane test beside `a_modifier_list_of_expression_tables_is_weighed_by_what_it_holds`.

## Related
#6479, #6212, #6211 (all closed); #6481 (no real-data inline pin).

## Completeness Checks
- [ ] **SIBLING**: Every owned table read by the hkx decode is covered by the weight (or the decode step count replaces the enumeration)
- [ ] **TESTS**: A default-lane `bwprobe`-shaped test pins the fix
