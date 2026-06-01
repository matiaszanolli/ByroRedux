# PERF-D4-NEW-04: animate_lights_system per-frame Vec alloc + 3-pass lock cycling

**Severity**: LOW · **Dimension**: ECS Query Patterns (PERF-D4-NEW-04)
**Location**: `byroredux/src/systems/light_anim.rs:76-188`
**Status**: NEW

The system reads LightFlicker+LightSource into a fresh `Vec<LightUpdate>` (`Vec::new()` per frame — the per-frame-alloc pattern), then re-acquires LightSource write + Transform write in two more passes. As an exclusive Stage::Update system there is no concurrent-writer reason to split read-then-write. LOW because flickering-light counts are small (dozens).

**Fix**: hoist the `Vec<LightUpdate>` into closure-captured scratch (clear() per frame), or collapse pass 1+2 — hold `query::<LightFlicker>` (read) + `query_mut::<LightSource>` (write) simultaneously (distinct storages) and write intensity in place, dropping the intermediate Vec. Transform pass stays separate (third storage, currently dead — jitter disabled).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
