# #4787: PERF-D5-2026-09-23-05: `sampleLocalMedium` evaluates the full density profile of transported volumes before discarding them

**Severity**: LOW
**Labels**: low, performance, renderer, shaders, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23.md (PERF-D5-2026-09-23-05)

- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `volumetrics_inject.comp:731-741`
- **Status**: NEW
- **Description**: The discard test is `!evaluateLocalFogVolume(...) || isTransportedProfile(profileKind)`. The profile kind is `volume.profile_params.x`, which is known before any evaluation. For every froxel inside a Smoke/Flame/Explosion primitive, the function runs `localProfileRadius`, `sampleLocalTurbulence` (2 noise texture fetches) and `localDensityProfile`, then throws the result away, because transported media are owned by the transport field.
- **Impact**: This affects only froxels inside transported primitives. That is small for torches, but it covers large regions for explosions and nuclear clouds and for near-camera fires, where froxels are densest. Fixing it is a one-line reorder.
- **Suggested Fix**: `if (isTransportedProfile(clamp(volume.profile_params.x, …))) continue;` before `evaluateLocalFogVolume`. Even better, have the CPU cluster build skip transported volumes in a separate "homogeneous-only" index list.

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
