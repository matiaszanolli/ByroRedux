# #6211: PAR-D1-01: `MAX_GRAPH_STEPS` does not charge the leaf tables read inside each object, so work and memory per step are unbounded: a ~150–300 KB mod behavior graph inside the step budget can take ~12 GiB or minutes of main-thread time

**Labels**: high,bug,import-pipeline,animation,safety,game:fo4

**Source**: docs/audits/AUDIT_PARSERS_2026-10-21.md · **Severity**: HIGH · **Dimension**: Size Discipline · **Location**: `crates/hkx/src/behavior.rs` (`MAX_GRAPH_STEPS`, `Walk`, `read_clip`, `bound_variable`, `read_bone_weights`)

- **Location**:
  - `crates/hkx/src/behavior.rs`:
    - 282 (`MAX_GRAPH_STEPS`)
    - 311–344 (`Walk`)
    - 1121–1152 (`read_clip`)
    - 1066–1079 (`bound_variable`)
    - 1163–1184 (`read_bone_weights`)
    - 1094 (transitions)
    - 1370 / 1395 / 1415 (modifier tables)
  - Caller: `byroredux/src/asset_provider/actor_graph.rs:92-104`.
  - Main thread: `byroredux/src/app_step.rs:1096/1108`.
- **Status**: NEW. This is a residual of #5891 (closed).
- **Trigger Input**:
  - A behavior graph whose blender fan-out re-enters one shared `hkbClipGenerator`. This is the shape #5891 bounded by steps.
  - The shared clip carries `MAX_TRIGGERS` (1,024) triggers and a binding set of `MAX_VARIABLES` (4,096) entries whose `memberPath` is not `playbackSpeed`.
  - Both tables live once in the file (32 KB and 160 KB) and are reached through one pointer each.
- **Description**:
  - e730f9a77 charges one step for each object `Walk::enter`s and one for each child-array entry (`walk.charge(len)` at 1279 / 1304 / 1451 / 1496).
  - Every leaf table read inside an entered object is free:
    - `read_clip` builds a `Vec<ClipTrigger>` of up to 1,024 entries, and `read_generator_node` clones it a second time into `content.blended_clips` (1222).
    - `bound_variable` walks up to 4,096 bindings and allocates an owned `String` (up to 256 B) for each one's member path. It is called for every clip, blender, blend child and transition effect.
    - `read_bone_weights` reads 1,024 weights and 4,096 bindings.
    - The tables of transitions (4,096, each with a condition parse), event ranges, expressions and assign-variable bindings are not charged either.
  - The doc on `MAX_GRAPH_STEPS` ("about 230 ns a step … a graph at the cap costs about a quarter of a second") holds for vanilla files only. A step's real cost scales with these tables by up to about 4,096×.
- **Evidence**: probe `probe_uncharged_leaf_tables`. Blender of 480 children → blender of 1,024 children → one shared clip; release build; the `read_generator` the decoder uses.

  | Triggers / bindings per clip | File | Steps | Clip copies | Peak RSS Δ | Time |
  |---|---|---|---|---|---|
  | 0 / 0 | 146 KB | 984,001 (< 1 M) | 491,519 | 305 MiB | 0.28 s |
  | 64 / 0 | 148 KB | 984,001 | 491,519 | 1,040 MiB | 0.94 s |
  | 128 / 0 | 150 KB | 984,001 | 491,519 | 1,760 MiB | 1.49 s |
  | 0 / 64 | 149 KB | 984,001 | 491,519 | 305 MiB | 2.23 s |
  | 0 / 256 | 158 KB | 984,001 | 491,519 | 305 MiB | 8.45 s |

  Both columns are linear. At the caps this extrapolates to about 12 GiB with 1,024 triggers, and about 135 s with 4,096 bindings, from a file of about 190 KB and about 300 KB respectively. Both are inside `MAX_GRAPH_STEPS`, so the #5891 guard never fires. The probe uses the Havok 2010 offsets; FO4 files go through the same reader functions with the 2014 table.
- **Impact**:
  - An FO4 mod actor (or placed object) whose behavior graph has this shape freezes the main thread or exhausts memory when the actor's graph is built (`populate_actor_graphs`, every tick that has pending actors).
  - `ActorGraphCache` then keeps the result and clones it (PAR-D1-02).
  - Vanilla is unaffected: the census maximum is 115,605 steps on `gunbehavior.hkx`, with small tables.
- **Related**:
  - #5891 / e730f9a77: the step budget.
  - #5893: inline nesting.
  - PAR-D1-02.
  - The census `fallout_4_behavior_graph_census_is_exact` pins steps, not table volume.
- **Suggested Fix**:
  - Thread `walk` into `read_clip`, `bound_variable`, `read_bone_weights`, the transition reader and the modifier table readers, and `walk.charge(len)` for every table they read, or charge bytes rather than entries.
  - Re-measure the shipped maximum with the census and keep the 8× headroom assertion.
  - Add a fixture: a shared clip with 1,024 triggers under a fan-out must fail with the budget error.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
