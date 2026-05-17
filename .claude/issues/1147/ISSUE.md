# #1147 — FO4-D6-003 Phase 2: gate PBR / SSS / model-space-normals paths in triangle.frag (renderer-side wiring of #1077)

Labels: enhancement, renderer, medium
State: OPEN

## Description

Follow-up to #1077 Phase 1 (closed in $COMMIT). Phase 1 surfaced
three BGSM shader flags on `ImportedMesh` — `is_pbr`,
`has_translucency`, `model_space_normals` — but the renderer-side
consumer hasn't been wired yet. This issue tracks Phase 2: gating
the shading paths in `triangle.frag` based on the flags.

## Scope

Three independent branches to add in the fragment shader:

1. **PBR vs Gamebryo specular gate** — when `GpuMaterial.is_pbr`
   is set, route through the metalness/roughness pipeline; else
   the existing legacy specular path. Today every FO4 material
   renders through the legacy path regardless of authoring intent.
2. **Subsurface scattering** — when `GpuMaterial.has_translucency`
   is set, apply the SSS approximation. The parameter suite
   (`translucency_subsurface_color`, `translucency_transmissive_scale`,
   `translucency_turbulence`, `translucency_thick_object`,
   `translucency_mix_albedo_with_subsurface_color`) is parsed by
   the bgsm crate but not yet on `ImportedMesh`; this issue adds
   it through `ImportedMesh` → `Material` → `GpuMaterial` in
   lockstep with the shader work.
3. **Normal-space gate** — when `GpuMaterial.model_space_normals`
   is set, the fragment shader's `perturbNormal` skips the TBN
   transform and uses the sampled normal directly.

## Required infrastructure changes

- `GpuMaterial` (in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`)
  needs three new fields. Bool-equivalent via `u32` since shaders
  don't have a Bool type in std430. Check the
  `gpu_instance_is_112_bytes_std430_compatible` test pattern for
  the byte-layout invariant.
- `Material` upload path (`crates/renderer/src/vulkan/scene_buffer/upload.rs`)
  forwards the bits from the per-instance material.
- `triangle.frag` adds three `if` branches reading the new flags.
  Per the project's `feedback_shader_struct_sync.md` memory:
  `GpuMaterial` lives in multiple shaders — every shader that
  declares it must be updated in lockstep.

## Why deferred from #1077 Phase 1

The shader changes alter every FO4 material's visible appearance
in ways `cargo test` cannot validate. The project's
`feedback_speculative_vulkan_fixes.md` memory flags exactly this
pattern as needing RenderDoc validation or visual A/B diffs
against shipped FO4 content, not speculation. A separate
implementation pass with concrete before/after captures is the
right shape.

## Acceptance criteria

- [ ] `GpuMaterial` gains 3 u32 (bool-equivalent) fields.
- [ ] All shaders declaring `GpuMaterial` (per
  `feedback_shader_struct_sync.md`) updated in lockstep.
- [ ] `triangle.frag` gates the three shading paths.
- [ ] RenderDoc A/B captures vs vanilla FO4 content (MedTek or
  Sanctuary) confirming non-regression on legacy materials +
  visible PBR / SSS / object-space-normals improvement on
  authored-flag materials.
- [ ] `gpu_instance_is_112_bytes_std430_compatible` test
  pattern extended to cover the new `GpuMaterial` size.
- [ ] Translucency parameter suite plumbed through from
  `BgsmFile` (parser already has it).

## Source

Audit: `docs/audits/AUDIT_FO4_2026-05-15.md` § FO4-D6-003 (MEDIUM)
Phase 1 closure: #1077 / commit $COMMIT
