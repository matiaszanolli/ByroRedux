# #899 — REN-D15-03: cloud layers 2/3 (ANAM/BNAM) reuse layers 0/1 (DNAM/CNAM) scroll velocities

**Severity**: LOW (visual variety; cross-cuts undecoded WTHR ONAM/INAM)
**Dimension**: Sky / Weather / Exterior Lighting
**Location**: `byroredux/src/systems.rs:1721-1731`
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-05-07_DIM15.md` § REN-D15-03
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/899

## Summary

`weather_system` advances 4 cloud scroll accumulators per frame, but layer 2 (ANAM) uses the same velocity as layer 0 (DNAM) and layer 3 (BNAM) uses the same as layer 1 (CNAM). Visual variety relies entirely on different texture lookups; identical-texture or absent ANAM/BNAM ⇒ no visible difference between layer pairs.

## Fix path

**Long-term**: decode WTHR ONAM (4 B, f32-likely) and INAM (304 B) per UESP byte sampling — surface per-weather scroll velocities. Original tracking #541 closed for SKY_LOWER/STARS but speed source remains undecoded.

**Interim cosmetic**: hardcode layers 2/3 to distinct multipliers (e.g. 0.85× and -1.15×) so 4 layers have 4 distinct speeds. Cosmetic patch; proper fix is ONAM/INAM decode.

## Status

NEW. CONFIRMED via line-walk. #541 closed, ONAM/INAM has no current open tracker — this issue stands as the catch-all.
