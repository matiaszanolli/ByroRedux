# WAT-D15-03: wave_amplitude/wave_frequency are parsed onto WaterMaterial but never forwarded to the water shader

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1936

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/core/src/ecs/components/water.rs:141-149` (`wave_amplitude`, `wave_frequency`); `byroredux/src/render/water.rs:73-117` (`WaterPush` build)
**Status**: NEW

## Description
`WaterMaterial.wave_amplitude`/`wave_frequency` are promoted onto the canonical material (WATAL Phase 1) and set at parse time, but `WaterPush` carries no field for them and `water.frag` never reads them. There is no vertex displacement or amplitude-driven chop at the shader boundary today — wave motion is purely normal-map/procedural.

## Evidence
`WaterPush` fields (timing/flow/shallow/deep/scroll/tune/misc/tint_reflect) contain no amplitude/frequency slot; the 128-byte push block is full (static-assert in `water.rs:95`). The component doc-comment explicitly acknowledges the deferral: "the flat-mesh RT path does not displace the BLAS per frame … drives normal-only chop today and an optional displacement pass later."

## Impact
Cosmetic/latent only — matches the tracker's existing "field parsed but not forwarded" pattern (cf. #1580, #1856). Not a correctness bug; the deferral is intentional and documented.

## Related
WATAL §6 (`docs/engine/watal.md`)

## Suggested Fix
If/when a displacement pass lands, thread `wave_amplitude`/`wave_frequency` through push constants — the 128-byte push block has no headroom, so it would need the set-1 instance SSBO or a device-capability check first. No action required now.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
