**Severity**: HIGH (currently masked by a workaround that disables normal mapping entirely)
**Source**: 2026-05-01 live debug session, traced + confirmed by capturing normals visualization (BYROREDUX_RENDER_DEBUG=4) on FNV `GSDocMitchellHouse` interior.

## Symptom

After fixes for #782 (interior fog leak) + per-light ambient fill tuning (commit 3a2d837), most surfaces rendered correctly but **specific architectural surfaces (interior walls, floor planks)** had a chrome / posterized / "white-grey-black variance" look with **sharp visible cuts at mesh boundaries**.

## Root cause

The engine has no per-vertex tangents in the [Vertex struct](../../tree/main/crates/renderer/src/vertex.rs). [`perturbNormal`](../../tree/main/crates/renderer/shaders/triangle.frag#L530-L568) reconstructs the TBN basis per-fragment from screen-space derivatives:

```glsl
vec3 dPdx = dFdx(worldPos);
vec3 dPdy = dFdy(worldPos);
vec2 dUVdx = dFdx(uv);
vec2 dUVdy = dFdy(uv);
vec3 T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y);
vec3 B = normalize(dPdy * dUVdx.x - dPdx * dUVdy.x);
```

At a mesh boundary (two floor planks meeting, two wall panels adjoining), `dFdx` samples cross the boundary. The UV derivative jumps (UV space is discontinuous between meshes), `T` and `B` directions flip arbitrarily, the perturbed normal flips on the two sides of the seam, and every lighting term (diffuse, specular, indirect AO) differs visibly. The visual signature: hard cuts at floor planks, chrome posterized walls, and high-frequency normal noise on otherwise-flat surfaces.

Visualizing normals via the existing `BYROREDUX_RENDER_DEBUG=4` debug flag produced exactly the diagnostic image — adjacent floor planks rendered yellow vs cyan vs lavender despite all sharing world-space-up as their interpolated vertex normal, confirming the TBN basis is per-screen-quad-different.

## Diagnostic confirmation

Commenting out the `perturbNormal` call at [triangle.frag:719](../../tree/main/crates/renderer/shaders/triangle.frag#L719) (replacing the perturbed normal with the raw vertex normal) **completely eliminated the chrome look and hard mesh-boundary cuts**. Surfaces lost their fine bump detail but rendered with correct lighting. That commit is the temporary workaround currently shipped — see the same line for the disabled call.

## Path forward

**M-NORMALS milestone**: parse Bethesda-encoded per-vertex tangents from NIF and feed them to the shader instead of computing TBN per-fragment.

### Subtasks

- [ ] **NIF parser**: route the tangent + bitangent vectors that Bethesda ships in `NiBinaryExtraData("Tangent space (binormal & tangent vectors)")` (Skyrim+/FO4 standard) and the older NiTangentData blocks (FO3/FNV) into `ImportedMesh`. Both formats present 3 floats per vertex per channel; encoding sign conventions vary per game.
- [ ] **Vertex struct**: add `tangent: [f32; 4]` (xyz tangent + w bitangent sign). Pushes per-vertex stride from 84 B (21 floats) → 100 B (25 floats). Updates `Vertex::attribute_descriptions()`, `VERTEX_STRIDE_FLOATS` in skin_compute.rs, the `gpu_instance_size_*` test, and the documentation across [triangle.vert](../../tree/main/crates/renderer/shaders/triangle.vert) + [triangle.frag](../../tree/main/crates/renderer/shaders/triangle.frag) + [skin_vertices.comp](../../tree/main/crates/renderer/shaders/skin_vertices.comp).
- [ ] **Shaders**: vertex shader passes `inTangent` + `inBitangentSign` as varying outputs; fragment shader's `perturbNormal` consumes them instead of computing dFdx/dFdy.
- [ ] **Skinned meshes**: `skin_vertices.comp` skins the tangent through the bone palette alongside position + normal. Sign of bitangent persists.
- [ ] **Re-enable `perturbNormal` call** at [triangle.frag:719](../../tree/main/crates/renderer/shaders/triangle.frag#L719) once the new TBN source is wired.
- [ ] **Reference**: pre-Bethesda content without authored tangents (rare in supported games but possible for legacy-port content) needs a graceful fallback. Current screen-space derivative path can be retained as that fallback, gated on `tangent.xyz == vec3(0)`.

## Stretch — full Oblivion-beating quality

Per-vertex tangents alone gets us to "lighting matches what Bethesda's renderers produced." Once that lands, several stretch refinements push past:

- BC5 normal-map sample quality (anisotropic filtering, mip-bias for distant surfaces — distant chrome was a TBN artifact, but mips on normal maps need their own tuning)
- Proper energy-conserving PBR (revert b803b29's `/PI` removal once authored brightness can be compensated globally)
- LIGHT-N2 fix (display-space fog) — separate issue
- TAA history rejection refinements (#737 was the SVGF nearest-tap fallback; TAA itself uses a similar logic that can probably tighten)

## Acceptance

A side-by-side of the user's 2026-05-01 reference shot against the same camera angle in the rebuilt engine shows: **no chrome posterization on walls, no hard cuts at floor mesh boundaries, fine bump detail visible on stone / fabric / metal surfaces** — i.e., the Path A endpoint of the original investigation.

## Context

This is a **HIGH-priority milestone** because the current workaround (perturbNormal disabled) lands a regression on visible surface detail. Visible bumpiness on stone walls / fabric / engraved metal is a defining feature of Bethesda content; without it, every interior looks visually flat compared to vanilla. Once M-NORMALS lands, the renderer hits the visual quality bar the audit roadmap has been pointing toward — the lighting model is correct, surfaces have proper detail, and the framework is in place for Oblivion-level (and beyond) interior fidelity.
