# REN-D15-09: submersion_system WaterVolume-absent exit leaves stale submersion state

## Description
Two "no water data" exits with opposite behaviour: the `WaterPlane`-absent exit resets `SubmersionState` to default with a comment explaining why; the `WaterVolume`-absent exit twenty lines later is a bare `return`, so the camera keeps `head_submerged: true` and a stale `material` that `compute_underwater_params` then feeds indefinitely. Only separable via `spawn_lod_water_plane` (#2449), which inserts `WaterPlane` without `WaterVolume` — a state in which the camera cannot already be submerged, so today's outcome is "no reset needed". Defence-in-depth gap.

## Location
`byroredux/src/systems/water.rs` (`submersion_system`)

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2792

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D15-09).
