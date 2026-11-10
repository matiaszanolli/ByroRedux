# PERF-2026-11-09-D5-02: the irradiance field is zero-stored for every froxel every frame, with its barrier pair, even when no local-grid box is live (unmeasured)

Labels: low,renderer,shaders,performance,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW — **unmeasured** (filed as a measure-first item). **Status**: NEW.

**Location**: tail of `crates/renderer/shaders/volumetrics_inject.comp` (`imageStore(localIrradiance, coord, irradiance); imageStore(localIrradiance, coord + ivec3(size.x, 0, 0), lightDirection);`), `LocalGrid::record_frame` (`crates/renderer/src/vulkan/volumetrics/local_grid.rs`; the irradiance read-to-write / write-to-read barriers are "unconditional", per a comment in the code), `composite_barriers` in the Stage F publish.

## Description
The light-field math is gated (`needsLightField` is true only for froxels within a box plus one froxel of margin: "rays are paid only there"), but the two stores are not: with no box live every froxel writes `vec4(0)` twice (16 B), the history barriers on the image run, and Stage F publishes it to the fragment stage. Composite never reads it unless `localGrid.info.z != 0`, and the inject reads the previous slot only inside `needsLightField`.

## Evidence
Derived store traffic = froxels x 16 B per frame: 14.7 MB at 720p, 33.2 MB at 1080p, 59.0 MB at 1440p, 132.7 MB at 4K native. Next to the 44 B of froxel state the same shader already writes per froxel per frame, that is +36 %. (Resident cost is D3-01.)

## Impact
Not quantified. At the 4070 Ti's bandwidth the 1440p figure is ~0.12 ms if the stores were bandwidth-bound, but they overlap the shader's ray work so the real number is probably a fraction of that; it is a fixed cost on the commonest frame type. The plan doc's fire cost tables all have fire in view, so the field's cost with no fire is not on record.

## Measure first
`volumetrics_inject_ms` on a no-fire exterior, shipped vs a build whose stores sit behind `localGrid.info.z != 0u`. Gating needs a history rule: on the first frame a box goes live the previous slot's field must not be blended (`prev_camera_pos.w` plus a host "field valid" bit), or stale values from an earlier fire leak in — which is why it is not a one-line change.

Related: #5950 (the inject's other atomics/stores were measured free; this site is different). Report: `docs/audits/AUDIT_PERFORMANCE_2026-11-09.md`.

## Completeness Checks
- [ ] **MEASURED**: Before/after numbers from an interleaved A/B (tier pinned) are attached; close with the number if the delta is below noise
- [ ] **TESTS**: A regression test pins this specific fix (history-validity rule for the first live frame)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct

