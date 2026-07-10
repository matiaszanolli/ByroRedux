# REN-D8-02: #865 XCLL cubic-fog was never reachable for the interiors it targets

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1927

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/composite.frag:510,533-540`
**Status**: NEW

## Description
The #865 XCLL cubic fog, per its own comment, was authored for "Vanilla FNV interiors (Doc Mitchell's House, Goodsprings Source Pump)". But it lives inside the exterior-only branch (`depth_params.x > 0.5`, present since before the cubic-fog addition) and mixes toward `compute_sky()` sky-haze — a quantity that is meaningless for interior cells (no sky). So even independent of the now-dead `z` gate, this feature could never have applied to the interiors it targets, and would have fogged toward the wrong color if it had.

## Evidence
`depth_params.x = is_exterior ? 1.0 : 0.0`; the cubic branch mixes `tonemapped` toward `aces(compute_sky(viewDir)*exposure)`; `git log -L510` confirms the exterior gate predates the #865 cubic-fog addition.

## Impact
FNV interior XCLL-authored fog curves are parsed and uploaded but never shape interior fog via composite; interior atmospheric depth is entirely dependent on the volumetric path. Low real-world impact given volumetrics runs for interiors.

## Related
REN-D8-01

## Suggested Fix
If interior XCLL cubic fog is still desired at composite, it needs its own interior-scoped branch that mixes toward `fog_color` (not sky-haze), independent of the exterior aerial-perspective path — or the feature should be explicitly retired in favor of volumetrics.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
