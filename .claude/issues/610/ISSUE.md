# D4-NEW-02: TexDesc.clamp_mode parsed but discarded — all samplers default REPEAT, decals bleed at edges

## Finding: D4-NEW-02

- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`
- **Game Affected**: Oblivion, FO3, FNV
- **Location**: [crates/nif/src/blocks/properties.rs](crates/nif/src/blocks/properties.rs) (TexDesc.clamp_mode parsed), [crates/nif/src/import/material.rs:543](crates/nif/src/import/material.rs#L543) (only texture_index + uv_set carried over)

## Description

`TexDesc.clamp_mode` is parsed for every texture descriptor — an enum: `WRAP_S_WRAP_T`, `WRAP_S_CLAMP_T`, `CLAMP_S_WRAP_T`, `CLAMP_S_CLAMP_T`. The importer keeps only `texture_index` and `uv_set`. The renderer creates a single sampler per texture format with hardcoded `VK_SAMPLER_ADDRESS_MODE_REPEAT`. Meshes that author clamp-on-edge for decals or skybox seams render with repeating bleed.

## Impact

Edge bleeding on decals, weapon scope reticles, some Oblivion architecture trim, and pre-shader skybox quads. Visible artifact but limited surface area.

## Suggested Fix

1. Promote `TexDesc.clamp_mode` to `MaterialInfo::texture_clamp_modes: [u8; N]` (one per slot).
2. Forward to `Material` component.
3. Renderer side: cache samplers per `(format, clamp_mode)` pair; pick the right one when binding the texture descriptor set.

## Related

- #219 (open): NiTexturingProperty per-slot UV transform discarded — adjacent same-struct field.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: UV transforms (#219) live on the same `TexDesc`. If both addressed, single PR.
- [ ] **DROP**: Sampler-cache cleanup on swapchain teardown if samplers bound to swapchain lifetime.
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Render-output diff on a clamp-authored decal — verify no edge bleed.

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`._
