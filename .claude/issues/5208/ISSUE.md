# #5208 — REN-D5-2026-10-03-04: volumetrics' per-slot host-visible buffers, including #4784's new occupancy masks, have no memory-budget.md row

**Labels**: low,renderer,memory,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `VolumetricsPipeline` construction (`crates/renderer/src/vulkan/volumetrics/init.rs`): `fog_volume_buffers`, `fog_cluster_buffers`, `fog_cluster_index_buffers`, `combustion_light_moment_buffers`, `combustion_occupancy_buffers`. Ledger: `docs/engine/memory-budget.md` § "Volumetrics (M55)".
- **Status**: NEW.
- **Description**:
  - c705c310d (#4784) added a resource owner: `combustion_occupancy_buffers`, one `create_host_visible` buffer per frame-in-flight slot. Each is `FOG_VOLUME_CLUSTER_COUNT` (16³ = 4096) × 4 B = 16 KiB, so 32 KiB in total.
  - The commit edited memory-budget.md only for the `GpuLight` 80→64 B row. `grep -i occupancy` on the ledger finds nothing.
  - Checking the section showed the gap is older and larger. The Volumetrics section ledgers the six froxel volumes and the noise pair, but none of the per-slot host-visible SSBOs:
    - `fog_cluster_index_buffers`: 4096 × (`MAX_FOG_VOLUMES_PER_CLUSTER` 64 + `MAX_FOG_PORTALS_PER_CLUSTER` 128) × 4 B = 3 MiB per slot, 6 MiB in total. This is CpuToGpu memory, so it is BAR-eligible.
    - `fog_cluster_buffers`: 4096 × 16 B = 64 KiB per slot.
    - The fog-volume upload buffer.
    - The 8 KiB per slot moment readback.
- **Evidence**: `occupancy_buffer = try_or_cleanup!(GpuBuffer::create_host_visible(device, allocator, std::mem::size_of::<[u32; FOG_VOLUME_CLUSTER_COUNT]>() ...))`. memory-budget.md's volumetrics rows end at the volume table and the 294,912 B noise pair.
- **Impact**: Ledger completeness only. The skill rule is that every resource owner added since the baseline needs a row. The unledgered host-visible total (about 6.2 MiB of BAR-eligible memory, mostly the index lists) is invisible to anyone budgeting BAR pressure, which is the condition #4889 had to degrade around.
- **Suggested Fix**: Add a per-slot host-buffer sub-table to the Volumetrics section, with sizes derived from the named constants. Optionally pin it like `memory_budget_ledgers_the_sky_and_ground_cover_owners`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
