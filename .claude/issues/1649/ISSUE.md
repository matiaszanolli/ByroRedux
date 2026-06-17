# D2-NEW-01: Particle billboards never instance-batch; additive emitters drop free instancing

**Issue**: #1649
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-16.md`
**Severity**: MEDIUM · **Dimension**: Draw & Instancing
**Labels**: medium, performance, renderer, enhancement

## Location
- `byroredux/src/render/particles.rs:81-144` (per-particle `DrawCommand` build; `alpha_blend: true` at :87, `sort_depth` at :144; stale module doc at :6-9)
- `byroredux/src/render/mod.rs:195-209` (transparent sort-key branch; `!sort_depth` slot 6 before `mesh_handle` slot 8)
- `crates/renderer/src/vulkan/context/draw.rs:2034` (batch-merge SSBO-contiguity requirement)
- `crates/core/src/ecs/components/particle.rs:279/295/329/364/401/428` (preset `max_particles` + `dst_blend` defaults)

## Description
Every particle billboard is emitted with `alpha_blend: true` and a per-particle `sort_depth`. The transparent sort-key branch orders by `!sort_depth` before `mesh_handle`, so same-emitter-mesh particles depth-interleave with all other transparent draws. Batch-merge requires contiguous same-mesh entries, so each particle (up to 256/emitter) becomes its own draw call. The module doc claiming instanced collapse (#272) is false for the transparent path. Most vanilla presets are additive (`dst_blend: 1` / ONE), order-independent, and could be mesh-sorted + instanced for free.

## Impact
Emitter-heavy scenes emit up to N×256 draw calls where N×1 would suffice for the additive subset. Scales with on-screen particle count, not entity count. No correctness impact (additive is order-independent).

## Suggested Fix
For `dst_blend == ONE`, sort by `mesh_handle` before `sort_depth` so same-mesh particles stay contiguous and batch-merge collapses them into one instanced indirect draw. Leave true alpha-over (`ONE_MINUS_SRC_ALPHA`, smoke preset at `particle.rs:364`) on the depth-sorted path. Update the `particles.rs:6-9` module doc. Pure CPU-side ordering — no Vulkan state change.

## Related
- Sort-key design in `render/mod.rs:187`; opaque batch-merge in `draw.rs`; instancing from #272.
