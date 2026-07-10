# REN-D11-01: triangle.vert.spv compiled to SPIR-V 1.5 while every sibling shader is SPIR-V 1.0

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1929

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/triangle.vert.spv` vs the documented build command in CLAUDE.md (Shader Compilation) and `docs/engine/shader-pipeline.md`
**Status**: NEW

## Description
The committed `triangle.vert.spv` is compiled to SPIR-V 1.5, while every sibling shader (`triangle.frag.spv`, `ui.vert.spv`, `ui.frag.spv`, `composite.frag.spv`) is SPIR-V 1.0. The documented recompile command `glslangValidator -V triangle.vert -o triangle.vert.spv` emits SPIR-V 1.0 (a byte-different, ~712-byte-smaller output). Whoever last built the vert used an extra version-targeting flag (e.g. `--target-env vulkan1.2`) not captured in the docs, so anyone following the documented command produces a byte-different binary. Independently, `triangle.frag.spv` uses ray queries yet is stamped SPIR-V 1.0 — a version/extension-capability mismatch tolerated by the current driver but technically ill-formed. (This finding was independently corroborated by the Tangent-Space dimension of the same audit.)

## Evidence
`spirv-dis triangle.vert.spv` → "Version: 1.5"; all other `*.spv` → "Version: 1.0". Source + binary last committed together (source unchanged since); `diff` of `OpDecorate` I/O semantics between committed and fresh recompile → identical (semantic I/O is not affected).

## Impact
None functional — SPIR-V 1.5 loads fine on the Vulkan 1.3 target and the vertex-input/descriptor contract is byte-identical. Cost is a reproducibility/churn hazard: the documented build command silently regresses the file to 1.0 if followed literally, and the frag's SPIR-V-1.0-with-ray-query stamp is a latent portability concern on stricter drivers.

## Related
Memory note `feedback_triangle_frag_spv_recompile`

## Suggested Fix
Recompile `triangle.vert.spv` with the same plain `-V` invocation as its siblings for uniformity, or document the `--target-env` flag in CLAUDE.md if 1.5 is intentional. Runtime impact on stricter drivers needs RenderDoc/driver verification before any version bump.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
