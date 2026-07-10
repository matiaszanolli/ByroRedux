# REN-D3-01: composite.frag.spv is stale — commit 977eb95a removed the depth_params.z volumetric gate from GLSL without recompiling the shipped binary

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1917

**Severity**: medium
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/composite.frag.spv` (last built at `9c10f14e`, 2026-06-15) vs `crates/renderer/shaders/composite.frag:417-445` (changed at `977eb95a`, 2026-07-07)
**Status**: NEW

## Description
Commit `977eb95a` (titled "Add Scripting Subsystem Audit report" — the renderer changes are buried in an unrelated-looking commit) rewrote the volumetric-apply block in `composite.frag`: the runtime branch `if (params.depth_params.z > 0.5) {...} else {...}` became the unconditional `combined = combined * vol.a + vol.rgb;`. The commit recompiled `triangle.frag.spv` and both volumetrics `.spv` but NOT `composite.frag.spv`. Since shaders ship as committed binaries via `include_bytes!`, the stale binary is the runtime shader.

## Evidence
Byte-for-byte proof: recompiling the pre-`977eb95a` source with local glslangValidator 16.2.0 produces output identical to the committed `composite.frag.spv` (`cmp` clean, 21924 B), while recompiling HEAD source differs (21680 B, differs at byte 13).

## Impact
Zero behavioral divergence today (`draw.rs` pins `depth_params.z = 1.0`, so the stale binary's branch always takes the same math path). The hazard is latent: the source-of-truth contract ("All GLSL edits require a recompile") is silently violated, and the next unrelated `composite.frag` recompile will silently ship this change on top of whatever else changed.

## Related
PIPE-01/SHDR-01 (AUDIT_RENDERER_2026-06-02, same class); cross-referenced by the SSBO/Ray-Queries, Denoiser/Composite, and Water dimensions of this same audit

## Suggested Fix
Recompile with plain `glslangValidator -V` (per project convention — no `-g0`) and commit the `.spv`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
