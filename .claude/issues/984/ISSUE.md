# Issue #984

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/984
**Title**: NIF-D5-ORPHAN-A2: Wire NiPSys{Gravity,Vortex,Drag,Turbulence,Air,Radial}FieldModifier consumers — particles ignore authored force fields
**Labels**: bug, nif-parser, import-pipeline, medium
**Parent**: #974 (orphan-parse meta) / #869 (original instance)
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: #974 Band A — orphan-parse follow-up
**Severity**: MEDIUM (visible on smoke/dust/magic-FX particles in Skyrim+ / FO4 cells)
**Domain**: NIF import + Particle simulator + ECS particles

## Description

All six `NiPSys*FieldModifier` types are dispatched and parsed cleanly but never consumed by the import pipeline:

- `NiPSysGravityFieldModifier` — point-source gravity (well/source)
- `NiPSysVortexFieldModifier` — rotational force around an axis
- `NiPSysDragFieldModifier` — velocity-proportional damping
- `NiPSysTurbulenceFieldModifier` — pseudo-random per-particle force
- `NiPSysAirFieldModifier` — directional wind with falloff
- `NiPSysRadialFieldModifier` — radial push/pull

Dispatch arms in `crates/nif/src/blocks/mod.rs:899-907`. `ParticleEmitter` ECS component at `crates/core/src/ecs/components/particle.rs:147` has a single `gravity: [f32; 3]` field (uniform downward / buoyant force). The six field-modifier variants offer richer per-emitter force-field config that the simulator doesn't expose today.

The `torch_flame()` preset at `particle.rs:235` explicitly notes "the parsers currently discard the per-emitter fields (`NiPSysBlock` is opaque)" — same defect class.

## Impact (current behaviour)

Authored magic-FX vortices, dust whirlwinds, smoke plumes, plasma-weapon trails ignore their gravity/vortex/drag config. Particles use the `ParticleEmitter::torch_flame` preset (or a similar heuristic) regardless of what the NIF authored. Visible delta: cinematic spell effects look anemic vs vanilla; dust devils don't rotate.

## Suggested fix

1. **Component side** — add a `Vec<ParticleForceField>` (or fixed-array since most emitters carry ≤3 modifiers) field to `ParticleEmitter`. The `ParticleForceField` enum mirrors the six NIF types:

```rust
pub enum ParticleForceField {
    Gravity { center: [f32; 3], strength: f32, decay: f32 },
    Vortex { axis: [f32; 3], strength: f32, decay: f32 },
    Drag { strength: f32 },
    Turbulence { frequency: f32, scale: f32 },
    Air { direction: [f32; 3], strength: f32, falloff: f32 },
    Radial { center: [f32; 3], strength: f32, falloff: f32 },
}
```

2. **Importer side** — follow the `NiPSysModifier` list on each `NiParticleSystem`; for each matching modifier, `scene.get_as::<NiPSys*FieldModifier>(idx)` and extract the relevant scalars into `ParticleForceField`.

3. **Simulator side** — extend the particle simulator (currently does `particle.vel += emitter.gravity * dt`) to fold each force field into the per-particle velocity integration step per frame.

## Completeness Checks

- [ ] **SIBLING**: all six variants implemented in the same PR — they share a `NiPSysModifier` base and authoring usually pairs Gravity+Drag or Vortex+Air; partial wiring drops visible interactions
- [ ] **TESTS**: per-variant fixture — emit one particle with each field type, verify position/velocity at T+1s matches the analytical expectation
- [ ] **ECS**: `ParticleEmitter::torch_flame()` preset comment at `particle.rs:234` should be updated to remove the "parsers currently discard" claim
- [ ] **PERF**: per-particle force evaluation cost — measure on a 1024-particle emitter to confirm <0.1 ms / frame on the dev GPU/CPU baseline
- [ ] **DOC**: the cell-loader / scene-import path that builds `ParticleEmitter`s should comment-link this issue + the matching parser site so future audits see the wiring

## Source quote (audit report)

> Particle force fields parse cleanly but particles ignore gravity / vortex / turbulence.

`docs/audits/AUDIT_NIF_2026-05-12.md` § HIGH → NIF-D5-NEW-01 (orphan-parse meta).

Related: #974 (meta), #869 (original instance of this pattern).

