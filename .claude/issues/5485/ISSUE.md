# #5485: NIF-D4-2026-10-09-01: Regression of #1269 — NIF scene-graph walkers have no revisit guard. A self-referencing `NiNode` stack-overflows three walkers, and a two-way self-reference blows up the two #1269 depth-capped walkers as 2^depth. All five…

**Labels**: bug, high, nif, nif-parser, safety

**Source**: `docs/audits/AUDIT_NIF_2026-10-09.md` — finding `NIF-D4-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

> **Regression of #1269** (closed).

- **Severity**: HIGH. This is an untrusted-input reader with an uncatchable process abort. Severity precedents are #4148 (NIF bounding-volume recursion, HIGH), #3237 and #3503 (ESM recursion, HIGH) and #4317 (HIGH).
- **Dimension**: Geometry Extraction & Import Handoff (`import/walk/`). Streaming impact is covered under Dim 6.
- **Game Affected**: all `NifVariant`s. The walkers are game-agnostic, and the trigger is a malformed or hostile mod NIF.
- **Location**:
  - **No depth cap and no cycle guard**:
    - `crates/nif/src/import/walk/lights.rs:18`, `walk_node_lights`, which self-calls at `:38` and `:53`;
    - `crates/nif/src/import/walk/texture_effect.rs:18`, `walk_node_texture_effects`, which self-calls at `:39` and `:54`;
    - `crates/nif/src/import/walk/emitter.rs:768`, `walk_node_particle_emitters_flat`, which self-calls at `:791` and `:818`.
  - **Depth-capped but not breadth-capped**: `crates/nif/src/import/walk/mod.rs:259` (`MAX_NIF_NODE_DEPTH = 128`). This applies to `walk_node_hierarchical` (`:288`, cap `:296`, self-calls `:360` and `:469`) and `walk_node_flat` (`:733`, cap `:741`, self-calls `:779` and `:842`).
  - **Entry points**: `crates/nif/src/import/mod.rs:514` (`import_nif_lights`), `:84` (`import_nif_particle_emitters`), `:546` (`import_nif_texture_effects`).
  - **Callers**:
    - `byroredux/src/streaming/pre_parse.rs:297-298` and `:311-316` (the worker);
    - `byroredux/src/cell_loader/references/import.rs:106-107` (the synchronous loader on the main thread);
    - `byroredux/src/scene/nif_loader.rs:328` (loose NIF).
- **Status**: Regression of #1269 (closed, labelled low). This is an incomplete fix, in the same shape as #3503 / #3237.
  - #1269's title covers "NIF scene-graph and collision walkers". Its SIBLING checklist said "currently two — hierarchical and flat", but the three satellite walkers already existed (#156, #401, #718).
  - The fix it shipped, `3d1307d55`, is a depth counter. A depth counter bounds depth, not fan-out.
  - Only `resolve_shape` got a `visited` set.
- **Description**:
  - `NiNode.children` is a list of raw on-disk `BlockRef`s. Neither the parser nor `NifScene` checks that the graph is a tree, so a child ref can name the node itself or an ancestor. Every walker recurses on every child with no "already walked" set.
  - The three satellite walkers have no bound at all, so a single self-reference recurses forever.
  - `walk_node_hierarchical` and `walk_node_flat` stop at depth 128. But a node that lists itself **twice**, or a diamond chain of shared children with no cycle at all, makes them visit 2^128 paths. The hierarchical walker pushes one `ImportedNode` per visit.
- **Evidence**: the `cycle` scratch bin patches the root `NiNode` child refs of a real `Fallout - Meshes.bsa` NIF to `0`. `parse_nif` accepts the patched bytes. Each entry point then runs on a 2 MiB thread, which is rayon's default worker stack, inside `catch_unwind(AssertUnwindSafe(..))`, exactly as `parse_one_nif` wraps it. Logs: `/tmp/audit/nif/cycle_proof.log` and `cycle_fan2_proof.log`.

  | Entry point | 1 self-child | 2 self-children |
  |---|---|---|
  | `import_nif_lights` | `thread has overflowed its stack` / `fatal runtime error: stack overflow, aborting` | stack overflow abort (measured) |
  | `import_nif_particle_emitters` | stack overflow abort | stack overflow abort (measured) |
  | `import_nif_texture_effects` | stack overflow abort | stack overflow abort (measured) |
  | `import_nif_scene` (hierarchical) | returns, 129 duplicate nodes | `memory allocation of 48 bytes failed`, rc 134 after 3.9 s under `ulimit -v 2 GiB`. Without the limit it consumes host RAM. |
  | `import_nif` (flat) | returns | still running at the 30 s timeout (2^129 visits) |

- **Impact**:
  - **One malformed NIF referenced by any streamed exterior REFR crashes the engine.** `parse_one_nif` calls `import_nif_lights` at `pre_parse.rs:297`, before any capped walker runs. The #854 design ("a single parser-level panic would tear down the worker…") assumes every failure unwinds, and a stack overflow does not. The same NIF crashes the synchronous interior loader and the loose-NIF viewer on the main thread.
  - If only the satellites were given a depth cap, the fan-out shape would still wedge the worker. The rayon `in_place_scope_fifo` in `parse_nif_pipeline` would never join, so exterior streaming would stop for the session, and the shutdown join would time out (see CONC-D7-2026-10-09-01).
  - Vanilla content is unaffected. The `multiparent` census found 0 multi-parent and 0 self-referenced children in 128,377 NIFs and 399,033 `NiNode` blocks across FNV, Oblivion, SSE, FO4 and Starfield Meshes01 (`/tmp/audit/nif/multiparent.log`).
- **Related**: #1269, #854, #4148, #3503. CONC-D7-2026-10-09-01 covers the shutdown that does not stop a wedged worker.
- **Suggested Fix**:
  1. Thread one per-walk `visited` bitset (`Vec<bool>` sized `scene.blocks.len()`) through all five walkers, and skip any block that has already been walked. The census shows vanilla scene graphs are strict trees, so "each block at most once" changes no vanilla import. This one bitset fixes both the cycle and the fan-out.
  2. Keep `MAX_NIF_NODE_DEPTH` as a stack bound and add it to the three satellites.
  3. Extend `recursion_depth_tests` with self-child and two-self-children scenes for all five entry points.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
