# 3246: D7-01: Animated material sinks hash raw per-frame float bits into dedup key with no quantization

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_RENDERER_2026-08-24.md` (D7-01)

## Description

`#2221` correctly wires `AnimatedAlpha`/color/shader fields into `DrawCommand` before `material_table.intern_by_hash` (the dedup key does see the animated value each frame — no correctness bug). What's missing is a cardinality bound: all seven fields hash as raw `f32::to_bits()` with no rounding, exactly the shape `quantize_fade`/`COLOR_FADE_STEPS` (`#1795`, `render/particles.rs`) was built to prevent ("N visually-near-identical continuous values, N `MaterialTable` slots"). Entities spawned in the same cell-load pass share phase-locked animation timing and dedup fine; the gap opens for instances of the same animated-material clip attaching on *different* frames — which the engine's own actively-developed exterior-streaming architecture produces routinely (props spawn as the player approaches, not all at once). The codebase already has an established fix pattern for exactly this class (`LightFlicker.phase_offset_secs`) that was not extended to material sinks.

## Location
- `byroredux/src/render/static_meshes.rs` (`collect_static_mesh_draws`, ~lines 705-775)
- `crates/renderer/src/vulkan/material.rs` (`hash_gpu_material_fields`)

## Evidence

```rust
// material.rs — no rounding before hash
h.write_u32(mat.material_alpha.to_bits());
h.write_u32(mat.shader_color_r.to_bits());
h.write_u32(mat.shader_float.to_bits());
```

## Impact

No visual corruption or crash — `MaterialTable` clears and rebuilds every frame, and the existing `MAX_MATERIALS` overflow-to-id-0 + warn-once path bounds worst case. A cell with many independently-phased animated-alpha/color props spends one `MaterialTable` slot per instance per frame instead of collapsing to one, directly working against the `#780` dedup-ratio telemetry this dimension exists to protect. Real-world magnitude unmeasured (no engine launch this pass).

## Related

`#1795` (sibling fix this diverges from), `#2221` (introduced the gap), the `LightFlicker` phase-offset precedent.

## Suggested Fix

Apply `quantize_fade`-style coarse quantization (32-64 steps) to `material_alpha`, the four color sinks, and `shader_float`/`shader_color` before hashing. Alternatively/additionally, stagger animation attach phase for streaming-spawned entities sharing a clip, mirroring `LightFlicker`.

## Completeness Checks
- [ ] **SIBLING**: Same quantization pattern applied consistently with `particles.rs`'s `quantize_fade`
- [ ] **TESTS**: A regression test asserting bounded distinct-hash count for phase-jittered animated instances
