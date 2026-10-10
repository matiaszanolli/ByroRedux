# #5482 — investigation

## The issue's water diagnosis was a misread oracle

The black field is NOT distant LOD water. `render.debug water_term` /
`water_refl` paint every non-water pixel with
`RENDER_DEBUG_NON_PARTICIPANT_GREY` (`vec3(0.08)` → ~80/255 after the
display encode) — the "mean 79-80 over black pixels" in the issue body is
that grey, i.e. the black pixels are ordinary opaque geometry (terrain,
mountain, trunks). Real water in the same frame shows as the
refrHit/foam/alpha triplet (a small green patch).

## Actual cause: contrast pivot in pre-exposure units

- `render.debug composite_term` shows a healthy (if dim) frame: the shadowed
  ground/mountain sit at ~0.015-0.02 linear. `r.health` clean (no NaN).
- Live grade (Tamriel weather): `sat=1.185 bri=1.125 con=1.300`.
- `presentation.frag` ran `(x - 0.18) * contrast + 0.18` on RAW radiance,
  then multiplied by exposure inside `tonemap(...)`. Anything under
  `0.18 * (1 - 1/1.3) = 0.0415` went negative → floored to 0 by `aces()`.
  With the auto meter pinned at its 2.0 ceiling the pivot sat at 2× the
  metered key, so ~60 % of the frame crushed.
- The user's "floors streaming wrong" report (same session) is the same
  defect: sun-averted terrain triangles (ambient only) floored to 0 read as
  holes with hard triangle edges.

## Reference

Decompiled vanilla Skyrim ISHDR (Skyrim Community Shaders
`package/Shaders/ISHDR.hlsl`): adaptation multiply first, tonemap, then
`lerp(avgValue.x, tinted, Cinematic.z)` — vanilla also crushes; CS adds
`pow(abs(c)/avg, contrast) * avg` blended in by `saturate(modified / 0.1)`
("Contrast modified to fix crushed shadows").

## Fix

Expose before the grade; contrast = linear stretch about 0.18 (exposed)
with the CS power-law toe below 0.1. Same frame: exactly-zero pixels
59.7 % → 0.0 %, midtones 7.3 % → 21.4 %.

## Unrelated, seen during verification

Rapier `sap_axis.rs:61` panic after a ragdoll invalid-solve restore +
explosive-velocity clamp on actor 16468 — matches open #5352.
