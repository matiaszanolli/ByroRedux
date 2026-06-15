# documentation, renderer, low

## REN-D3-DOC-02: material.rs hash/collision doc comments say "50 scalar fields"; GpuMaterial has 75

**Severity**: LOW
**Dimension**: GPU-Struct Layout
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW

## Description
`GpuMaterial` is 300 B = 75 scalar slots; the hash walk writes exactly 75 `h.write_u32` (full coverage). Two doc comments in `material.rs` still say "50 live scalar fields", predating the ior/subsurface/sheen/sheen_tint/anisotropic (#1248-#1250) + translucency (#1147) additions.

## Evidence
- `crates/renderer/src/vulkan/material.rs:772` — "Canonical material hash — FxHash (#1368) over the 50 live scalar ..." (and the `intern_by_hash` collision-policy doc).
- `awk` count of `h.write_u32` = 75; `pub <field>:` count = 75; `comm` diff empty both directions (zero missing/extra/dup).

## Impact
Doc-only; hash is correct/complete. A reader reasoning about collision probability from "50" has a wrong premise.

## Suggested Fix
Change both "50" → "every scalar field" (drop the hardcoded count so it can't drift again).

## Completeness Checks
- [ ] **SIBLING**: both the `hash_gpu_material_fields` doc and the `intern_by_hash` collision-policy doc are updated
