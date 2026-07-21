# REN-RESTIR-01: ReSTIR-DI reservoir surface tag truncates the now-unbounded surface_id to 22 bits

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2117
**Labels**: bug, renderer, medium

**Severity**: MEDIUM (one reviewing sub-agent independently assessed this as LOW — see note below; kept at MEDIUM as the more cautious of two independent takes, given the shared root cause with #2116)
**Dimension**: Ray Queries / Denoiser (ReSTIR-DI surface identity)
**Location**: `crates/renderer/shaders/triangle.frag` — `RESERVOIR_SURFACE_MASK = 0x3FFFFFu` (22 bits, line ~71), the `uint surfaceId = inst.surfaceId & RESERVOIR_SURFACE_MASK;` reuse site (~line 2478).
**Status**: NEW — reported independently by two audit sub-agents, same root cause, re-confirmed directly against the current tree.

**Description**: The reservoir's surface-identity tag was switched (commit `883f57cd`) from `fragInstanceIndex + 1` (bounded by `MAX_INSTANCES = 0x40000`, comfortably under the 22-bit field) to `inst.surfaceId & RESERVOIR_SURFACE_MASK`, where `surface_id = entity_id + 1` is unbounded across a session (entity IDs are never recycled — see `crates/core/src/ecs/world.rs::spawn()`). Past ~4.19M cumulative spawns, two distinct live surfaces can alias onto the same 22-bit tag, letting the spatial-reuse pass mis-accept a neighbour reservoir belonging to a different surface. The adjacent in-source comment justifying the field width ("comfortably above MAX_INSTANCES") is now factually stale. Separately, the mesh-ID/TAA-SVGF path uses the full 31 bits for the same `surface_id`, so above 2^22 spawns the reservoir and mesh-ID paths would disagree on surface identity for the same fragment.

**Impact**: Visual-only (direct-shadow bleed across aliased coplanar surfaces at the aliasing threshold), no crash/corruption. Realistically self-correcting via the per-sample final visibility ray, and requires a multi-hour session to reach the 2^22-spawn regime — hence MEDIUM rather than HIGH despite sharing a root cause with #2116.

**Related**: #2116 (identical unbounded-`surface_id` root cause, different consumer). Introduced by `883f57cd`.

**Suggested Fix**: Update the stale comment to state the tag now holds `entity_id + 1` (unbounded, aliases every 2^22 spawns, self-correcting). Optionally hash `surface_id` into 22 bits rather than truncating, or widen the reservoir's surface field if bits can be spared from the light index.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the surface-identity source and its collision bound (or documents it as an accepted long-session limitation)

Filed from `docs/audits/AUDIT_RENDERER_2026-07-20.md`.
