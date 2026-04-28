# EXT-RENDER-1: Global green tint on FNV WastelandNV exteriors despite WTHR-decoded ambient/sunlight being cool blue

## Severity: MEDIUM

## Game Affected
FNV (any exterior); likely FO3 / Skyrim / FO4 / SF too once their exteriors stream

## Surfaced By
M40 Phase 1b first FNV WastelandNV streaming session (2026-04-27, commit `7dc354a`).

## Evidence

`WTHR 'NVWastelandClear'` resolved at boot via the freshly-added `apply_worldspace_weather` log (#7dc354a):

```
zenith   = [0.196, 0.278, 0.529]   blue (correct sky upper)
horizon  = [0.361, 0.451, 0.580]   desaturated blue-grey (correct)
sun      = [1.0,   0.890, 0.667]   warm yellow (correct)
ambient  = [0.592, 0.659, 0.718]   cool blue-grey  ← NOT green
sunlight = [0.341, 0.412, 0.541]   cool blue       ← NOT green
fog_day  = -10 to 200000           (long, ~1 mile)
```

Yet rendered terrain + foreground meshes have a uniform green cast. Neither of the two lighting terms the engine consumes (`CellLightingRes.ambient`, `CellLightingRes.directional_color = sunlight`) match that hue.

## Hypothesis Ranking

1. **Fog color comes from `SKY_FOG[TOD_DAY]` (NAM0 group index 1)** — currently *not logged*.
2. **`weather_system` TOD interpolator** drifting at game_hour=10.0 default, picking the wrong key pair.
3. **`NiVertexColorProperty.lighting_mode` (#694)** unconditionally multiplying scene color by an RGB that drifts green.

## Reproduction

```
cargo run --release -- --esm "Fallout New Vegas/Data/FalloutNV.esm" --grid 0,0 --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" --textures-bsa "Fallout - Textures2.bsa"
```

## Suggested Investigation

1. Add `fog_color` to the `apply_worldspace_weather` log line — fastest discriminator.
2. If `fog_color` IS green: NAM0 slot index audit against UESP's FNV-specific WTHR docs + xEdit's NAM0 schema view.
3. If `fog_color` is NOT green: bisect downstream — disable fog blend in composite.frag, re-test. Then disable vertex-color modulation, re-test.
