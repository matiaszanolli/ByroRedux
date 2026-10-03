# #5203 — REN-D3-2026-10-03-01: #5055 shrank `GpuLight` to 64 B, but four texts still describe the 80 B identity-carrying struct, including the `NoUninit` SAFETY comment, which names a test that no longer exists

**Labels**: low,renderer,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: the stale texts are listed under Evidence.
- **Status**: NEW. This absorbs Dim 2's REN-D2-2026-10-03-02, which found the same `renderer.md` site; that ID is retired into this one. #5055 (CLOSED) is the change itself. #4952 (CLOSED) was the previous size-drift on the same struct. No open issue covers these sites.
- **Description**:
  - c705c310d moved the ReSTIR remap identity off the GPU struct and updated `gpu_types.rs`, the size pin (`gpu_light_is_80_bytes` → `gpu_light_is_64_bytes`), `shader-pipeline.md` and the `memory-budget.md` light row.
  - Four other texts still state the old layout:
    - Two are prose.
    - One is the SAFETY justification of an `unsafe impl`. It cites "80 B total", "one `[u32; 4]` identity" and the renamed test as the pin holding the layout fixed.
  - The invariant it relies on still holds: four `[f32; 4]`, no padding. The justification text and its named guard are wrong.
- **Evidence**:
  - `docs/engine/renderer.md` § Multi-light SSBO: "Each `GpuLight` is an 80-byte struct of four `vec4`s and a `uvec4` identity … `history_id` identifies the producer across light animation and priority reordering; zero declines selection reuse."
  - `MAX_LIGHTS` rustdoc in `crates/renderer/src/vulkan/scene_buffer/constants.rs`: "1023 lights × 80 bytes plus the 4112-byte remap header is about 84 KiB". The live figure is 1023 × 64 + 4112 = 69 584 B ≈ 68 KiB.
  - `hash_light_upload` rustdoc in `crates/renderer/src/vulkan/scene_buffer/descriptors.rs`: "`GpuLight` is `#[repr(C)]` with four `[f32; 4]` fields, one `[u32; 4]` identity, and no implicit padding."
  - SAFETY comment on `unsafe impl crate::vulkan::buffer::NoUninit for super::gpu_types::GpuLight` (same file): "four `[f32; 4]` fields and one `[u32; 4]` identity (16 B each, 80 B total) … `gpu_light_is_80_bytes` (in `gpu_instance_layout_tests.rs`) holds the layout fixed." `grep -rn "fn gpu_light_is_80_bytes" crates/` finds nothing.
- **Impact**: No runtime effect. A reviewer checking the `NoUninit` justification follows it to a test that does not exist. renderer.md tells readers the identity is GPU-resident, which is the design #5055 removed; its rustdoc says "Do not re-add identity data here".
- **Related**: #5172 (OPEN, the same class of drift for `GpuTerrainTile` 160 → 176 in the same window). Stale skill premises below (Dim 3 Guard line, Dim 10 `history_id` bullet).
- **Suggested Fix**:
  - Rewrite the four texts for the 64 B / four-`vec4` struct and name `gpu_light_is_64_bytes`.
  - In renderer.md, say the identity rides `FrameInputs.light_ids`, CPU-only.
  - The window produced two Gpu* size drifts and both left prose behind. Consider extending the `gpu_material_size_claims` scanner (which #4952 already suggested) to every struct with a size pin (`GpuLight`, `GpuTerrainTile`, `GpuInstance`, `GpuCamera`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
