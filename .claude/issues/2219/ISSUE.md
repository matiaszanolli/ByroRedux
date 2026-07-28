# #2219 — REN-2026-07-28-02: skinned RT secondary-hit normals reconstructed from undeformed bind-pose vertices

_Filed from `docs/audits/AUDIT_RENDERER_2026-07-28.md` by `/audit-publish` on 2026-07-28. Immutable snapshot of the issue **as filed** — GitHub is authoritative for current state (`gh issue view 2219 --json state`)._

---

**Severity:** MEDIUM · **Dimensions:** 2 (SSBO/ray queries), 9 (GPU skinning/BLAS refit), 15 (water), 17 (Disney PBR)
**Source:** `docs/audits/AUDIT_RENDERER_2026-07-28.md` — REN-2026-07-28-02
**Status when filed:** Existing implementation gap, previously unfiled

## Description

For skinned meshes the BLAS correctly follows the *deformed* geometry, but every
secondary-ray consumer reconstructs the hit normal from the **undeformed bind-pose**
global vertex data. A ray therefore intersects the correct animated surface *position*
but shades it with a normal from a different pose.

## Evidence

`crates/renderer/shaders/include/ray_hit.glsl:39-56` — `getHitTriNormal` reads the
global bind-pose `vertexData` SSBO and applies `GpuInstance.model`:

```glsl
vec3 v0 = vec3(vertexData[p0], vertexData[p0 + 1], vertexData[p0 + 2]);
vec3 v1 = vec3(vertexData[p1], vertexData[p1 + 1], vertexData[p1 + 2]);
vec3 v2 = vec3(vertexData[p2], vertexData[p2 + 1], vertexData[p2 + 2]);
vec3 w0 = (hi.model * vec4(v0, 1.0)).xyz;
...
return normalize(cross(w1 - w0, w2 - w0));
```

`crates/renderer/shaders/skin_vertices.comp` writes position-only, compute-skinned
**absolute-world** vertices into the per-entity BLAS input — a different buffer from the
one `getHitTriNormal` reads.

`crates/renderer/src/vulkan/acceleration/predicates.rs:60-72` documents the split
explicitly, and names the consequence:

> `getHitTriNormal` (triangle.frag) needs it to rotate the bind-pose vertices it reads
> from the global vertex SSBO into world space for the RT hit-normal. **That bind-pose
> normal approximation is a separate M29 concern**, untouched here.

Consumers: `traceReflection`, `traceShadowTransmittance`, and direct calls in
`crates/renderer/shaders/triangle.frag` for glass interfaces, refraction exits, and
secondary-hit lighting.

## Impact

Moving limbs, cloth-like deformation, and animated glass produce wrong Fresnel,
reflection lighting, transmission loss, and refraction direction. The error scales with
how far the animated pose departs from bind pose, so it is worst exactly where skinning
is most visible.

## Suggested Fix

Give the shared RT hit helper access to deformed triangle positions, or a deformed normal
stream keyed by the same skinned instance. Two constraints the chosen representation must
respect:

- preserve the compact 12-byte BLAS position input, and
- not apply the entity transform twice (skinned BLAS geometry is already absolute-world,
  which is why `tlas_instance_transform` emits identity for it).

## Verification

Needs a RenderDoc capture on a strongly animated actor beside glass or a reflective
surface — this is not a change to validate from static reasoning.

## Completeness Checks
- [ ] **SIBLING**: All three consumers updated together — `traceReflection`,
      `traceShadowTransmittance`, and the direct `triangle.frag` glass/refraction calls
- [ ] **DROP**: If a new deformed-normal buffer is added, its Vulkan teardown is
      reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] Confirm the entity transform is not applied twice for skinned instances
