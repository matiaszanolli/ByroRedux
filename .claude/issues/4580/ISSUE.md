# REN-D1-2026-09-21-02: `TRIANGLE_FACING_CULL_DISABLE` is inert — no ray query sets a facing-cull flag, so #416's premise and `tlas.rs`'s "RT honors two_sided / ~2× ray cost" comment are false

**Labels**: low, renderer, vulkan, documentation, doc-rot

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (documentation/premise drift; no runtime change is wanted) · **Dimension**: AS Correctness
**Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs` (~:591-604): the `TRIANGLE_FACING_CULL_DISABLE` gate on `draw_cmd.two_sided` and its comment. Also every `rayQueryInitializeEXT` under `crates/renderer/shaders/`.
**Status**: NEW
**Verified against**: HEAD `f97775ca8`

## Description

`VK_GEOMETRY_INSTANCE_TRIANGLE_FACING_CULL_DISABLE_BIT_KHR` matters only when a ray asks for facing culling with `gl_RayFlagsCullBackFacingTrianglesEXT` or `gl_RayFlagsCullFrontFacingTrianglesEXT`. No ray query in the shader tree does. Every query uses `gl_RayFlagsOpaqueEXT`, sometimes with `| gl_RayFlagsTerminateOnFirstHitEXT`. So every ray already sees both faces of every triangle, whether or not the instance sets the bit.

The comment on the gate makes two claims, and neither is true:
- that pre-#416 "every instance disabled backface culling, so shadow / GI rays hit the interior backfaces of closed single-sided meshes … ~2× ray cost";
- that "the RT path now honors the same bit".

Toggling the bit changes nothing. Both-face visibility is also why single-sided walls block light from behind, which bears on the `f97775ca8` light-leak investigation.

## Evidence

- `grep -rhoE "gl_RayFlags[A-Za-z]+EXT" crates/renderer/shaders/` finds only `gl_RayFlagsOpaqueEXT` (17 sites) and `gl_RayFlagsTerminateOnFirstHitEXT` (6 sites). `CullBackFacing` and `CullFrontFacing` appear nowhere.
- In `tlas.rs`, `let instance_flags = if draw_cmd.two_sided { TRIANGLE_FACING_CULL_DISABLE } else { 0 };` sits under the comment quoted above.

## Impact

This is documentation and audit-premise drift only. Anyone reasoning from the comment, or from #416, about RT back-face behaviour, ray cost, or two-sided leak mechanics gets the wrong model. The audit-renderer skill repeats the premise ("a blanket enable is the regression (~2× ray cost)").

## Related

- #416 (closed): introduced the gate on this premise.
- REN-D1-2026-09-21-01 (#4576): the shadow-mask policy change whose investigation leaned on this premise.

## Suggested Fix

Correct the `tlas.rs` comment, and the Dim-1 premise in `.claude/commands/audit-renderer/SKILL.md`. Both should say the bit is inert: no ray query requests facing culling, so RT always sees both faces. Do **not** add cull flags to the ray queries. That would open leaks through single-sided room shells, which today block light from both sides.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D1-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: the audit-renderer skill's Dim-1 line (`TRIANGLE_FACING_CULL_DISABLE … ~2× ray cost`) corrected in the same sweep
- [ ] **TESTS**: optional — a source-shape pin that no shader requests `gl_RayFlagsCull*FacingTrianglesEXT` without the comment being revisited
