# #4300: REN-2026-09-14-D5-01: `memory-budget.md` ledgers none of the three new GPU resource owners, and `sky_cube_bytes_per_frame` — documented as existing "so the memory budget can account for it" — has zero callers

- **Labels**: low,renderer,memory,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4300
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**:
  - `crates/renderer/src/vulkan/sky_cube.rs` (`sky_cube_bytes_per_frame`)
  - `crates/renderer/src/vulkan/groundcover.rs` (`GroundCoverPipeline::create_buffers`)
  - `crates/renderer/src/vulkan/cloud_noise.rs` (`CloudNoiseVolumes`)
  - `docs/engine/memory-budget.md` (VRAM Rough Budget table; "Not yet ledgered")
- **Status**: NEW
- **Description**:
  - `memory-budget.md` does not mention the SKYAL sky cube, the cloud noise volumes, or EXAL ground cover.
  - Its "Not yet ledgered" section lists only `StagingPool` retained capacity, and says: "A grep of this page for the owning subsystem name is the cheapest way to find a gap in it."
  - `sky_cube_bytes_per_frame`'s doc says it is "Exposed so the memory budget can account for it the way `SSAO_BYTES_PER_PIXEL` does". Its only reference is its own unit test.
  - The ground-cover blade buffer is the largest of these, a fixed 16 MiB device-local allocation that exists on every RT device whether or not the scene has ground cover.
- **Evidence** (sizes derived from code):
  - **Ground-cover blade buffer**: `GROUNDCOVER_MAX_CHUNKS` (256) × `GROUNDCOVER_MAX_BLADES_PER_CHUNK` (4096) × 16 B = 16,777,216 B.
  - **Other ground-cover buffers**:
    - interaction field: `INTERACTION_TEXEL_COUNT` (256²) × 2 × 4 B = 524,288 B
    - indirect buffer: 256 × 16 B = 4,096 B
    - per-FIF host-visible chunk / cell / species / table / state / disturber / readback buffers (small)
  - **Sky cube**: 6 × 128² × 8 B = 786,432 B per FIF × `MAX_FRAMES_IN_FLIGHT` = 1,572,864 B.
  - **Cloud noise**: 64³ + 32³ R8 = 288 KiB, per `cloud_noise.rs`'s module doc. The same doc notes `VolumetricsPipeline` "still uploads its own copy of the same texels".
  - `grep -rn sky_cube_bytes_per_frame` returns only the definition and `the_vram_figure_follows_the_face_size`.
  - `screen_scaled_reservation_bytes` (`crates/renderer/src/vulkan/acceleration/predicates.rs`) has no fixed-size term. That is consistent with its name, so the BLAS reservation is not wrong. The gap is the ledger.
- **Impact**: About 18.5 MiB of resident VRAM is invisible on the page cited as authoritative, and the page's own grep-for-subsystem recipe returns nothing for any of the three owners. No leak and no correctness risk.
- **Related**: #3566 (the mesh-side `StagingPool` ledger gap, fixed; same class); REN-2026-09-11-D5-01 / #4117 (dead telemetry accessor, same shape).
- **Suggested Fix**: Add ground cover, sky cube and cloud noise rows to the VRAM Rough Budget table, derived from the constants above. Then either cite `sky_cube_bytes_per_frame` from the doc or a test, or drop its "so the memory budget can account for it" claim.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
