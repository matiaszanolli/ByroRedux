# REN-D14-NEW-02: caustic max_lights budget bounds accepted lights, not ray queries — occluded lights trace for free

- **Severity**: MEDIUM
- **Dimension**: 14 — Caustics / Performance
- **Location**: `crates/renderer/shaders/caustic_splat.comp` — the `for (uint li = 0u; li < lightCount && processedLights < maxLights; ++li)` loop and the `processedLights++` placement, against the documented bound on `is_caustic_source` in `crates/renderer/src/vulkan/context/draw.rs`
- **Description**: `processedLights++` executes after the visibility block, so a light inside the radius but shadowed hits `continue` and never increments the counter — having already spent a `rayQueryEXT` traversal (two when `visibilityMaskUsesGlass`). The loop's real per-pixel bound is `lightCount` traversals, not `maxLights` (uploaded as 8). `MAX_LIGHTS` is 512 — worst case ~2 orders of magnitude above the intended budget. Directional lights skip the radius test and are culled only by the Lambert cosine.
- **Evidence**: `needsVisibility` block ends `continue;` with `processedLights++` sitting below it. `visibilityMaskNeedsTrace` is true for any light whose mask touches `VISIBILITY_MASK_ALL_OPAQUE | VISIBILITY_LAYER_GLASS`, i.e. every ordinary shadow-casting light. Contradicts the CPU gate's own doc justification: "the compute pass burns max_lights TLAS ray queries per flagged pixel, so the gate has to stay tight" — a bound the shader does not enforce.
- **Impact**: Unbounded (up to `lightCount`) shadow-ray cost per caustic-source pixel in dense interior lamp clusters where most lamps are occluded. Blast radius limited by how few pixels carry the flag, further masked today by REN-D14-NEW-01 and REN-D11-2026-08-12-01. No frame-time claim made — not measured.
- **Related**: #922; REN-D14-NEW-01 and REN-D11-2026-08-12-01 (fixing those increases the pixel count reaching this loop — sequence accordingly).
- **Suggested Fix**: Charge the budget for the traversal, not the acceptance — move `processedLights++` above the visibility block, or add a separate `tracedLights` counter — and correct the `is_caustic_source` doc.

## Completeness Checks
- [ ] TESTS: A regression test pins this specific fix (frame-time/ray-count telemetry if feasible)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2775
