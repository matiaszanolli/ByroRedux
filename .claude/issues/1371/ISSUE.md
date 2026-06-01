# PERF-D6-NEW-01: take_dirty() reallocates dirty-set Vec every frame (capacity not reused; doc comment wrong)

**Severity**: MEDIUM · **Dimension**: CPU Allocations (PERF-D6-NEW-01)
**Location**: `crates/core/src/ecs/packed.rs:56-58` (`take_dirty`); consumers `crates/core/src/ecs/systems.rs:87` (transform prop) + `byroredux/src/systems/bounds.rs:55-60` (bound prop)
**Status**: NEW

`take_dirty()` is `std::mem::take(&mut self.dirty)` which swaps in a **zero-capacity** Vec — the next `mark_dirty` re-grows from 0 (0→1→2→4→…→N) every frame ≥1 entity moves. 2 heap allocs/frame on motion (transform + global dirty sets). **The doc comment claims "the backing capacity is reused next frame" — factually wrong** (mem::take hands the capacity to the caller, dropped at end of frame). Steady-state static cell: zero (empty Vec take is a no-op).

**Fix**: add `drain_dirty_into(&mut self, out: &mut Vec<EntityId>) { out.clear(); out.append(&mut self.dirty); }` (`Vec::append` empties self.dirty keeping its capacity) + let bounds.rs/systems.rs own a persistent `dirty_scratch` reused across frames (bounds already owns dirty_roots/post_order/stack this way). **Correct the doc comment regardless.** Capacity-retention is unit-testable today; allocs/frame reduction needs dhat.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
