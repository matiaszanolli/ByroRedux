# Issue #3447: REN-2026-08-27-D3-01 (regression of #3201/#2483): shader-pipeline.md still documents GpuInstance at 128 B and GpuCamera at 352 B, and memory-budget.md understates the Instance SSBO by 25%

**Filed**: 2026-08-27 via /audit-publish from `docs/audits/AUDIT_RENDERER_2026-08-27.md`

**Severity**: MEDIUM
**Dimension**: GPU-Struct Layout (doc lockstep)
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-27.md` (REN-2026-08-27-D3-01)
**Status as reported**: Regression of #3201 / #2483 (both CLOSED) — same defect class, new growths. `GpuInstance` 128 → 160 B landed 2026-08-23 (#3231) and was **not** caught by the 2026-08-24 sweep's own D3-01, which examined only `GpuMaterial`; `GpuCamera` 352 → 368 B landed with #3323 after that sweep.

## Location
- `docs/engine/shader-pipeline.md:193` (`GpuCamera` heading — "352 bytes")
- `docs/engine/shader-pipeline.md:211` (table's last row is `render_debug` at offset 336; no `exterior_sky_tint` row at 352)
- `docs/engine/shader-pipeline.md:248` (`GpuInstance` heading — "128 bytes")
- `docs/engine/shader-pipeline.md:268` (table's last row is `_reserved` at offset 120; no morph rows at 128/136/144)
- `docs/engine/shader-pipeline.md:427`
- `docs/engine/memory-budget.md:31` (Instance SSBO row), `:36` (Camera UBO row)
- `docs/engine/renderer.md:271`, `:531`, `:582`
- `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:335` (`"**352 bytes**"`), `:342` (growth history stops at 352), `:131`, `:137` (`"current 128 B"`)
- `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:50` (`"must stay 352 B … plus ten"` — it is eleven now), `:130` (`"struct still exactly 128 B"`)
- `crates/renderer/src/vulkan/context/mod.rs:810`
- `.claude/commands/audit-renderer/SKILL.md:115`

## Description
The Rust structs, their GLSL mirrors and all layout-pin tests are correct. The documentation the project's own audit protocol designates authoritative for these byte layouts is not. `docs/engine/renderer.md:582` additionally names two tests that no longer exist (*gpu_instance_is_128_bytes_std430_compatible*, *gpu_camera_is_352_bytes*); the live pins are `gpu_instance_is_160_bytes_std430_compatible` and `gpu_camera_is_368_bytes`.

## Evidence
`size_of::<GpuInstance>() == 160` and `size_of::<GpuCamera>() == 368` are asserted at `gpu_instance_layout_tests.rs:30` and `:66`, both green. Against that:

```
docs/engine/memory-budget.md:31
| Instance SSBO | `MAX_INSTANCES` = 262 144 | 262 144 | 128 B (#2219) | 33.6 MB | **67.1 MB** |
docs/engine/memory-budget.md:36
| Camera UBO | — | 1 | 352 B | 352 B | **704 B** |
```
The correct figures at 160 B are 41.9 MB / 83.9 MB, so the row understates the second-largest scene-buffer allocation by ~16.8 MB.

`gpu_types.rs:335` still opens the struct with *"GPU-side camera data (**352 bytes**, std140-compatible)"* two lines above a doc line that correctly names `gpu_camera_is_368_bytes`.

## Impact
No runtime effect — every allocation sizes from `size_of::<T>()`. The damage is that an auditor or implementer following `_audit-common.md`'s instruction to *"audit against those docs"* is handed wrong numbers, and that the VRAM budget doc under-reports the real ceiling. This is the fifth iteration of the same manual-fix cycle (#2222 → #2308 → #2415 → #2483/#3201 → here).

## Related
#3201, #2483, #3197, `feedback_shader_struct_sync.md`. Distinct from #3414 (`material_translate.rs`'s stale 348-byte `GpuMaterial`), which covers a different struct and site.

## Suggested Fix
Update the sites, add the three `GpuInstance` morph rows and the `GpuCamera.exterior_sky_tint` row, and rename the two dead test citations. Given the recurrence count, the durable fix is the one #3197 already argued for: a doc-glob regression check that greps `docs/engine/*.md` + `.claude/commands/*.md` for size literals and dead `gpu_*_is_*_bytes` test names and diffs them against `size_of::<T>()`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all `GpuMaterial` / `GpuLight` / `GpuInstance` doc mirrors, `.claude/commands/*.md`)
- [ ] **TESTS**: A regression test pins this specific fix (doc-glob size-literal check)
