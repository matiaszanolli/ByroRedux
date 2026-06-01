# PERF-D6-NEW-02: animation_system allocates 3 collect() Vecs per frame (not scratch-reused)

**Severity**: MEDIUM · **Dimension**: CPU Allocations (PERF-D6-NEW-02)
**Location**: `byroredux/src/systems/animation.rs:382` (entities_with_players), `:394` (playback_states), `:527` (stack_entities)
**Status**: NEW

The stack inner loop reuses scratch (#828) and NameIndex is refilled in place (#824), but the three top-of-phase entity-list collections are `collect()`-ed fresh every frame whenever any AnimationPlayer/AnimationStack exists (every animated-NPC cell) → 3 heap allocs/frame, freed and re-collected next frame. The collect-then-drop-lock pattern is needed (can't hold the query lock across apply) but the *buffer* can persist.

**Fix**: promote the three Vecs to closure-captured scratch (mirroring make_world_bound_propagation_system), clear()+refill each frame. Behavior unchanged (covered by animation_system_e2e_tests); alloc reduction needs dhat (not wired).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
