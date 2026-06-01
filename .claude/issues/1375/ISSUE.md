# PERF-D4-NEW-01: Late-stage GlobalTransform writers leave bounds one frame stale (latent trap)

**Severity**: MEDIUM · **Dimension**: ECS Query Patterns (PERF-D4-NEW-01)
**Location**: `byroredux/src/systems/character.rs:426-433` (camera_follow), `byroredux/src/systems/audio.rs:300/337/397`; drain at `byroredux/src/systems/bounds.rs:55-60`
**Status**: NEW

`world_bound_propagation` drains GlobalTransform's dirty set in PostUpdate, while `camera_follow_system` + audio emitters re-arm it in Stage::Late (after the drain). Benign today (camera + audio emitters have no LocalBound → pass-1 skips them), but a latent correctness trap: the moment a Late-stage system writes GlobalTransform on a *bounded* entity, that entity's WorldBound silently lags one frame.

**Fix**: document+assert the invariant "no Late-stage system may write GlobalTransform on a LocalBound-bearing entity", OR move the bounds drain to Stage::Early of the next frame so it captures the prior frame's complete write set. Pin the PostUpdate ordering contract (propagation → billboard → bounds) in a main.rs comment block.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
