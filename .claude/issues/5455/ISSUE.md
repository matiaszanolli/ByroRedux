# #5455: REN-D10-2026-10-08-01: #5249's transmission trace hands its foreign-hit leg an opaque-only mask, so glass blockers never attenuate the transmission half, contradicting the comment

**Labels**: low,renderer,shaders,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5455

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-08.md` — `REN-D10-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `mask = visibilityMask & VISIBILITY_MASK_ALL_OPAQUE` (15u, excludes `VISIBILITY_LAYER_GLASS` 16u) is passed to `traceShadowTransmittance`, whose glass loop returns early without the glass bit; the inline handoff comment also claims glass semantics. Cost side of the same trace: PERF-D5-2026-10-08-01.

- **Severity**: LOW
- **Dimension**: Soft Shadows
- **Location**: `crates/renderer/shaders/include/shadow_transport.glsl`, `traceShadowTransmittanceSkippingInstance`: `uint mask = visibilityMask & VISIBILITY_MASK_ALL_OPAQUE;` and the handoff `return traceShadowTransmittance(at, direction, remaining, emitterRadius, mask);`.
- **Status**: NEW.
- **Description**:
  - The doc comment says the first foreign hit "defers to the shared alpha/glass-aware trace … so translucent blockers keep their tint semantics".
  - `traceShadowTransmittanceDetailed` runs its glass loop only when `(visibilityMask & VISIBILITY_LAYER_GLASS) != 0u`. The mask passed down has that bit cleared, so the glass loop returns `vec3(1.0)` immediately.
  - Alpha-tested opaque blockers still keep their semantics; glass panes do not.
- **Evidence**: before #5192 the transmission lobes were multiplied by the full `selectedRayVisibilityMask` visibility. After #5249 they are multiplied by `transmissionVisibility`, which never sees glass.
- **Impact**: visual only and narrow. A back-lit surface behind a window pane (the Skyrim back-light lobe, the FO4 translucency lobe) keeps its transmission lobe untinted and unattenuated, while its reflection half is correctly attenuated by the same pane.
- **Related**: #5249, #5192, #4946.
- **Suggested Fix**: pass the full `visibilityMask` to the foreign-leg `traceShadowTransmittance` (the self-skip walk can stay opaque-only), or correct the comment to say glass is deliberately ignored.

## Completeness Checks
- [ ] **SIBLING**: Other callers of `traceShadowTransmittance` checked for a pre-masked glass bit
- [ ] **TESTS**: A regression test pins this specific fix
