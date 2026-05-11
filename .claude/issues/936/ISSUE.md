# NIF-D5-NEW-01: NiBSplineCompFloatInterpolator + NiBSplineCompPoint3Interpolator not dispatched

**Severity**: HIGH
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 5)

## Game Affected

FNV, FO3, Skyrim LE/SE, FO4 (per the "B-splines aren't Skyrim+ only" feedback memory — these are reachable on FO3/FNV too).

## Location

`crates/nif/src/blocks/mod.rs:683` — only `NiBSplineCompTransformInterpolator` is dispatched. The float and point3 companions are absent.

## Why it's a bug

These cannot alias the transform parser — they inherit `NiBSplineFloatInterpolator` / `NiBSplinePoint3Interpolator` and add 2× `f32` (offset + half-range) on top of a different parent layout.

Wherever a transform B-spline ships on a `NiControllerSequence`, paired float (alpha/scale) or Point3 (color/translation) compact splines usually ride alongside.

## Impact

Silent `NiUnknown` skip via block_size (no drift on 20.0.1.0+). Animation importer loses the curves; affected float and Point3 channels collapse to constant or rest pose. The renderer-side animation stack receives nothing for these clips.

## Fix

Two new parsers in `interpolator.rs` next to the existing transform variant (~40 LOC each): parse base interp fields (value/handle), then trailing offset/half-range pair. Wire dispatch entries. Animation channel emitter in `crates/nif/src/anim.rs` already handles the transform variant; copy that path for float and Point3.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Both Float + Point3 variants added; verify dispatcher in `blocks/mod.rs` for the NiBSpline* family
- [ ] **DROP**: N/A
- [ ] **TESTS**: Add fixture parse tests for both new interpolator types; extend `parse_real_nifs.rs` to assert non-zero count on FNV/FO3 corpus
