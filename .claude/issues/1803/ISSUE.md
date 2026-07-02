# PERF-D1-NEW-03: emit_particles acquires GlobalTransform and performs a dead per-emitter probe every frame

**Issue**: #1803
**Labels**: low,renderer,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D1-NEW-03)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D1-NEW-03)

## Location
`byroredux/src/render/particles.rs:48-55`

## Description
`emit_particles` (`render/particles.rs:37`) hard-requires a `GlobalTransform` query (`:48-53`) and then, per emitter per frame, executes a discarded `let _ = gtq.get(entity);` at `:55` with a comment claiming the transform is "sampled by the system at spawn." `gtq` is used nowhere else in the function (verified: only two references, the query bind and the discarded probe) — `emit_particles` reads particle world positions directly from `em.particles.positions`.

## Evidence
`particles.rs:48-53` query bind; `:55` `let _ = gtq.get(entity);` — sole other use.

## Impact
Micro (emitter counts are small); primarily misleading dead code + a wasted per-emitter SparseSet get. No quantitative guard exists for this site.

## Related
`particle_system` (`byroredux/src/systems/particle.rs:317-325`) is the real transform consumer (there the `get` is not dead).

## Suggested Fix
Delete the `gtq` acquisition and the dead probe; take only the `ParticleEmitter` query.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

