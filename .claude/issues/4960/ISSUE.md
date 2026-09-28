# #4960: REN-D5-2026-09-27-01: Nothing pins that the in-place compaction actually runs; its fallback is silent, so a layout drift would quietly bring back the 150–210 ms stall

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4960
- **Labels**: low,renderer,memory,test-gap,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D5-2026-09-27-01**._

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh/geometry_ssbo.rs` (`MeshRegistry::plan_geometry_compaction`, the `in_place` predicate and the `else` allocating branch; tests `in_place_compaction_moves_each_survivor_with_its_bytes`, `out_of_order_layout_falls_back_to_the_allocating_copy`)
- **Status**: NEW
- **Description**: `7e9da5dcc` keeps the allocating copy as a fallback for any layout that is not ascending and disjoint in slot order. The commit says the two branches produce identical pools and offsets, so a byte-for-byte test cannot tell them apart.
  - `in_place_compaction_moves_each_survivor_with_its_bytes` asserts payloads and lengths only. It passes unchanged if `in_place` is hard-wired to `false`.
  - The `else` branch emits no log or counter.
  - The invariant it depends on holds today: `upload_scene_meshes_batched` and `accumulate_global_geometry` append geometry and push slots in lockstep. A future upload path that reserves slots out of order would silently reinstate the ~400 MiB double allocation and its page-fault stall, and nothing would fail.
- **Evidence**: The #2678 test `repeat_compaction_without_a_new_drop_does_not_recopy` already pins pointer stability for the no-hole case (`reg.pending_vertices.as_ptr()`). The new in-place test takes no such snapshot across a real compaction.
- **Impact**: Perf regression with no signal. No correctness impact.
- **Related**: #2678 (the same pointer-stability technique), `7e9da5dcc`.
- **Suggested Fix**: In the in-place test, snapshot `pending_vertices.as_ptr()` / `pending_indices.as_ptr()` before the plan and assert they are unchanged after it. Add a once-per-session `log::warn!` (or a scratch-telemetry counter) on the allocating branch.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
