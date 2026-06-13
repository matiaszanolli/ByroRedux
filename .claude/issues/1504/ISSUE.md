## Finding REN2-19 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Disney BSDF / PBR Gating (doc-rot)
- **Location**: `byroredux/src/cell_loader.rs:201-202` (the explicit false claim: "Drives the spec-glossiness F0 derivation in the fragment shader"); related loose phrasing at `crates/renderer/src/vulkan/material.rs:500-528`; ground truth at `crates/renderer/src/shader_constants_data.rs:157-165`
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab` (grep `BGSM_AUTHORED` in `crates/renderer/shaders/` → zero hits; note `cell_loader.rs` is a real 17 KB file sibling to the `cell_loader/` dir — cited lines live there, no re-pointing needed).

## Description

`BGSM_AUTHORED`'s translation is entirely CPU-side; the bit is telemetry-only ("rides through for debug-server inspection only", `shader_constants_data.rs:157-165` — deliberately NOT mirrored to GLSL; the shader is format-agnostic and doesn't branch on material provenance). The `cell_loader.rs` doc describes a fragment-shader spec-glossiness F0 branch that never existed.

## Suggested Fix

Rewrite the `cell_loader.rs:201-202` doc to match the `shader_constants_data.rs` ground truth (CPU-side translation, telemetry-only flag); tighten the `material.rs:500-528` phrasing while there. Per-game semantics stay at the parse→Material boundary (see `/audit-nifal`).

## Completeness Checks
- [ ] **SIBLING**: Grep for other docs claiming shader branches on provenance flags
- [ ] **CANONICAL-BOUNDARY**: Confirm the doc fix reasserts that per-game logic stays at the NIFAL parser→`Material` boundary — never in shaders
- [ ] **TESTS**: N/A (doc-only)

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
