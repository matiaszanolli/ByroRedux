# EXT-RENDER-3: Perceived view distance ~30m on FNV exterior despite WTHR fog_far=200000 — fog color (likely green) dominates distant pixels

## Severity: MEDIUM (cosmetic but heavily affects exterior playability perception)

## Game Affected
FNV (any exterior).

## Surfaced By
M40 Phase 1b first FNV WastelandNV streaming session (2026-04-27, commit `7dc354a`). Default `NVWastelandClear` weather.

## Description
User reports view distance is "a couple of meters" — terrain past ~30m fades into uniform green-grey. WTHR's authored `fog_day = -10 to 200000` (~1 mile) is fine. The user suspects fog *color*, not distance.

## Hypothesis
`CellLightingRes.fog_color` from `wthr.sky_colors[SKY_FOG][TOD_DAY]`. `SKY_FOG = 1` "likely doesn't match FNV's actual NAM0 layout".

## Discriminator
"Add `fog_color` to the `apply_worldspace_weather` log line. Resolves #729 and this ticket simultaneously."

## Related
- #729 — global green tint; suspected same root cause.
