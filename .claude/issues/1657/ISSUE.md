# SF-D8-01: GLSL GpuMaterial field order has no automated cross-check against the Rust #[repr(C)] struct

**Issue**: #1657
**Source audit**: docs/audits/AUDIT_STARFIELD_2026-06-18.md (HEAD post `2aac5351`)
**Severity**: LOW (hardening; no current drift) · **Labels**: low, renderer, bug
**Dimension**: NIFAL canonical material translation (renderer shader-contract) — cross-link /audit-nifal
**Location**: `crates/renderer/src/vulkan/material.rs:1345`
(`gpu_material_field_offsets_match_shader_contract`, Rust-only) vs
`crates/renderer/shaders/include/bindings.glsl:61` (sole GLSL `struct GpuMaterial`);
GLSL-parsing tests at `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`

## Description
The GpuMaterial offset/size tests pin the Rust layout via `offset_of!`/`size_of` only —
self-referential, never parse the GLSL struct. The layout tests positively cross-check
only `GpuInstance` field order (`gpu_instance_layout_tests.rs:297`); for GpuMaterial they
assert only *absence* in `ui.vert`/`water.vert`. A within-vec4 reorder of the GLSL
`GpuMaterial` (size-preserving) would pass all `cargo test` and corrupt every lit-surface read.

## Evidence
- One GLSL `struct GpuMaterial` (`bindings.glsl:61`) + Rust struct (`material.rs:69`).
- `gpu_instance_layout_tests.rs:229-304` extracts only the `GpuInstance` body; no
  GpuMaterial field-order extraction/compare.
- Field-for-field match verified by hand this audit → latent gap, no active drift.

## Impact
Zero today; all-game blast radius if a future reorder regresses. LOW = latent guard gap.

## Suggested Fix
Add a GLSL-parsing test mirroring the `GpuInstance` one: extract `struct GpuMaterial { ... }`
from `include/bindings.glsl` and assert ordered field names match the Rust struct.

## Folded in — SF-D8-02 (INFO doc-rot, `218b425b` split)
Stale `triangle.frag:110-184` / "only triangle.frag mirrors GpuMaterial" references at
`material.rs:1340`, module-doc `material.rs:21-25`, and `gpu_instance_layout_tests.rs`
assert messages → repoint to `include/bindings.glsl` as part of the same fix.
(`material.rs:1235` already correct.)

## Related
Template: existing `GpuInstance` GLSL guard. Distinct from #1627.
