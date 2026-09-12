# REN-2026-09-11-D3-01/D7-01: GpuMaterial doc/comment sites still say 432 B after #3909's 432→428 shrink

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4114
**Labels**: documentation, renderer, low, doc-rot

**Severity**: LOW
**Dimension**: GPU-Struct Layout / Material Table (renderer report D3-01/D7-01); Dimension 6 - R1 Material Table Layout Soundness (safety report SAFE-D6-2026-09-11-01)
**Location**: `crates/renderer/src/vulkan/material.rs` (struct top-doc lines 43,48,49, plus inline sites at 392, 1047, 1321, 1363, 1380), `crates/renderer/src/vulkan/material_tests.rs` (lines 56, 1284), `crates/renderer/src/vulkan/scene_buffer/constants.rs:174,177`, `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:2214`, `byroredux/src/material_translate.rs:104,107`, `byroredux/src/render/static_meshes.rs:1108`, `.claude/commands/audit-safety/SKILL.md:261,269,282`, `.claude/commands/_audit-common.md:101`, and `docs/engine/material-abstraction.md`, `docs/engine/memory-budget.md`, `docs/engine/nifal.md`, `docs/engine/launcher.md`, `docs/engine/shader-pipeline.md`, `docs/engine/renderer.md`
**Status**: NEW — reported independently by two audits the same day (`docs/audits/AUDIT_RENDERER_2026-09-11.md` as REN-2026-09-11-D3-01/D7-01, and `docs/audits/AUDIT_SAFETY_2026-09-11.md` as SAFE-D6-2026-09-11-01); filed as one issue since both describe the identical defect

## Description
`a65dbffe` (Fix #3909, 2026-09-07) removed `GpuMaterial.texture_index`, correctly shrinking the struct from 432 B to 428 B and correctly renaming/updating the pinning test (`gpu_material_size_is_428_bytes`) and one nearby "Shader Struct Sync" paragraph. It did not reach the struct's own top-of-file doc comment (`material.rs`, a few lines above the corrected paragraph) or the 15+ other files that cite the old figure — several of which (`material.rs:1047/1321/1363/1380`, `material_translate.rs`) narrate the struct's entire growth history and simply stop one step short of the real total. `material.rs` now contradicts itself within 30 lines, and `material_tests.rs`'s own doc comment sits two lines above the assertion it contradicts. The struct's own last-field comment carries an internally-inconsistent sum (`// offset 424 → total 432`, i.e. 424 + 4 ≠ 432). This is the third or fourth recurrence of this exact doc-rot class for this exact struct (`#3846`, `#3414`/`#3240`). Notably, two of the stale sites are the audit-skill files themselves (`_audit-common.md`, `audit-safety/SKILL.md`), meaning the audit process that exists to catch this class was primed with the wrong number.

## Evidence
Confirmed live: `grep -rn "fn gpu_material_size_is_432_bytes"` → zero hits anywhere in the tree; only `gpu_material_size_is_428_bytes` exists (`material_tests.rs:62-64`) and asserts `size_of::<GpuMaterial>() == 428`. Re-derived field count independently: 107 `pub` fields × 4 B = 428 B, matching the live test. The GLSL mirror (`crates/renderer/shaders/include/bindings.glsl`) has exactly 107 matching scalar fields — the Rust↔GLSL layout is in lockstep; this is not a runtime layout drift, only a stale documentation trail. `grep -rn "432" crates/renderer/src/vulkan/material.rs crates/renderer/src/vulkan/material_tests.rs crates/renderer/src/vulkan/scene_buffer/constants.rs crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs byroredux/src/material_translate.rs byroredux/src/render/static_meshes.rs` confirms all cited stale sites still present at HEAD.

## Impact
Documentation only — `hash_gpu_material_fields`, `DrawCommand::material_hash`, and every offset/size pin are internally consistent at 428 B; no pixel or dedup-key behavior is affected. Cost is to the next reader/contributor, and to the next (eighth) size change, which will very likely repeat this at the same sites absent a structural fix. Two of the nine/sixteen stale sites are the audit-skill files themselves, so future audit runs are primed with the wrong number.

## Related
#3909, #3846, #3414/#3240 (prior recurrences of this exact class), #1321/#1522/#1624/#3869/#4042 (the parallel `classify_pbr` doc-rot recurrence, same shape different symbol)

## Suggested Fix
Mechanical find/replace "432 B"/`432`/`gpu_material_size_is_432_bytes` → "428 B"/`428`/`gpu_material_size_is_428_bytes` at all sites listed (including `_audit-common.md` and `audit-safety/SKILL.md`), and fix `material.rs:392`'s `→ total 432` to `→ total 428`. Given this is the third+ sweep for this one struct, consider either (a) a `collect_stale_gpu_material_size_claims` source-scanning test (mirroring `#4042`'s `collect_live_classify_pbr_claims` for the `classify_pbr` recurrence) that fails the build if a `4\d\d B` figure near "GpuMaterial" doesn't match `size_of::<GpuMaterial>()`, or (b) have `build.rs` emit a `GPU_MATERIAL_SIZE_BYTES` constant into the generated `shader_constants.glsl` and assert the Rust-side pin against that constant instead of a hand-typed literal.

## Completeness Checks
- [ ] **TESTS**: A structural guard test (source-scanning or build-time constant) prevents an eighth recurrence of this exact drift
