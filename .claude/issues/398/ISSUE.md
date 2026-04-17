# OBL-D4-H1: NiZBufferProperty z_test/z_write extracted but dropped before GPU pipeline

**Issue**: #398 — https://github.com/matiaszanolli/ByroRedux/issues/398
**Labels**: bug, renderer, high, pipeline

---

## Finding

`NiZBufferProperty.z_test` and `.z_write` survive the full extraction pipeline into `ImportedMesh.z_test / z_write` at `crates/nif/src/import/mesh.rs:160-161`, but **nothing downstream reads them**.

- `rg "z_test|z_write" crates/renderer byroredux` → zero hits.
- `GpuInstance` at `crates/renderer/src/vulkan/scene_buffer.rs:44-93` has no depth-state fields.
- Pipeline creation at `crates/renderer/src/vulkan/pipeline.rs:236-237,269-270` hardcodes `depth_test_enabled = true` (except UI/composite passes).
- `z_function` (the comparison operator) isn't even pulled from the block at `material.rs:339-341` — we only extract the two booleans.

Previous audit claimed FIXED but the fix stopped at `MaterialInfo` — no actual GPU state flows from the parsed value.

## Impact on Oblivion

- **Sky domes, first-person viewmodels, ghost overlays, HUD markers, billboarded particles** all author `z_write=0` (sometimes `z_test=0`) to force a specific draw order. Currently they z-fight adjacent geometry or z-clip through world meshes.
- Glow/fade planes (enchantment FX halos) also rely on `z_write=0`.

## Fix

1. Extend `GpuInstance` with `depth_state: u8` (packed `z_test | z_write | z_func`), OR add depth as a pipeline cache key.
2. Either:
   - (a) Use Vulkan dynamic state (`vkCmdSetDepthTestEnable`, `vkCmdSetDepthWriteEnable`, `vkCmdSetDepthCompareOp`) — `VK_EXT_extended_dynamic_state`, widely supported.
   - (b) Add pipeline variants to `PipelineSet` for each (z_test, z_write, z_func) combo.
3. Thread `z_function` extraction in `material.rs` — currently missing; default `LESS_EQUAL` is fine when unspecified.

Option (a) is the right shape for this kind of per-draw state.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Same story for OBL-D4-C1 (blend factors) — both need per-draw state machinery. Land them together.
- [ ] **DROP**: If pipeline cache expands, verify Drop tears all variants down.
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Render test with a sky-dome-style mesh authored `z_write=0` → no z-fighting against world geometry behind it.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 4 H4-01 (and L4-02 for z_function).
