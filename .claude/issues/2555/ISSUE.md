# FNV-D2-02: classify_pbr_keyword's env-map arm is documented as the FNV majority path but is unreachable on ~83% of sampled FNV meshes post-#2315

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2555
**Finding ID**: FNV-D2-02

**Severity**: MEDIUM
**Dimension**: NIFAL Canonical Translation (FNV slice)
**Location**: `crates/core/src/ecs/components/material.rs:681-727`
**Status**: NEW

## Description
`classify_pbr_keyword`'s env-map arm's in-source comment claims `env_map_scale = 1.0` is FNV's neutral default on "nearly every FNV surface." That premise was invalidated by #2315 (CLOSED), which forces `env_map_scale` to 0.0 unless an explicit environment-mapping shader flag is authored. Measured: `env = 0.00` on 15 of 18 sampled FNV meshes. Compounding it, the arm's metalness lift reads `spec_lum`, which is 0.0 on all 18 sampled meshes per FNV-D2-01 (this session) — so even meshes that do reach the arm cannot produce `metalness > 0` from it. Not a wrong output on its own (matte fallback is defensible), but the single PBR decision point for all legacy content now documents a false reachability story, risking future audits/fixes reasoning from a stale premise.

## Evidence
Confirmed directly at `material.rs:680-695`: the comment reads "`BSShaderPPLighting` authors `env_map_scale = 1.0` as the neutral default on nearly every FNV surface, so this arm catches the vast majority of interior content."

## Impact
Not a wrong output on its own. Risk is to future audits/fixes reasoning from the stale reachability claim.

## Related
#2315 (CLOSED), #1873, #2352, FNV-D2-01 (this session).

## Suggested Fix
Correct the comment to state post-#2315 reachability; decide explicitly whether to retire or re-source the specular-luminance conductor lift. If FNV-D2-01 is fixed as suggested, this arm stays correctly inert and only the comment needs updating.

## Completeness Checks
- [ ] **TESTS**: N/A unless the arm's logic is also changed; if only the comment is corrected, no test needed
