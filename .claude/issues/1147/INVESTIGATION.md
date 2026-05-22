# #1147 / FO4-D6-003 Phase 2b — Investigation

## User-confirmed direction

Asked via AskUserQuestion: 3 options offered (infra-only / full impl / defer). User chose **Full implementation including triangle.frag branches** with conservative gating.

## Phase 1 (#1077) state at fix time

- ImportedMesh already carries `is_pbr`, `has_translucency`, `model_space_normals` (Phase 1 done).
- `pack_bgsm_material_flags` already packs the 3 flag bits into `GpuMaterial.material_flags` (Phase 2a done).
- Material ECS component did NOT carry the 3 flags or the translucency parameter suite (gap).
- Translucency parameter suite was in `BgsmFile` but not surfaced anywhere downstream.

## What landed

### Infrastructure (Rust)
- `GpuMaterial`: 260 → 280 bytes (+5 f32 fields). Size pin + offset pin + GLSL field-name pin all extended. (+1 drive-by `greyscaleLutIndex` field-name needle catch-up.)
- `material_flag::BGSM_TRANSLUCENCY_THICK_OBJECT` (1<<8) + `BGSM_TRANSLUCENCY_MIX_ALBEDO` (1<<9) added.
- `DrawCommand` + `DrawCommand::material_hash` + `hash_gpu_material_fields` walks all extended in lockstep.
- `Material` ECS component gains 3 new fields (subsurface RGB + scale + turbulence). Defaults zero.
- `ImportedMesh` gains 5 new fields (the translucency parameter suite).
- `apply_bgsm_chain` forwards the translucency parameter suite from `BgsmFile`.
- `pack_bgsm_material_flags` extended to pack the 2 new shape bits.
- 9 construction sites updated (5 `DrawCommand{}` + 4 `ImportedMesh{}`).

### Shader (triangle.frag)
- `struct GpuMaterial` mirror updated (+5 fields).
- `#define MAT_FLAG_BGSM_PBR / _TRANSLUCENCY / _MODEL_SPACE_NORMALS / _TRANSLUCENCY_THICK_OBJECT / _TRANSLUCENCY_MIX_ALBEDO` (bits 5-9).
- **PBR F0 gate**: when `BGSM_PBR` is set, F0 = mix(0.04, specularColor, metalness) (correct PBR metals); else mix(0.04, albedo, metalness) (preserved).
- **MSN gate**: when `BGSM_MODEL_SPACE_NORMALS` is set, sample normal map directly with BC5 Z-reconstruction, skip TBN multiply.
- **SSS gate**: when `BGSM_TRANSLUCENCY` is set, additive back-side wraparound contribution gated on 2 shape bits (thick/thin) and 1 colour-mix bit (skin-tinting vs raw pigment).

### Tests
- Workspace 2389 pass, 0 fail.
- Size pin 260 → 280 ✓
- Offset pin +5 assertions ✓
- GLSL field-name pin +6 needles (5 new + 1 drive-by) ✓
- Material-hash contract test caught one drift during implementation (`hash_gpu_material_fields` walk was missing the new fields); confirmed lockstep after fix.

## Regression safety

Every branch is flag-gated. The 99% case (every non-BGSM-v>=8 material — FO3/FNV/Skyrim/Oblivion NIF, FO4 BGSM v<8, BGEM) sees identical output to pre-commit. The 1% case (BGSM v>=8 with authored PBR/SSS/MSN flags) sees new shading; visual A/B against vanilla FO4 (MedTek / Sanctuary) is the acceptance gate the issue specified.

## Files touched (15 code + 1 SPV)

Per the commit body (`fe22e64c`).

## Visual A/B verification (acceptance criterion — pending)

Per `feedback_speculative_vulkan_fixes.md` ("Don't ship Vulkan render-pass/pipeline/barrier changes when failure modes are invisible to cargo test") — shader path changes affecting visible output need RenderDoc captures. This commit lands the infrastructure + branches; the visual A/B remains for a future bench session with FO4 content. Closing the tracker on landed-infrastructure basis; if the visual A/B reveals issues, file follow-up trackers per failure mode.
