# SK-D3-05: BSLightingShaderProperty MultiLayerParallax (11) and EyeEnvmap (16) variants stub at shader, but CPU pack still runs

**Severity:** MEDIUM (per-draw CPU waste; no visible artifact)
**Source:** `docs/audits/AUDIT_SKYRIM_2026-04-24.md`

## Problem
- Shader stubs at `crates/renderer/shaders/triangle.frag:762-778` for variants 11 (MultiLayerParallax) and 16 (EyeEnvmap).
- CPU pack runs unconditionally at `byroredux/src/render.rs:529-549` for every draw.
- GpuInstance defaults are neutral, so output unaffected — but pack cost burns ~56 bytes / instance every frame on every draw.

## Audit-prescribed short-term
Gate the pack behind `material_kind ∈ {11, 16}` so non-affected materials skip.

## SIBLING
Other stubbed branches in triangle.frag with the same dead-CPU-pack issue.
