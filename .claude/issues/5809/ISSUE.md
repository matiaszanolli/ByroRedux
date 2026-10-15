# TD2-2026-10-15-08: Fifteen clamp-to-edge point/linear samplers are hand-built, each an 8-line `SamplerCreateInfo` chain plus `try_or_cleanup!` and `.context(...)`

Labels: low,renderer,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-08`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/renderer/src/vulkan/ssao.rs:165,179`, `presentation.rs:265`, `caustic.rs:427`, `sky_cube.rs:289`, `svgf.rs:536`, `taa.rs:269,283`, `composite.rs:481,502`, `bloom.rs:306`, `volumetrics/init.rs:281,302,322`, `exposure_meter.rs:81`, `cloud_noise.rs:124` (repeat); `context/helpers.rs:611` and `texture_registry/mod.rs:1396` are legitimately different
- **Status**: NEW
- **Age**: the sibling of #5593 (c6d36b4e7, 2026-10-14) which consolidated layout bindings, UBO creation and cleanup but left samplers
- **Effort**: small
- **Description**: clones of `vk::SamplerCreateInfo::default().mag_filter(F).min_filter(F).address_mode_u/v/w(CLAMP_TO_EDGE)` with `F` in {NEAREST, LINEAR} and an optional `.mipmap_mode(NEAREST|LINEAR)` / `.max_lod(..)`; `taa.rs:269` and `:283` are two copies in the same function differing in `F` only. There is no `descriptors::` sampler builder to mirror `layout_binding`/`write_*`.
- **Evidence**: `grep -rn 'create_sampler(' crates/renderer/src` -> 20 sites in 16 files; the 15 listed differ only in filter and mip mode.
- **Impact**: a sampler policy change (anisotropy off for point samplers, a debug-name, a new `unnormalized_coordinates` rule) is a 15-site edit; low blast radius, high boilerplate.
- **Related**: #5593 (same family and guard `passes_build_layouts_writes_and_teardown_through_the_shared_helpers`).
- **Suggested Fix**: `pub(crate) unsafe fn clamp_sampler(device, filter: vk::Filter, mip: vk::SamplerMipmapMode) -> Result<vk::Sampler>` in `descriptors.rs`; extend the #5593 shape guard to flag a new bare `create_sampler(` outside it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
