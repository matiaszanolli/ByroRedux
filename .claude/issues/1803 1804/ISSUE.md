# #1803: PERF-D1-NEW-03: emit_particles acquires GlobalTransform and performs a dead per-emitter probe every frame

**Severity**: LOW
**Location**: `byroredux/src/render/particles.rs:48-55`

`emit_particles` hard-requires a `GlobalTransform` query and then, per emitter
per frame, executes a discarded `let _ = gtq.get(entity);` with a comment
claiming the transform is "sampled by the system at spawn." `gtq` is used
nowhere else — `emit_particles` reads particle world positions directly from
`em.particles.positions`.

## Fix
Deleted the `gtq` acquisition and the dead probe; the function now only
queries `ParticleEmitter`. Removed the now-unused `GlobalTransform` import.

## Completeness Checks
- [x] **SIBLING**: Grepped `byroredux/src/render/`, `systems.rs`, `crates/core/src/`
      for the same discarded-query-probe pattern — none found.
- [x] **TESTS**: N/A — pure dead-code removal, no behavior change to pin
      (existing `emit_particles` test coverage in `particles.rs` already
      exercises the function end-to-end and continues to pass).

---

# #1804: D2-NEW-03: Two-sided glass split runs on additive particle batches — 2x draws + a fully-culled vertex pass with zero compositing benefit

**Severity**: LOW
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1086-1088,1159-1172`;
`byroredux/src/render/particles.rs:96`

Particles emit `two_sided: true` + `alpha_blend: true`, so every particle
batch hits `needs_split = is_blend && two_sided` and dispatches twice
(FRONT-cull then BACK-cull), excluded from indirect grouping. The split
exists to stabilize TAA depth-winner flips on volumetric glass — a rationale
that requires depth writes + order-dependent compositing. Particles have
`z_write: false`, so the FRONT-cull pass rasterizes ~nothing while still
shading the whole instanced batch — dead work.

## Fix
Extracted the predicate into `needs_two_sided_blend_split(&DrawBatch) -> bool`
in `draw.rs`, narrowed to `is_blend && two_sided && z_write`. Verified
`z_write: false` is unique to particles across the whole codebase (grep
confirmed no other draw-command site sets it), so glass (which carries
`z_write: true`) is unaffected and still gets the split.

## Completeness Checks
- [x] **SIBLING**: Confirmed via grep that `particles.rs` is the only
      `z_write: false` site in the codebase — no other batch type is
      affected by narrowing the gate.
- [x] **TESTS**: Added `needs_two_sided_blend_split_tests` module in
      `draw.rs` pinning: splits when blended+two-sided+z_write; does NOT
      split when z_write is false (the particle case); does not split
      when single-sided; does not split when opaque.
