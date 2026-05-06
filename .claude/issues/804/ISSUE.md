# R1-N4 / #804 — GpuMaterial.avg_albedo populated but never read

**Severity:** LOW
**Domain:** renderer
**Audit:** `docs/audits/AUDIT_RENDERER_2026-05-03_R1.md`

## One-line
`GpuMaterial.avg_albedo_r/g/b` (offsets 144-152) populated by `to_gpu_material` for every material but no shader reads `mat.avgAlbedo*` — both consumers (`caustic_splat.comp`, `triangle.frag` GI miss) read from `GpuInstance.avgAlbedo*` instead.

## Fix
Remove the 3 fields from `GpuMaterial` (Rust + GLSL), drop the populate lines in `to_gpu_material`, update size test 272→260.
