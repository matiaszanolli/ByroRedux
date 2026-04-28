# EXT-RENDER-2: Cloud texture pixelated despite per-WTHR tile_scale derivation working — likely NEAREST sampler or undersized DDS

## Severity: LOW (cosmetic)

## Game Affected
Any exterior cell with WTHR cloud layers (FNV / FO3 / Skyrim / FO4 / SF).

## Surfaced By
M40 Phase 1b first FNV WastelandNV streaming session (2026-04-27, commit `7dc354a`). Default `NVWastelandClear` weather.

## Description
WTHR cloud-layer flow + parallax look correct (#M33.1 + #529 derivation working) but each cloud texture renders with visible texel boundaries — the cloud texture is being magnified to occupy hundreds of screen pixels per texel without bilinear or anisotropic filtering.

Two non-exclusive root causes:
1. **Sampler filter**: cloud sampler in `composite.frag` (or wherever the cloud blend lives) might be `NEAREST` instead of `LINEAR`.
2. **DDS resolution**: per #529 the per-WTHR `tile_scale` is derived as `baseline * 512 / authored_width`. If the FNV cloud DDS is 256² (some Bethesda DLC clouds are), `tile_scale` doubles to 0.30 and texels map to ~3 screen px.

## Suggested Fix
1. Audit cloud sampler creation — confirm `MagFilter::LINEAR`, `MipmapMode::LINEAR`, anisotropy on.
2. If sampler is fine, log cloud DDS dimensions in `resolve_cloud_layer`.
