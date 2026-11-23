# #7370 — REN-D2-2026-11-23-03: RLT-IBL-004's exterior ray is outside both the ray-quality tiers and the A13 cost gate

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D2-03). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Ray Queries / Performance coupling
- **Location**: `triangle.frag` sky-occlusion block (gate: `exteriorSkyProbe`, `probeShare`, `reflectionGlassRayEnabled`, `rtLOD < RT_LOD_REFLECT`, `DBG_DISABLE_AO`; no `rayBudget.qualityTier` term); `ray_budget.rs` `AdaptiveRayBudget::settings_for_tier`
- **Status**: NEW (the cost is disclosed in PRD rev 49: "its scenes are interiors, so the exterior ray's cost is not in it", and §6 row 12 "One more ray per exterior probe pixel"; no issue exists. `gh issue list --state all --search "sky occlusion ray exterior cost"` is empty)
- **Description**: tier 0 is documented as the watchdog-safe floor ("direct shadows but no diffuse path"), and #7076 / PAF-012 shed work by tier. The new ray fires for every non-glass exterior pixel with `rtLOD < 3` (about 1.3 BU per pixel and below, so most of the visible frame) at *every* tier, and the controller's cost model cannot shed it. The only cost evidence (A13) is on interior scenes, so the ray's cost on Whiterun-class exteriors is unmeasured, and the `floor` the controller learns now silently includes it (raising the climb target via #7076's `floor + ADDED_COST_ALLOWANCE_MS`).
- **Evidence**: `traceEnvironmentRay`-style gating only; `GpuRayBudget` has no field the block reads; PRD rev 49 text quoted above.
- **Impact**: potential frame-time regression on exteriors with no way for the adaptive budget to recover it; also feeds the heavy-scene-floor logic. No correctness impact.
- **Related**: #7076, #7221 (same "premium" accounting), PRD rev 49 A13 note.
- **Suggested Fix**: run A13-style exterior cases (Whiterun, Prospector exterior) with the ray on/off; if the cost matters, gate the ray on `rayBudget.qualityTier >= 1` (the SSAO form is already the fallback beside it).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **SHADER**: A GLSL edit recompiles the SPIR-V, including `triangle_early.frag.spv` (`scripts/check-shader-artifacts.sh` is the gate)
- [ ] **TESTS**: A regression test pins this specific fix
