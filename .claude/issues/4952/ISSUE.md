# #4952: REN-D3-2026-09-27-01: `memory-budget.md` Light SSBO row still bills 64 B lights and a 16-byte header after the 80 B / 4112 B change

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4952
- **Labels**: low,renderer,memory,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D3-2026-09-27-01**._

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `docs/engine/memory-budget.md:94` (Scene-Buffers table, "Light SSBO" row); `docs/engine/shader-pipeline.md:606` and `:663` (Set 1 binding 0 and volumetrics binding 3 rows: "`u32 count` + `GpuLight[]`")
- **Status**: NEW
- **Description**: `186234944` grew `GpuLight` to 80 B and added a 4112-byte header (`LightHeader`: count, 3 pads and `previous_to_current: [u32; MAX_LIGHTS + 1]`). `buffers.rs` sizes each slot as `size_of::<LightHeader>() + size_of::<GpuLight>() * MAX_LIGHTS`, which is 85,952 B. The memory-budget row still says an entry size of 64 B, 64 KB per frame and **128 KB** total. The real total is about 86 KB per frame and 172 KB for 2 FIF.
  - `shader-pipeline.md`'s dedicated `GpuLight` section was updated correctly (80 B, header described).
  - The two descriptor-table rows still describe the buffer as `u32 count` + `GpuLight[]`, which omits the 4 KiB remap array between them.
  - The Set 1 binding 0 row's "Used by: triangle, cluster_cull" also omits `caustic_splat`, `volumetrics_inject` and `water.frag`, which all read it.
  - The code is right; the docs are wrong.
- **Evidence**: `constants.rs` doc says "1023 lights × 80 bytes plus the 4112-byte remap header is about 84 KiB". `memory-budget.md:94` says `| 1023 | 64 B | 64 KB | **128 KB** |`.
- **Impact**: The budget ledger is under-billed by about 44 KB. Anyone laying out the light buffer from the binding table would place `lights[]` at offset 16 instead of 4112.
- **Related**: #3565 (the previous Light-SSBO row drift), #4872 (other ledger drift, open).
- **Suggested Fix**: Update the row to 80 B plus a 4112 B header, 86 KB per frame and 172 KB total. Say "count + 1024-entry previous→current remap + `GpuLight[]`" in both descriptor tables. Consider extending the `gpu_material_size_claims` scanner pattern to `GpuLight`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
