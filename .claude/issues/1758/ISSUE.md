# TD7-001: skin-shader workgroup size 64 hand-written in 3 places, bypasses generated-constants pipeline

_Filed 2026-06-26 as #1758 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1758` for live state)._

**Severity**: LOW · **Dimension**: 7 — Magic Numbers & Hardcoded Constants
**Location**: `crates/renderer/shaders/skin_vertices.comp:40`, `skin_palette.comp:36`, `crates/renderer/src/vulkan/skin_compute.rs:37`
**Status**: NEW · **Audit**: TD7-001

## Description
The skin-shader workgroup size `64` is a hand-written literal in 3 places, none of which is the canonical `crates/renderer/src/shader_constants_data.rs` single-source-of-truth. Every other compute shader sources `local_size_x` from the build-script-generated `WORKGROUP_X` / `THREADS_PER_CLUSTER` defines; the two skin shaders hard-write `64`, and the Rust dispatch (`skin_compute.rs:593,940`) carries its own `WORKGROUP_SIZE = 64`.

## Why LOW (not HIGH)
`skin_compute.rs:1242 skin_palette_workgroup_size_matches_skin_vertices` string-scans both GLSL sources for `local_size_x = {WORKGROUP_SIZE}` and fails the build on drift — so a divergence is caught at test time, not silently. The residual debt is purely architectural: the value bypasses the mandated `shader_constants_data.rs → build.rs → shader_constants.glsl` pipeline. (`WORKGROUP_X = 8` can't be reused — skinning is a 1D 64-wide dispatch.)

## Suggested Fix
Add `pub const SKIN_WORKGROUP_SIZE: u32 = 64;` to `shader_constants_data.rs`; emit `#define SKIN_WORKGROUP_SIZE 64` in `build.rs` (no `u` suffix — layout qualifier); change both shaders to `local_size_x = SKIN_WORKGROUP_SIZE`; re-export the generated const in `skin_compute.rs`. The existing string-scan test keeps passing and now transitively pins to the header.

## Completeness Checks
- [ ] **SIBLING**: no other shader literal bypasses the generated `shader_constants.glsl`
- [ ] **TESTS**: `skin_palette_workgroup_size_matches_skin_vertices` still green
