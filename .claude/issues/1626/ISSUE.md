# TD5-001: GI hit-normal SSBO fetch mis-resolved — bounce uses a stand-in normal

_Filed as #1626 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Stale Marker · **Effort**: medium · **Age**: commit 6ac502ac8, 2026-06-05
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD5-001)
**Status**: Active marker, no tracking issue (this issue is that tracker)

## Description
The 1-bounce GI path in `crates/renderer/shaders/triangle.frag` (~3591-3598) approximates the receiver normal as `-giDir` because "the SSBO normal/position fetch mis-resolved here." Unlike the other two stale markers (unbuilt features), this documents a *known-broken* fetch. Visual impact is "less sharp colour bleed," not "wrong colour" (a defensible 1-spp choice) — hence LOW.

## Evidence
`triangle.frag:3591` "with `-giDir` (the surface as seen facing the incoming GI"; `:3597` "mis-resolved here)."; `:3598` `vec3 hitN = -giDir;`.

## Impact
GI bounce uses a stand-in normal instead of the hit surface's true normal. Marker has no driving issue, so it is invisible to issue-state audits.

## Suggested Fix
Investigate the SSBO normal/position fetch mis-resolution so the bounce can use the true hit normal; this issue is the tracker. If the fetch was never actually attempted, downgrade the marker wording to "not yet wired."

## Completeness Checks
- [ ] **SIBLING**: Check the other RT hit-attribute fetches (shadow/reflection) resolve the SSBO normal/position correctly
- [ ] **TESTS**: A Cornell-box / GI reference comparison pins the colour-bleed sharpness if the fetch is fixed
