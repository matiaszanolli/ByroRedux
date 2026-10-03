# #5215 — REN-D8-2026-10-03-02: `combustion_occupancy_buffers` depends on the all-slots fence wait, but the `sync.rs` rider list does not name it; its previous-slot RAW is covered only by an unrelated barrier

**Labels**: low,renderer,sync,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs` (`VolumetricsPipeline::dispatch`: `self.combustion_occupancy_buffers[frame].write_mapped(..)`; the Stage B `history_barriers` array); `crates/renderer/src/vulkan/sync.rs` (the "Bumping this constant still requires:" rider list and `frames_in_flight_contract_names_every_dependent_resource`); `crates/renderer/src/vulkan/context/post_passes.rs` (`record_volumetrics_pass`, the "Compute→compute visibility" `memory_barrier`)
- **Status**: NEW (same class as #4988 and #4851)
- **Description**: Frame N, slot `f`, host-zeroes and seeds `combustion_occupancy_buffers[f]`. That buffer was last read on the GPU by frame N‑1 (slot `1-f`) through binding 25, so slot `f`'s own fence does not retire that read. Only the top-of-frame all-slots wait does. The field doc and the commit body both say so ("the all-slots fence wait at the top of `draw_frame` has retired the previous reader"). The resource is still missing from the `sync.rs` list that the #4601/#3643 rule calls load-bearing and that #5117 relies on before any wait narrowing.
  - The read-after-write half has a similar gap. Frame N‑1's atomicOr marks are read by frame N's inject through binding 25. Stage B publishes the previous slot's WRITE→READ for all five per-FIF history images, but includes no buffer barrier for this mask.
  - That RAW is currently ordered only by the global COMPUTE `SHADER_WRITE` → COMPUTE `SHADER_READ` `memory_barrier` in `record_volumetrics_pass`. That barrier exists for cluster_cull's buffers. Its first scope happens to reach the previous submission's inject.
  - The Stage F `SHADER_WRITE → HOST_READ` barrier carries an explicit comment on why a fence alone does not give a memory dependency. The occupancy mask has no equivalent stated dependency.
- **Evidence**: The `history_barriers` array lists `lighting_volumes`, `emission_history_volumes`, `combustion_state_volumes`, `combustion_dynamics_volumes` and `combustion_optical_volumes`, with no buffer barrier. `grep -n occupancy crates/renderer/src/vulkan/sync.rs` returns nothing.
- **Impact**: Nothing breaks today. If the cluster-cull barrier is narrowed to a buffer barrier, the RAW loses its only ordering. If the fence wait is narrowed per slot (#5117), the host write races frame N‑1's read of the mask.
  - Either change can produce a false-negative occupancy bit. A false negative skips the RK2 transport block where combustion actually sits, so a plume freezes in place.
  - `cargo test` cannot see either failure.
- **Related**: #5117, #4988, #4851, #4601; REN-D5-2026-10-03-04.
- **Suggested Fix**: Add the occupancy mask to the `sync.rs` rider list and its pinning test. Name the previous-slot mask in Stage B's barrier set, or document there that the cluster-cull barrier covers it. Stop at that: per the skill, any barrier edit is "needs syncval" (below).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
