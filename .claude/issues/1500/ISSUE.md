## Finding REN2-15 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `byroredux/src/material_translate.rs:180` (`pub(crate) const NORMAL_ALPHA_SPEC_BIT: u32 = 0x8000_0000;`, consumed at `byroredux/src/render/static_meshes.rs:416`) vs `crates/renderer/shaders/triangle.frag:1924-1925` (`(mat.glossMapIndex & 0x80000000u)` hardcoded)
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

The gloss-in-alpha flag bit value is duplicated literally across the Rust↔GLSL boundary and rides outside the generated-header contract that protects `MAT_FLAG_*`/`DBG_*` — neither `shader_constants.rs` nor `shader_constants_data.rs` mention `NORMAL_ALPHA_SPEC`. A value flip on either side would compile silently and break gloss extraction.

## Suggested Fix

Route it through `shader_constants_data.rs` like its siblings (generated `#define` + lockstep pin), then recompile `triangle.frag.spv` (plain `-V`).

## Completeness Checks
- [ ] **SIBLING**: Grep shaders for other hardcoded bit literals that bypass the generated-header contract
- [ ] **TESTS**: Covered by `generated_header_contains_all_defines` once routed (verify the new define is in its pinned set, cf. open #1482)

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
