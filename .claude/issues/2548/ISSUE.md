# FO3-D2-05: extract_emitter_rate only downcasts interpolator_ref as NiFloatInterpolator -- drops the authored rate on 78% of real FO3 particle effects

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2548
**Finding ID**: FO3-D2-05

**Severity**: MEDIUM
**Dimension**: NIF v20.2.0.7 Parser — FO3 Block Subset
**Location**: `crates/nif/src/import/walk/mod.rs:842-865` (`extract_emitter_rate`, "Modern" branch)
**Status**: NEW

## Description
The modern-path branch downcasts `NiPSysEmitterCtlr.interpolator_ref` only as `NiFloatInterpolator` and returns `None` on failure, never handling the `NiBlendFloatInterpolator` wrapper (`blocks/interpolator.rs:1052`, a weighted array of sub-interpolators via `InterpBlendItem`).

## Evidence
Real `Fallout - Meshes.bsa` (10,989 NIFs): of 361 `NiPSysEmitterCtlr.interpolator_ref` targets, **283 (78%) are `NiBlendFloatInterpolator`** and only 78 are the handled `NiFloatInterpolator`. Affected files include `fxharoldfire.nif`, `fxpulseexplosion01.nif`, `fxplasmacritburst.nif`, `explosionbigfiry.nif`, `fxravenrockexplosion01.nif`, `fxdrippingwater01.nif`, `fxdrippingblood03.nif`, `fxgorepophead01.nif` — most of FO3's fire/explosion/dust/blood/gore VFX library. Confirmed directly: the Modern branch (`walk/mod.rs:843-864`) only calls `scene.get_as::<NiFloatInterpolator>(interp_idx)`, no `NiBlendFloatInterpolator` arm.

## Impact
Visual-only regression — affected emitters silently fall back to the heuristic preset spawn rate instead of the artist-authored birth-rate curve. No parse failure, no crash, no data loss. `emitter_params` (speed/radius/life-span/color) and GrowFade `base_scale` both decode correctly for the same blocks — only the birth-rate path is affected.

## Related
Upstream of #1364/#1402/#1771 (the `sane()`-filter follow-up on the *handled* `NiFloatInterpolator` path — this bug is on the interpolator-*type* dispatch, not the value-sanity gate). No existing open issue matches.

## Suggested Fix
In the "Modern" branch, on `NiFloatInterpolator` downcast failure, try `NiBlendFloatInterpolator`: walk `base.items`, resolve the highest-weight (or first) `InterpBlendItem.interpolator_ref` as `NiFloatInterpolator`, apply the same keyed-data → constant-value → `sane()` chain. Add a fixture-based regression test sibling to `import/walk/tests.rs:798-861`.

## Completeness Checks
- [ ] **TESTS**: A fixture-based regression test pins the `NiBlendFloatInterpolator` decode path (mirroring `import/walk/tests.rs:798-861`)
- [ ] **SIBLING**: Check other `NiPSysEmitterCtlr`-adjacent interpolator dispatch sites (e.g. speed/radius/color) for the same `NiFloatInterpolator`-only gap
