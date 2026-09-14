# #4344 — TD2-002: SKYAL's `cloud_noise.rs` re-implements volumetrics' R8 density-noise upload; both hand-roll the TRANSFER_DST→SHADER_READ publish barrier

**Labels**: low, renderer, vulkan, terrain-exterior, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4344

- **Severity**: LOW · **Dimension**: 2
- **Location**: `crates/renderer/src/vulkan/cloud_noise.rs:82-244` (barrier `:209-218`), `crates/renderer/src/vulkan/volumetrics/init.rs:880-903`, `:1003-1070` (barrier `:1047-1055`); helper `crates/renderer/src/vulkan/descriptors.rs:449-466` · **Status**: NEW · **Age**: `564d0d2fe` (09-13) · **Effort**: small · **Kind**: tech-debt
- **Finding**: Same memoized payloads, extents, `R8_UNORM` format and staging/copy sequence. The VRAM images are also allocated twice.
- **Suggested Fix**: Add `volumetrics::noise::record_density_noise_upload` + `create_density_noise_staging` using `image_barrier_transfer_dst_to_shader_read`. Follow-up: have volumetrics sample `CloudNoiseVolumes` instead of owning a second pair.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
