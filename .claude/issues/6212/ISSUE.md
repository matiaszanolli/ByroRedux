# #6212: PAR-D1-02: `inline_references` multiplies the per-decode budget by up to 65: each of its 64 resolves is a full clone of a cached decoded graph, all live at once on the recursion (10%-of-budget self-reference peaks at 2 GiB / 9.7 s)

**Labels**: high,bug,import-pipeline,animation,safety,game:fo4

**Source**: docs/audits/AUDIT_PARSERS_2026-10-21.md · **Severity**: HIGH · **Dimension**: Size Discipline · **Location**: `crates/hkx/src/behavior.rs` (`inline_references` / `inline_machine`); `byroredux/src/asset_provider/actor_graph.rs` (`decode_behavior`, `build`)

- **Location**:
  - `crates/hkx/src/behavior.rs:933-993` (`inline_references`, `inline_machine`, `inline_references_with`; the height check is at 966).
  - `byroredux/src/asset_provider/actor_graph.rs:92-115` (`decode_behavior` returns `.clone()` of the cached graph on every resolve; `build` inlines on the main thread).
- **Status**: NEW. This is a residual of #5893 and #5891 (both closed).
- **Trigger Input**: an FO4 behavior graph with a state whose generator is an `hkbBehaviorReferenceGenerator` naming its own path (or two graphs naming each other), plus a body of N steps that is still under `MAX_GRAPH_STEPS`.
- **Description**:
  - `MAX_GRAPH_STEPS` bounds one decode. `decode_behavior` memoises that decode and hands out a deep clone on every call. The generator tree is `Box`/`Vec`-owned, and `machine_modifier_id_bound` clones each state's generator again.
  - `inline_machine` resolves a reference, then recurses into the clone's own references (`inner.inline_references_with(resolve, budget)`, 961). It applies the `MAX_MACHINE_NESTING` height check only after that recursion returns (966).
  - A self-reference therefore stacks up to `MAX_INLINED_REFERENCES` (64) live clones before any is refused. The ones that fit the height limit (32 here) are kept in the final graph, which `ActorGraphCache` retains for the process.
  - Nothing bounds the total size of the inlined result. 3ce34ed28 bounded the nesting depth and hashed the name merge, but not the volume.
- **Evidence**: probe `probe_inline_clone_amplification`, built from `blender 48 → 1,024 → clip`, one state that refers to itself, and a resolver that clones, as `decode_behavior` does:

  ```
  PROBE2 file=104683B steps=98401 one_copy=30MiB resolves=64 machines_retained=32 peak=1990MiB elapsed=9.66s
  ```

  Scaled to a graph near the step cap (305 MiB for each copy, from the PAR-D1-01 baseline row), this is about 20 GB at peak and about 100 s. PAR-D1-01's table amplification multiplies it further.
- **Impact**:
  - A mod graph that names itself freezes or kills the process the first time an actor using it spawns.
  - Separately, `*ids += inner_bound` (977) and `*id += offset` (823) add file-offset-derived modifier ids across up to 64 inlines in `u32`. A graph file of 64 MiB or more overflows them: a panic in dev builds, and silently colliding modifier runtime state in release.
- **Related**: #5893 (closed), #5891 (closed), PAR-D1-01. #5861 / #6005 graph restart scope (gameplay).
- **Suggested Fix**:
  - Give `inline_references` one shared work budget: charge each resolved graph's step count, or a node count, against a total such as `MAX_GRAPH_STEPS`.
  - Run the height check before recursing into a resolved graph. Skip a reference whose path is already on the inline stack, since a self-reference can never fit.
  - Use `checked_add` on the modifier-id offsets.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
