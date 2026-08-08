# TD3-208: gpu_instance_layout_tests.rs's own GpuMaterial field-order test doc comments still quote the pre-#1657-era 300 B size

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2415
**Finding ID**: TD3-208 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 3 — Stale Documentation & Comments
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:939` and `:990`
**Status**: NEW

## Description
The authoritative pin for `GpuMaterial`'s current size is `gpu_material_size_is_348_bytes` (`crates/renderer/src/vulkan/material.rs:1271-1272`, `assert_eq!(std::mem::size_of::<GpuMaterial>(), 348)`) — the struct grew 300→348 B when twelve common supplemental texture-role indices landed (`1d94eb24`, 2026-07-27). But the doc comment on the sibling `gpu_material_glsl_field_order_matches_rust_struct` test — in the *same file* that carries the size pin — still describes a hypothetical metalness/roughness reorder as one that "preserves the 300 B size." A prior fix (`2aded28e`, #2308) fixed the same class of drift in `docs/engine/renderer.md` but did not touch this file.

## Evidence
```
939: /// `roughness`) that preserves the 300 B size — the shader would then
990:             keeps the 300 B size but corrupts every lit-surface read — see #1657 / SF-D8-01.",
```
vs. `material.rs:1271-1272`: `assert_eq!(std::mem::size_of::<GpuMaterial>(), 348);`

## Impact
No runtime effect — the test is byte-agnostic (checks field *order*, not size) and remains correct/passing. Purely misleads a future reader of the exact file whose job is to be the lockstep-drift reference. Same file cluster as the renderer audit's `gpu_types.rs:84` and `constants.rs:168` findings, which caught the same stale-size class at three other sites in the same cluster.

## Related
#2308 (CLOSED, `renderer.md` copy of this class), #2222 (CLOSED).

## Suggested Fix
Update both doc comment sites to 348 B, or drop the literal number entirely and say "the current pinned size." If sibling `gpu_types.rs:84`/`constants.rs:168` stale-size findings are filed separately (from `AUDIT_RENDERER_2026-08-07.md`), fix all in one pass since they're the same drift class in the same file cluster.

## Age
Comment dates to 2026-06-18 (`78743032`), went stale 2026-07-27 (`1d94eb24`) — 11 days.

## Completeness Checks
- [ ] **SIBLING**: Check `gpu_types.rs:84` and `constants.rs:168` for the same stale 300 B reference (flagged separately by the renderer audit)
- [ ] **TESTS**: No test change needed — this is comment-only; confirm the test itself still asserts correctly after the comment edit
