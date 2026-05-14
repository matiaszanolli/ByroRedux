# Issue #1028

**Title**: R-D6-01: triangle.vert CameraUBO block omits trailing skyTint vec4 field — latent footgun

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — R-D6-01
**Severity**: LOW (latent footgun, no current corruption)
**File**: `crates/renderer/shaders/triangle.vert:56-65`

## Premise verified (current `main`)

- Rust `GpuCamera` (`scene_buffer.rs:344-380`) is 9× `[f32;4]` after the 3 mat4s → 288 B total: `view_proj`, `prev_view_proj`, `inv_view_proj`, `position`, `flags`, `screen`, `fog`, `jitter`, `sky_tint`.
- `triangle.frag:149-165` declares all 9 vec4s including `skyTint` → matches Rust.
- `triangle.vert:56-65` declares only 8 vec4s — terminates at `jitter`; `skyTint` omitted.

## Issue

Today vert reads only `viewProj` / `prevViewProj` / `jitter`, so the trailing absence is benign. But the next contributor adding e.g. a `skyTint`-driven vertex effect (sun-disc billboard, atmospheric scatter blend at the vertex stage) will silently get garbage / OOB-read because the block declaration is incomplete.

## Fix

Add `vec4 skyTint;` as the 9th field in `triangle.vert`'s `CameraUBO` block. Mechanical, no behavioural change.

## Completeness Checks
- [ ] **SIBLING**: Verify `composite.frag` and `caustic_splat.comp` CameraUBO blocks (any consumer that re-declares the struct)
- [ ] **TESTS**: GpuCamera size-pin test already covers Rust side; consider a CI grep that asserts shader-side struct field count matches Rust

