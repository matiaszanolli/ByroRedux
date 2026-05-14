# Issue #1023

**Title**: REN-D20-NEW-01: sunAngularRadius is a hardcoded shader constant, not a UBO field — blocks per-cell tuning

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D20-NEW-01
**Severity**: MEDIUM
**File**: `crates/renderer/shaders/triangle.frag:2386`

## Issue

`sunAngularRadius` is declared as `const float = 0.020;` in triangle.frag rather than living in a UBO. No matching slot in `GpuCamera` (`scene_buffer.rs:346`); zero hits in `render.rs` for `sun_angular`/`0.020`/`0.0047`. Value is correct (`0.020` per M-LIGHT v1 bump from `0.0047`) but cannot be tuned per-cell or per-TOD without a shader recompile. PCSS-lite future work flagged in the shader comment will require plumbing it through anyway.

## Fix

Add `sunAngularRadius: f32` slot to `GpuCamera` (Rust), wire from SkyParamsRes / weather TOD, replace shader const with UBO read. Lockstep update across `render.rs` (assembly), `scene_buffer.rs` (struct), `triangle.frag` (consume).

## Completeness Checks
- [ ] **SIBLING**: Update GpuCamera Rust struct + triangle.vert + triangle.frag + size pin test
- [ ] **TESTS**: Pin GpuCamera new size; integration test asserting different per-cell values produce different penumbra widths

