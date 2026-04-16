# AR-08: NiBlendInterpolator parsed but never consumed in animation import

## Finding: AR-08 (MEDIUM)

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`
**Dimension**: Animation Readiness
**Games Affected**: Skyrim, FO3, FNV, FO4 (NIFs with embedded multi-sequence animations)
**Location**: `crates/nif/src/blocks/interpolator.rs:728-910` (parsed), `crates/nif/src/anim.rs:411-442` (not consumed)

## Description

NiBlendTransformInterpolator and its siblings (Float, Point3, Bool) are fully parsed at the block level, but `extract_transform_channel` only handles NiTransformInterpolator and NiBSplineCompTransformInterpolator. When a ControlledBlock's interpolator_ref points to a NiBlendTransformInterpolator (common for NIF-embedded controller managers with multiple active sequences), the channel extraction returns None and animation data is silently lost.

The AnimationStack provides layer-based blending at the ECS level, but there is no bridge that decomposes a NiBlendInterpolator's sub-interpolator array into separate layers.

## Evidence

```
grep -r "NiBlendTransformInterpolator" crates/nif/src/anim.rs
# No matches — type is never handled in animation import
```

## Impact

Embedded multi-sequence animation blending (idle loops with overlaid partial-body animations) fails silently. Single-sequence KF files work fine. Affects complex NPC animation setups.

## Suggested Fix

When `extract_transform_channel` encounters a NiBlendTransformInterpolator, follow its weighted interpolator array, extract each sub-interpolator as a separate channel, and either merge at import time or create multiple AnimationStack layers.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`._
