# D5-NEW-02: NiPathInterpolator falls through extract_transform_channel — embedded path animations static-pose

## Finding: D5-NEW-02

- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`
- **Game Affected**: Oblivion (door swings), FO3/FNV (moving platforms), Skyrim (minecart rails, dragons)
- **Location**: [crates/nif/src/anim.rs:874-915](crates/nif/src/anim.rs#L874-L915), [crates/nif/src/blocks/interpolator.rs:556-620](crates/nif/src/blocks/interpolator.rs#L556-L620)

## Description

`NiPathInterpolator` (driven by `NiPosData` + `NiFloatData` + a percent-along-path interpolator) is fully parsed (regression test at mod.rs:1349-1373 from #394) and dispatches via `parse_block`. Like D5-NEW-01, it never reaches a downcast in `extract_transform_channel`, so spline-path-driven embedded animations are silently dropped. The legacy `NiPathController` is parsed but explicitly stubbed (mod.rs:567 comment: "legacy NiTimeController"); content using the pre-Bethesda path setup loses translation entirely.

Continuation of closed #228 — same shape as D5-NEW-01.

## Evidence

```rust
// crates/nif/src/anim.rs:888-915 — only two interpolator types match
if let Some(interp) = scene.get_as::<NiTransformInterpolator>(interp_idx) { ... }
if let Some(interp) = scene.get_as::<NiBSplineCompTransformInterpolator>(interp_idx) { ... }
None  // NiPathInterpolator falls here
```

## Impact

Embedded path animations (door hinge sweeps, dragon flight curves authored as splines, minecart spline rails) static-pose. Scripted door open in Oblivion shipping a path interpolator on the door NIF — non-functional from the NIF side.

## Suggested Fix

Sample `NiPathInterpolator` at the same `BSPLINE_SAMPLE_HZ` cadence used for `NiBSplineCompTransformInterpolator`, emit linear-interpolated translation keys. Rotation may be derivable from path tangent (Frenet frame) but Gamebryo just held the static rotation — match that to start.

## Related

- #228 (closed): parser added; import dispatch missing.
- D5-NEW-01: companion (NiLookAtInterpolator).
- #394 (closed): added the parser regression test.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Bundle with D5-NEW-01.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic NIF with NiPathInterpolator + 3-point NiPosData → verify sampled keys interpolate the path.

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`._
