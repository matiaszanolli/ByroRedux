# #5513: ECS-2026-10-09-D3-01: #5384's subtree adoption walk has no visited set or traversal guard; so do two loading-cover walks

**Labels**: bug, ecs, low, safety

**Source**: `docs/audits/AUDIT_ECS_2026-10-09.md` — finding `ECS-2026-10-09-D3-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. A `Children` cycle needs corrupt hierarchy state, and no current producer creates one. This is defence in depth, and the same class as the closed #4572 (LOW).
- **Dimension**: 3 — Hierarchy walks
- **Location**:
  - `byroredux/src/systems/cinematic.rs:688-700` (`retry_cinematic_readoption`, the adoption subtree)
  - `byroredux/src/loading_screen.rs:570-587` (`collect_stage_mesh_handles`)
  - `byroredux/src/loading_screen.rs:594-607` (`collect_subtree`, which feeds `despawn_subtree` and `retire_stage`)
- **Status**: NEW. The cinematic walk landed in `faf8e5682` (#5384). The loading-screen pair predates the baseline (`e60911864`, 10-02) and has not been reported.
- **Description**: The project rule, after #3700, #4572 and #4769, is a visited set plus a `HierarchyTraversalGuard` budget on every Parent/Children walk:
  - `bounds.rs`, `anim_convert.rs`, `ragdoll.rs` and `npc_spawn/resumable/runtime.rs:1310` (`subtree_has_skinned_mesh`) pair the two.
  - `loot_appearance.rs:106` (`subtree_entities_under`) has a visited set.
  - The three walks above push every child id onto a stack with no visited set. In the adoption walk the other walks *in the same function* do dedupe, but this one does not. On a cyclic `Children` graph it never terminates, and it grows `subtree` without bound until the process runs out of memory. A child listed under two parents is visited twice. Every duplicate is then `insert`ed and pushed into `CellRootIndex` again; `drain_cell_victims` dedupes the index entries later, so those cost extra work, not corruption.
- **Evidence**:
  ```rust
  // cinematic.rs:689-700
  let mut subtree = vec![entity];
  let mut stack = vec![entity];
  while let Some(node) = stack.pop() {
      if let Some(row) = children.as_ref().and_then(|c| c.get(node)) {
          for &child in row.0.iter() {
              if !player_subtree.contains(&child) { subtree.push(child); stack.push(child); }
  ```
- **Impact**: A corrupted hierarchy hangs the streaming step (adoption), or the loading-cover setup or retirement, with no diagnostic. That is exactly the failure mode #3700 removed elsewhere.
- **Related**: #3700, #4572, #4769 (all closed, same class). CONC-D5-2026-10-09-01 and SAVE-D4-2026-10-09-01: their fix also needs a "collect the whole subtree" step.
- **Suggested Fix**: Add one shared guarded subtree collector, for example `collect_subtree(world, root) -> Vec<EntityId>` beside `HierarchyTraversalGuard` in `crates/core/src/ecs/hierarchy.rs`, with a visited set and a guard budget. Route these three walks through it, along with the six other hand-rolled ones (`unload.rs:19-49`, `cinematic.rs:503/526/648`, `loot_appearance.rs:106`). The purge fix for CONC-D5 can then hand `release_entities` a correct closure.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
