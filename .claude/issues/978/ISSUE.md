# Issue #978

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/978
**Title**: NIF-D5-NEW-02: Uncompressed NiBSpline{Transform,Float,Point3}Interpolator have no dispatch arms
**Labels**: bug, animation, nif-parser, medium
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: `docs/audits/AUDIT_NIF_2026-05-12.md` (Dim 5)
**Severity**: MEDIUM
**Dimension**: Coverage
**Game Affected**: FO3, FNV, Skyrim SE (older idle KFs authored without compression). Pre-Bethesda Gamebryo titles (Civ IV, Empire Earth) also affected
**Location**: `crates/nif/src/blocks/mod.rs:707-721` — only `…Comp…` variants dispatch

## Description

`nif.xml` defines six concrete B-spline interpolator classes; only the compressed three have dispatch arms:

```
NiBSplineFloatInterpolator       inherit="NiBSplineInterpolator"   (concrete, NO arm)
NiBSplineCompFloatInterpolator   inherit="NiBSplineFloatInterpolator"   ← arm exists
NiBSplinePoint3Interpolator      inherit="NiBSplineInterpolator"   (concrete, NO arm)
NiBSplineCompPoint3Interpolator  inherit="NiBSplinePoint3Interpolator"   ← arm exists
NiBSplineTransformInterpolator   inherit="NiBSplineInterpolator"   (concrete, NO arm)
NiBSplineCompTransformInterpolator inherit="NiBSplineTransformInterpolator"   ← arm exists
```

`niflib` ships layouts for all six. The uncompressed variants appear in earlier Gamebryo content and also in some FO3/FNV idle KFs where the animator chose verbatim control points over compression.

## Impact

- On FO3+: any KF with an uncompressed B-spline channel falls to `NiUnknown`, channel collapses to rest pose. Drift telemetry (#939) now surfaces these but the playback is silently broken.
- On Oblivion: no block_sizes recovery, the parse cascades and truncates the rest of the NIF.

Memory of the Session-34 closeout for #936 explicitly notes "alpha / scale floats and color / translation Vec3s usually ride alongside [a CompTransform]" — same logic applies to the uncompressed variants when the original animator skipped compression.

## Suggested Fix

Add three dispatch arms. Two options:

**Option A** — alias to the compressed parsers' uncompressed siblings (preferred if the on-disk layout is a strict subset; needs niftools verification):

```rust
"NiBSplineFloatInterpolator" | "NiBSplineCompFloatInterpolator" => { /* existing body */ }
"NiBSplinePoint3Interpolator" | "NiBSplineCompPoint3Interpolator" => { /* existing body */ }
"NiBSplineTransformInterpolator" | "NiBSplineCompTransformInterpolator" => { /* existing body */ }
```

**Option B** — dedicated parsers for the uncompressed variants (correct if the layouts diverge; needs nif.xml inspection of the `compact_control_points` vs `control_points` fields).

Inspect `nif.xml` lines for each class before deciding. The `NiBSplineFloatInterpolator` layout has `start_time/stop_time` + `spline_data_ref` + `basis_data_ref` + `float_control_points` (vs compressed's `compact_control_points`).

## Completeness Checks

- [ ] **NIFXML_REVIEW**: Read nif.xml entries for all 6 B-spline classes; document whether Option A or B is correct
- [ ] **TESTS**: Fixture-based parse of an uncompressed-B-spline KF (find one in a FO3/FNV `meshes\actors\character\idleanims\*.kf` archive sweep)
- [ ] **DRIFT_HISTOGRAM**: After fix, the `NiBSpline*Interpolator` rows in `--drift-histogram` should show zero
- [ ] **SIBLING**: Memory `feedback_bspline_not_skyrim_only.md` notes B-splines are reachable on FNV/FO3 — verify the fix is exercised on at least one FNV idle KF in tests, not just Skyrim
- [ ] **ANIM_CONVERT**: `byroredux/src/anim_convert.rs` channel conversion must handle uncompressed control points — verify the existing CompTransform path doesn't assume compression-specific fields

## Audit reference

`docs/audits/AUDIT_NIF_2026-05-12.md` § Findings → MEDIUM → NIF-D5-NEW-02.

Related: #936 (compressed-variant dispatch closeout — this is the uncompressed counterpart).

