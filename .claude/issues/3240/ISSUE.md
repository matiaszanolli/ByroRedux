# 3240: SAFE-D6: bindings.glsl GpuMaterial comment still cites retired 348B / gpu_material_size_is_348_bytes

**Severity**: LOW · **Dimension**: Safety Dimension 6 (R1 Material Table Layout Soundness) · **Report**: `docs/audits/AUDIT_SAFETY_2026-08-23.md` (SAFE-D6-NEW-01)

## Description

The doc comment directly above `struct GpuMaterial` in the canonical shader-contract header (`crates/renderer/shaders/include/bindings.glsl:99,107-108`) says `Mirrors the Rust GpuMaterial (348 B std430)` and `the size of this struct (348 B) is pinned by \`gpu_material_size_is_348_bytes\` on the Rust side`. The struct grew to 364 B under #2221 (animated `shader_color_r/g/b`/`shader_float` sinks) and the Rust-side test was renamed to `gpu_material_size_is_364_bytes` in lockstep — but this comment, in the one file every future field-adder is told to keep in sync, was not updated. Not covered by open #2483, which lists only `gpu_types.rs:84` and `constants.rs:168` as the untracked sites of this drift class (see the sibling issue for those).

## Evidence

```
99: // Mirrors the Rust `GpuMaterial` (348 B std430) defined
107: // lockstep to the Rust `GpuMaterial` struct + the matching
108: // `intern`/encoding sites; the size of this struct (348 B) is pinned by
109: // `gpu_material_size_is_348_bytes` on the Rust side.
```
vs. `crates/renderer/src/vulkan/material.rs:1421`: `fn gpu_material_size_is_364_bytes()` / `assert_eq!(..., 364)`.

## Impact

No runtime effect — prose, not code the compiler or shader reads. But it's exactly the class of drift `_audit-common.md`'s Path-Reference Convention singles out as non-decorative: a backticked, now-nonexistent test name in the one header comment a future contributor reads before adding a `GpuMaterial` field. Misleads anyone doing byte-math off this file (e.g. computing MaterialBuffer VRAM) by ~4.6%.

## Suggested Fix

Update both occurrences to `364 B`/`gpu_material_size_is_364_bytes`, or drop the literal number and say "the current pinned size" to reduce future re-drift.

## Completeness Checks
- [ ] **SIBLING**: Fix alongside #2483's two sites in the same pass
