# TD3-207: inline ImageSubresourceRange literals in bloom + caustic (helper exists)

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-17.md` — Dimension 3 (Logic Duplication)

## Severity
**LOW** — mechanical migration; canonical helper already exists and is used by 5+ other Vulkan modules. Bloom + caustic are the outliers.

## Locations
- `crates/renderer/src/vulkan/bloom.rs:490` (1 site, reused 2×)
- `crates/renderer/src/vulkan/caustic.rs:726/768/800` (3 sites)

## Description
Both files build `vk::ImageSubresourceRange { aspect_mask: COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 }` as inline struct literals. The same struct is built via the helper `super::descriptors::color_subresource_single_mip()` at `descriptors.rs:99-107`, and TAA / swapchain / volumetrics / gbuffer all use the helper.

## Proposed Fix
Replace each inline literal with a call to `descriptors::color_subresource_single_mip()`. ~6 LOC saved per site × 4 sites.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify no other Vulkan modules added new inline literals between today and the fix landing
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Type system enforces (struct field set unchanged); no regression test needed
