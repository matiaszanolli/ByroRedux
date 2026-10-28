# PAR-D1-01: `inline_references` weighs a sub-behavior by node count, not by what it holds. 64 inlines of one leaf-heavy graph are 64 deep copies, and a ~50 KB Fallout 4 mod graph can take tens of GiB on the main thread

Labels: high, import-pipeline, animation, game:fo4, bug
Source: docs/audits/AUDIT_PARSERS_2026-10-27.md

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-27.md` finding `PAR-D1-01` (HIGH)
**Location (re-mapped to current tree)**: `crates/hkx/src/behavior.rs`: `machine_weight` (:855-877), `MAX_INLINED_WEIGHT` (:732), the charge at ~:1011-1016, the deep copy in `visit_modifier` -> `Arc::make_mut` (:757-758) reached from `remap_machine_events` / `offset_machine_modifier_ids`; modifier reader `read_modifier_node`; `for_each_modifier_mut` (:601-613). Caller: `byroredux/src/asset_provider/actor_graph.rs` (`populate_actor_graphs` -> `build`, main thread)

**Severity**: HIGH (an OOM reachable from a mod file). Residual of closed #6212 (which fixed baseline PAR-D1-02). Dimension: Size Discipline. Untrusted input: mod-supplied FO4 `.hkx`.

## Description
#6212 states its invariant as "every unit weighed here cost at least one step to decode, so the inlined volume stays within one maximal graph's worth however the references repeat". The first half is true; the conclusion does not follow, because one unit can stand for ~a million steps of decoded data:
- **Clips.** A clip weighs 1, but its decode charges 1 + triggers + bindings and it keeps up to 1,024 `ClipTrigger`s.
- **Modifiers.** `machine_weight` counts `for_each_modifier_mut` nodes; that visitor only yields the top-level modifier of each `hkbModifierGenerator` (`Modified { modifiers, inner }`) and does not descend into `hkbModifierList` / `BSModifyOnceModifier` / `hkbEventDrivenModifier` children. A list of 1,024 `hkbEvaluateExpressionModifier`s weighs 1; each child holds up to 1,024 owned `ExpressionData` strings of <=256 B.
- **Copies.** Each resolve clones the cached graph (`actor_graph.rs` `decode_behavior` -> `.clone()`); the event remap and modifier-id offset then call `Arc::make_mut` on every modifier, which still has a second owner in the cache. Every inline is a full deep copy and all 64 are kept in the parent graph.

## Trigger input
An FO4 (packfile v11) sub-behavior shaped as one state -> `hkbModifierGenerator` -> `hkbModifierList` of 1,024 pointers to one `hkbEvaluateExpressionModifier` carrying 1,024 expressions that all point at one 255-char string: ~1.05 M steps (inside `MAX_GRAPH_STEPS` 2 M), weight 3, decodes to ~1 M owned strings (~300 MB) from a ~50 KB file. A root graph naming it from 64 states (siblings, not a cycle) passes the stack cut; 64 resolves fit `MAX_INLINED_REFERENCES`, and the 192 total weight is far below `MAX_INLINED_WEIGHT`.

## Evidence
Probe (`hkxprobe`, calls the real `inline_references` on in-memory graphs of N EvaluateExpression modifiers x 1,024 x 255-char expressions):

| Cached sub-graph | References | Resolved / inlined | Inlined expressions | RSS growth | Time |
|---|---|---|---|---|---|
| 64 modifiers (~19 MB) | 64 | 64 / 64 | 4,194,304 (64x) | +1,216 MiB | 1.2 s |
| 256 modifiers (~76 MB) | 64 | 64 / 64 | 16,777,216 (64x) | +4,866 MiB | 4.8 s |

Growth is linear at ~300 B per expression per copy. Extrapolated: the 1,024 x 1,024 trigger above (~300 MB/copy) gives ~19 GB; a sub-graph at the full 2 M-step budget ~37 GB.

## Impact
`populate_actor_graphs` -> `build` runs on the main thread; the inlined graph is kept in `ActorGraphCache`. One FO4 mod behavior can OOM-kill the engine or stall it for minutes when an actor using it spawns. The trigger-table variant (clips weigh 1, keep 1,024 triggers each) reaches the same multiplier with smaller numbers.

## Related
#6212, #6211 (closed); PAR-D4-01 (no real-data inline pin).

## Suggested Fix
Charge each inline what its decode cost: (1) record the decode's `Walk::steps` on the `BehaviorGraph` and charge that against `MAX_INLINED_WEIGHT`, or (2) make `machine_weight` count leaf-table lengths and recurse through `visit_modifier` children. Consider charging the copies as well, since `make_mut` makes each inline a full copy. Pin with the modifier-list fixture from the trigger input.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **TESTS**: A regression test pins this specific fix

