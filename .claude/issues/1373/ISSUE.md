# PERF-D9-NEW-02: distant-terrain LOD ring not wired into per-frame streaming — stale + teleport leak (Slice 2)

**Severity**: MEDIUM (streaming correctness + latent VRAM growth) · **Dimension**: World Streaming (PERF-D9-NEW-02)
**Location**: `byroredux/src/main.rs:948-1085` (`step_streaming` — no LOD call), `byroredux/src/scene/world_setup.rs:753` (only call site), `byroredux/src/cell_loader/terrain_lod.rs:16-18` (Slice-2 TODO)
**Status**: NEW (known/pending "Slice 2")

The LOD ring is built once in `stream_initial_radius` and never re-evaluated. As the player walks, the ring stays anchored to the spawn block → the high-detail hole-out region stays centred on the old spawn cell, so near terrain can z-fight/gap against a stale ring. Any re-entry to `stream_initial_radius` (M40 scripted teleport) re-spawns a fresh ~600-block ring with **no unload of the prior one** (LOD blocks are bare entities, untracked in `state.loaded`, never reclaimed).

**Fix (Slice 2)**: track LOD blocks by block-coord in `HashMap<(i32,i32), LodBlock>` on `WorldStreamingState`; in `step_streaming`, on cell-boundary crossing recompute the desired block set around the new player block, load/unload the delta, and regenerate boundary blocks whose hole pattern changed (16-bit hole mask). Frees the teleport leak too.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
