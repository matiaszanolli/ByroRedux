# #5401: PERF-D5-2026-10-08-01: #5249's transmission-lobe shadow trace adds an unmeasured, ray-tier-ungoverned closest-hit loop to the dominant GPU pass

**Labels**: medium,performance,renderer,shaders,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5401

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-08.md` — `PERF-D5-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `triangle.frag` gate at ~:3855, `MAX_TRANSMISSION_SELF_SKIPS 8u` at `shader_constants.glsl:38`, no `rayBudget` read on this path. Related correctness finding on the same function: REN-D10-2026-10-08-01 (glass mask on the foreign-hit leg).

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**:
  - `crates/renderer/shaders/triangle.frag:3833-3867` (the finalize; the same source builds `triangle_early.frag.spv`)
  - `crates/renderer/shaders/include/shadow_transport.glsl:179-225` (`traceShadowTransmittanceSkippingInstance`)
  - `crates/renderer/shaders/include/lighting.glsl:494-515` (`traceLightTransmittanceSkippingInstance`)
  - `crates/renderer/shaders/include/shader_constants.glsl:38` (`MAX_TRANSMISSION_SELF_SKIPS` 8u)
- **Status**: NEW. It arrived with `34c3adf14` (#5249, 2026-10-07), a closed correctness fix. It is not listed in ROADMAP R6a-stale-25's frame-path changes (the list predates it).
- **Description**: the transmission half of the selected light's radiance (wrap excess, back-light, translucency) used to be unshadowed (#5192). #5249 gave it its own visibility trace. For each fragment whose `restirSelectedTransmission` is non-zero and `shadowFade > 0.01`, the finalize now runs `traceLightTransmittanceSkippingInstance`: a loop of up to 8 `rayQueryInitializeEXT` + full `rayQueryProceedEXT` traversals.
  - Each hop needs the nearest committed hit, so it cannot use `TerminateOnFirstHit`.
  - A hit on the receiver's own instance is stepped past. The first foreign hit hands the remaining leg to the shared alpha/glass-aware transport, which is a further traversal.
  - Eligibility is material-scoped. The lobes are non-zero only for materials with `MAT_FLAG_SOFT_LIGHTING`, `MAT_FLAG_BACK_LIGHTING` or `MAT_FLAG_TRANSLUCENCY` (wrap-lit skin, hair, foliage, FO4 v≥8 translucency), and only when the selected light is behind the shading plane (`rawNdotL < 0`).
  - That includes Skyrim and FO4 NPC bodies.
- **Evidence**:
  - `triangle.frag:3854-3867` gates the trace only on `dot(restirSelectedTransmission, restirSelectedTransmission) > 1e-12 && shadowFade > 0.01`.
  - The loop bound is the constant `MAX_TRANSMISSION_SELF_SKIPS`.
  - `GpuRayBudget` (`scene_buffer/ray_budget.rs:12-30`) has `direct_shadow_samples`, `ray_count`, `quality_tier` and others; none of them is read by this trace. The existing shadow rays are bounded by `clamp(rayBudget.directShadowSamples, 1u, MAX_DIRECT_SHADOW_SAMPLES)` (`triangle.frag:3771`).
  - The commit message records "live-verified on FNV GSProspectorSaloonInterior, 120 frames, RT tier 1, zero validation errors" and nothing about `main_render_ms`.
- **Impact** (derived; unmeasured): on every eligible fragment of every frame, at least one extra traversal on top of the K ≥ 1 existing shadow rays, and up to 8 plus a shared-transport leg when the ray starts inside a closed own-instance body, which is exactly the case the trace was written for. At tier 0, `directShadowSamples` clamps to the minimum, so the relative increase is largest where the adaptive budget is trying to shed cost. In the last measured dense scene the main pass is 60–67 ms with about 27 ms attributable to rays, so this is the pass where a regression is least affordable. The size depends on the on-screen share of those materials, which no in-repo capture quantifies.
- **Related**: #5192 (the lobe split), #5249 (closed), #4946 (the wall-bleed it re-closes), `docs/audits/AUDIT_PERFORMANCE_2026-09-26b.md` (hotspot attribution), ROADMAP R6a-stale-25.
- **Suggested Fix**:
  1. Capture `main_render_ms` A/B with `--rt-test-ray-quality-tier` pinned (same pose, same upscaler) on a Skyrim NPC interior (WhiterunBanneredMare) and FO4 MedTek, before and after `34c3adf14`.
  2. If material, reuse ray 0's result. It already traced the same origin and direction (`selectedRayOrigin/Direction/TMax`) through `traceLightTransmittanceDetailed`, which reports a committed instance. The skip loop adds information only when that first hit is the receiver's own instance. Verify the exact `committedInstance` semantics in `traceShadowTransmittanceDetailed` before relying on it.
  3. Cap the hop budget below 8 for the common case, and/or gate the trace on `rayBudget.quality_tier`.
  - This is a shader change with no unit-test coverage of cost. State confidence and keep a revert plan (see the skill's speculative-Vulkan caveat).

## Completeness Checks
- [ ] **SIBLING**: Other per-lobe/secondary visibility traces in `triangle.frag` are checked for the same ray-tier gating
- [ ] **TESTS**: A `main_render_ms` pinned-tier A/B is recorded before and after the change (shader cost has no unit-test coverage)
