# 2160: PERF-D4-01: Particle draws collide with real entity IDs in the new rigid motion-history map, corrupting motion vectors

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2160
**Labels**: bug, high, performance

---

## Severity
HIGH

## Dimension
SSBO Sizing & Per-Frame Upload (Dim 4) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/draw.rs:1491-1501` (history read/write); producer `byroredux/src/render/particles.rs:190-194`

## Description
Commit `33d9a468` ("preserve rigid instance motion history") added a per-frame `previous_rigid_models`/`current_rigid_models_scratch` map keyed on `DrawCommand::entity_id`. Particle draws synthesize that field as `entity ^ (i as u32)` — the producer's own comment says this is only ever meant as a *sort tiebreaker* ("Deterministic tiebreaker for same-emitter particles sharing depth bucket and color"), not an identity. XOR-ing a small particle index into a dense sequential ECS entity ID routinely lands inside the live-entity ID range: emitter entity 500, particle `i = 1` produces key 501, which collides with any static-mesh entity 501 also drawn this frame (`static_meshes.rs:648` uses the raw, un-XORed `entity_id: entity`).

Confirmed against current code: `particles.rs:194` — `entity_id: entity ^ (i as u32)`; `draw.rs:1491-1501` reads/writes `previous_rigid_models`/`current_rigid_models` keyed on `draw_cmd.entity_id`.

## Evidence
```rust
// draw.rs:1491-1501
let previous_source = if draw_cmd.bone_offset == 0 && !camera_cut {
    self.previous_rigid_models.get(&draw_cmd.entity_id).unwrap_or(m)
} else { m };
previous_models.push(rebase_model_matrix(previous_source, render_origin));
if draw_cmd.bone_offset == 0 {
    current_rigid_models.insert(draw_cmd.entity_id, *m);
}
```
```rust
// particles.rs:190-194
// Deterministic tiebreaker for same-emitter particles sharing depth bucket and color.
// XOR keeps the emitter grouping intact while giving each particle its own ordering slot.
entity_id: entity ^ (i as u32),
```
Particles set `bone_offset: 0` (`particles.rs:144`), so every particle draw takes the rigid branch unconditionally. Note this check is itself gated by `!camera_cut` (the sibling PERF-D9-NEW-01 issue) — on a frame where `camera_cut` misfires, this particular bug is masked because the whole map is bypassed, but that is an accident of the other bug's presence, not a fix.

## Impact
A colliding static surface reads a billboard's previous-frame transform as its own, producing a large bogus screen-space motion vector for that surface. This vector feeds `GpuPreviousModel` -> `triangle.vert` -> the motion-vector G-buffer attachment -> FSR 3.1 (the shipped default upscaler), TAA, and SVGF reprojection. Blast radius is any cell with particle emitters (torches, fires, steam, dust — i.e. nearly every FNV/FO3/Skyrim interior). Symptom is smearing/ghosting on scattered static geometry that shifts as particles are born and die. Per `_audit-severity.md`, "SVGF reprojection using wrong motion vectors" is a HIGH minimum. This is a plausible new contributor to the open ghosting investigation (memory: renderer_ghosting_investigation_open), though that investigation predates `33d9a468` and is not explained away by it.

## Related
Introduced by `33d9a468`; `surface_id: draw_cmd.entity_id.wrapping_add(1)` (`draw.rs:1653`, introduced by `883f57cd`) inherits the same collision but is largely masked because particles carry `ALPHA_BLEND_NO_HISTORY`. PERF-D9-NEW-01 (same loop, different bug, filed separately).

## Suggested Fix
Stop overloading the sort tiebreaker as a temporal identity. Either give particles a reserved ID namespace that provably cannot alias a real ECS entity (e.g. `PARTICLE_ID_BASE | (emitter << 16) | i`), or — simpler — skip the history insert/lookup entirely for draws with `INSTANCE_FLAG_ALPHA_BLEND` set, since billboards get no temporal benefit from motion-history reuse anyway. Add a test pinning that no two `DrawCommand`s in one frame share an `entity_id`.

## Completeness Checks
- [ ] **TESTS**: A test pins that no two `DrawCommand`s in one frame share an `entity_id`
- [ ] **SIBLING**: The `surface_id` `wrapping_add(1)` collision at `draw.rs:1653` checked for the same fix pattern
