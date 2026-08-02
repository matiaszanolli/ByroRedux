# REN-D18-01: Session 62 made only the interior ambient cube authoritative — is_exterior and the whole TOD sky survive an exterior→interior transition

Severity: high
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2226

**Dimension**: 18 (Sky/weather)
**Location**: `byroredux/src/render/sky.rs` (`build_sky_params`, line 55)
**Status**: NEW

**Description**: `3b922734` correctly made the interior XCLL cube override a stale exterior
`SkyParamsRes`, per its own in-source comment stating that premise — but
fixed only the `dalc_cube` field. Every other field, including
`is_exterior: sky_res.is_exterior`, still flows unconditionally from the
stale resource. Since #1199, `SkyParamsRes` is worldspace-scoped and
survives cell unload/transition by design, and `SkyParamsRes` is *only ever*
constructed with `is_exterior: true`. A sealed interior hides the symptom
(the sky term is gated on `depth==1.0`), which is why the Session 62 author
only saw the ambient-cube half of it; any interior with an unsealed
roof gap or failed mesh gets full exterior TOD sky, sun disc, height fog,
and the exterior volumetric shadow-ray path.

**Evidence**: `sky.rs:75-116` — when `world.try_resource::<SkyParamsRes>()` returns `Some` (a stale resource from a prior exterior load), every field (`zenith_color`, `sun_direction`, `is_exterior`, cloud fields, etc.) is copied straight from `sky_res` unconditionally, EXCEPT `dalc_cube` at line 115 which correctly does `interior_cube.or_else(|| sky_res.current_dalc_cube...)`.

**Impact**: Any interior cell with an unsealed roof gap or a mesh/collision failure renders full exterior time-of-day sky, sun disc, height fog, and the exterior volumetric shadow-ray path — a correctness gap present since #1199, now more visible because Session 62 partially (not fully) fixed the symptom.

**Suggested Fix**: make the interiority decision one read-side call — when
`CellLightingRes.is_interior`, return `SkyParams { dalc_cube: interior_cube,
..SkyParams::default() }` rather than only overriding one field.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
