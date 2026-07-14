**Severity**: low
**Dimension**: Volumetrics / cross-cutting doc (renderer audit 2026-07-14, DIM8+DIM16)
**Location**: `crates/renderer/src/vulkan/volumetrics.rs` (`struct VolumetricsParams`, `sun_dir` field doc, line ~79) + `docs/engine/shader-pipeline.md` (GpuCamera table, `sun_direction` row, line ~142)
**Status**: NEW (CONFIRMED against HEAD)

## Description
The sun-direction convention was corrected by commit `68d9c43b` (#1937/#1939): the host and every consumer upload `sun_direction` as the direction **TO** the sun (light-incoming), matching `GpuLight.direction_angle`, `GpuCamera.sun_direction`, `bindings.glsl`, and `water.frag`. That commit fixed the three shader comments and the math but left two docs asserting the old, wrong "from sun" convention:

1. `VolumetricsParams::sun_dir` Rust field doc reads *xyz = directional light "from sun toward ground"* (was finding **V-DIM16-01**, LOW).
2. `shader-pipeline.md` GpuCamera row reads *`sun_direction` … xyz = direction **from** sun (unit); w = sun intensity* (was finding **V-DIM16-02**, INFO).

Both are the exact wording that produced the #1937 sign bug. Consolidated here because they share one root cause and one fix.

## Evidence
- Live shader math is correct: `volumetrics_inject.comp` — `light_in = -normalize(sun_dir.xyz)` (propagation, away from sun); `ray_dir = -light_in = normalize(sun_dir.xyz)` (toward sun).
- Default `SkyParams.sun_direction = [-0.4, 0.8, -0.45]` (`context/mod.rs`) — positive +Y in Y-up = pointing up toward the sun.
- `bindings.glsl` `vec4 sunDirection;`; `water.frag` "xyz = world-space direction TO the sun", `sunDir = normalize(sunDirection.xyz);`.
- `volumetrics.rs:79` still reads "from sun toward ground"; `shader-pipeline.md:142` still reads "direction **from** sun".

## Impact
Documentation only — the code is correct. Risk is a future editor trusting either stale comment and re-negating the sign, reintroducing #1937.

## Related
#1937 / #1939 (the sun-direction sign fix); #1915 (sibling `shader-pipeline.md` table drift).

## Suggested Fix
- `volumetrics.rs`: *xyz = direction TO the sun (world space, unit; matches GpuCamera.sun_direction / GpuLight.direction_angle, #1937)*.
- `shader-pipeline.md:142`: change "direction **from** sun" → "direction **to** sun (unit)".

## Completeness Checks
- [ ] **SIBLING**: Grep the tree for any other "from sun" / "from the sun" comment on a `sun_direction`-family field and fix in lockstep.
- [ ] **TESTS**: N/A (doc-only); optionally add a source-scanning assertion pinning the convention string if churn recurs.
