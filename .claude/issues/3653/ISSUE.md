# CONC-D4-2026-08-30-02: `submersion_system` (Late) sets `ParticleEmitter.rate`; its only consumer runs in PostUpdate

**Issue**: #3653
**Labels**: bug, ecs, low, water, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D4-2026-08-30-02 (LOW, D4 · Scheduler Access Declarations, cross-stage sequencing).

**Location**: `byroredux/src/boot.rs:1118` (`particle_system`, PostUpdate exclusive) + `byroredux/src/boot.rs:1390-1400` (`submersion_system`, Late exclusive); write site `byroredux/src/systems/water.rs:262-277`, read site `byroredux/src/systems/particle.rs:367-372`.

## Description

`submersion_system` writes `emitter.rate` for every water volume the camera disturbs; `particle_system` is the **sole consumer** of `rate` (it integrates the spawn accumulator from it).

`Stage::PostUpdate` (2) executes before `Stage::Late` (4), so the rate `particle_system` spawns against in frame N is the one `submersion_system` computed in frame **N-1**.

## Evidence

```rust
// byroredux/src/systems/water.rs:262-268   (submersion_system, Stage::Late exclusive)
if let Some((volume_q, mut emitter_q)) = world.query_2_mut::<WaterVolume, ParticleEmitter>() {
    for (entity, volume) in volume_q.iter() {
        if let Some(emitter) = emitter_q.get_mut(entity) {
            let previous = emitter.rate;
            let rate = disturbance_rate(cam_pos, volume);
            emitter.rate = rate;

// byroredux/src/boot.rs:1118              (particle_system, Stage::PostUpdate exclusive)
scheduler.add_exclusive(Stage::PostUpdate, particle_system);
```

## Verification Path

Hand-check of the stage table only. The KPIs are blind to it (`analyze_pair` is intra-stage), and `submersion_system`'s declaration is the **only one of the pair that names `ParticleEmitter` at all** — `particle_system` is a bare `add_exclusive`, so its side is an undeclared row (cf. #3473).

## Trigger Conditions

Any frame in which the camera enters, leaves, or moves within a `WaterVolume` that carries a `ParticleEmitter` — i.e. every ripple/splash disturbance emitter.

## Impact

One frame of latency on the water-disturbance spawn rate at the moment the player enters/leaves water. Cosmetic and sub-perceptual in steady state; no race and no correctness hazard.

**Pre-existing rather than a #3180 regression**: before that commit `submersion_system` was a PostUpdate exclusive registered *after* `particle_system` (`git show 5ce2b1c5^:byroredux/src/boot.rs`, lines 1057 vs 1221), so the inversion already existed within PostUpdate.

## Related

#3180 (moved `submersion_system` PostUpdate -> Late); CONC-D4-2026-08-30-01 (same class, larger blast radius); #3473 (bare `add_exclusive` leaving a blank `sys.accesses` row).

## Suggested Fix

Either accept and **document** the one-frame lag on the `submersion_system` registration comment, or move the disturbance-rate write out of `submersion_system` into a PostUpdate step that precedes `particle_system` — the write needs only `ActiveCamera` + `WaterVolume`, none of the Late-authored camera pose that forced #3180's move.

## Completeness Checks
- [ ] **SIBLING**: If CONC-D4-2026-08-30-01 is fixed by moving `camera_follow_system` into PostUpdate, re-derive this finding — the fix may resolve or relocate it
- [ ] **TESTS**: If the write moves, a cross-stage ordering pin, since `analyze_pair` cannot see it
