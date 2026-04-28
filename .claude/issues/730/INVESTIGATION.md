# INVESTIGATION — Issue #730

## Audit of hypothesis 1 (sampler is NEAREST)

The shared bindless sampler is created in
`crates/renderer/src/texture_registry.rs:218-241`:

```rust
.mag_filter(vk::Filter::LINEAR)
.min_filter(vk::Filter::LINEAR)
.address_mode_u/v/w(SamplerAddressMode::REPEAT)
.anisotropy_enable(max_sampler_anisotropy > 1.0)
.max_anisotropy(max_sampler_anisotropy)
.mipmap_mode(SamplerMipmapMode::LINEAR)
.max_lod(16.0)
```

`load_dds` (line 318) passes `self.shared_sampler` to every loaded
texture, so the cloud DDS uses the same LINEAR/LINEAR/REPEAT/aniso
sampler the rest of the bindless array uses. The sampler is fine.
Hypothesis 1 is **disproven**.

## Audit of hypothesis 2 (DDS resolution / tile_scale)

`cloud_tile_scale_for_dds` divides `baseline * CLOUD_REF_WIDTH (=512)` by
`authored_width`, so a 256² DDS gets `tile_scale = baseline * 2`. That
keeps the *on-screen blob density* constant across authored widths — a
256² sprite tiles twice as often, exactly cancelling the per-texel size
increase. So if the cloud DDS at any width were the only issue, the
on-screen filtering would still come out the same as a 512² baseline.

**However** — and this is what the audit surfaced — `Texture::from_rgba`
(line 120) hard-codes `.mip_levels(1)`. Uncompressed-RGBA DDS files
arrive with their authored mip chain in the bytes, but the GPU image
gets a single level. The `from_bc` path (line 375) correctly uses
`meta.mip_count`. So compressed cloud DDS files (BC1/BC3 — vanilla
FNV's Bethesda cloud sprites) get full trilinear filtering, while any
uncompressed-RGBA cloud loses every mip below 0.

Mip-loss on its own causes minification aliasing, not magnification
pixelation, so it's not a complete explanation for "hundreds of pixels
per texel". But it is a real correctness gap surfaced during the audit
and worth fixing.

## What we don't know yet

The user's screenshot shows the artifact but the actual cloud DDS
dimensions, format, and mip count are not in the log. The shader's
projection (`dir.xz / max(elevation, 0.05) * tile_scale`) magnifies
heavily near the zenith — bilinear there can still show visible
texel-boundary diamonds when adjacent texels have high-contrast
alpha edges (cloud silhouette). Logging the DDS metadata is the
fastest discriminator the user asked for in suggested-fix step 2.

## Plan

This commit is diagnostic + safety — no behaviour change for the
common BC cloud path:

1. Log cloud DDS dimensions + format + mip_count in
   `resolve_cloud_layer` so the next streaming session immediately
   reveals what the cloud sprites actually are.
2. (Sibling correctness, scoped narrowly) — leave `from_rgba` as-is
   for this fix; if the diagnostic log shows uncompressed-RGBA cloud
   sprites, a follow-up adds the runtime mip-chain generation.
   The audit finding is captured in this file so it's not lost.
