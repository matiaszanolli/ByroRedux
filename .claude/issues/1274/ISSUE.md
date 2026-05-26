# REN-D16-2026-05-26-01: Anisotropic-GGX TBN omits Gram-Schmidt at triangle.frag:2500/2740

## Severity: Low (latent today)

**Location**:
- `crates/renderer/shaders/triangle.frag:2500-2501` (fallback-directional specular path)
- `crates/renderer/shaders/triangle.frag:2740-2741` (per-light specular path)

Both sites were introduced by #1250 (`c0374d00` — anisotropic GGX path for brushed-metal / hair / vinyl authoring).

## Problem

When `mat.anisotropic > 0`, the anisotropic specular sites reconstruct a tangent frame from `fragTangent` but **omit the Gram-Schmidt orthogonalization** that `perturbNormal` Path-1 performs. Result: two different tangent frames coexist on the same fragment — one used for normal-map sampling (orthogonalized) and one used for the anisotropic GGX lobe orientation (tilted).

### Evidence

```glsl
// perturbNormal Path-1 (lines 933-944) — Gram-Schmidt before B:
vec3 T = normalize(vertexTangent.xyz);
T = normalize(T - dot(T, N) * N);          // ← orthogonalize against N
vec3 B = vertexTangent.w * cross(N, T);

// Anisotropic-GGX specular sites (lines 2497-2501, 2737-2741) — no Gram-Schmidt:
if (mat.anisotropic > 0.0
    && dot(fragTangent.xyz, fragTangent.xyz) > 1e-4)
{
    vec3 T = normalize(fragTangent.xyz);
    vec3 B = normalize(cross(N, T)) * fragTangent.w;  // ← T may not be ⟂ N
    float HdotX = dot(H, T);
    float HdotY = dot(H, B);
    ...
}
```

## Impact

When the per-vertex authored T is not exactly perpendicular to the interpolated per-fragment N (normal across smoothing-group seams — the very condition Gram-Schmidt was added to handle in `perturbNormal`), the anisotropic specular projection uses a tilted T while the normal-map sample uses an orthogonalized T. The anisotropic GGX lobe orientation is slightly off-axis relative to the bump-mapped normal.

**Visually**: subtle directional shift on hair / brushed-metal / hair-card surfaces near smoothing-group seams.

**Currently latent**: `mat.anisotropic == 0` on every legacy NIF — #1250 added the path in preparation for hair/brushed-metal but no authored anisotropy flows in yet. Will manifest only when authored anisotropy lands (BGSM v22+ or synthetic hair-card paths).

## Fix

Mirror `perturbNormal`'s Gram-Schmidt at both sites. Single-line addition between the `normalize(fragTangent.xyz)` and the `cross(N, T)` reconstruction:

```glsl
vec3 T = normalize(fragTangent.xyz);
T = normalize(T - dot(T, N) * N);          // ← add this
vec3 B = normalize(cross(N, T)) * fragTangent.w;
```

After Gram-Schmidt, T is unit-length and ⟂ N, so `cross(N, T)` is already unit-length — the inner `normalize` on the B line becomes redundant but harmless.

## Architectural Recommendation

**Extract a `mat3 buildTBN(vec3 N, vec4 vertexTangent)` helper.** Four TBN-reconstruction sites in `triangle.frag` are drifting independently:

| Site | Path | Gram-Schmidt? | UV-mirror sign? |
|---|---|---|---|
| `perturbNormal` Path-1 (`:933-944`) | authored-tangent normal mapping | ✓ | n/a |
| `perturbNormal` Path-2 (`:954-970`) | screen-space derivative fallback | n/a | ✓ (after #1104) |
| Anisotropic specular fallback-directional (`:2497-2501`) | per-light specular lobe | ✗ (this finding) | n/a |
| Anisotropic specular per-light (`:2737-2741`) | per-light specular lobe | ✗ (this finding) | n/a |

#1104 was a bug because Path-2 drifted from Path-1's UV-mirror convention. This finding is the same class of bug — Path-1 has Gram-Schmidt, the anisotropic paths don't. Consolidating to one helper would have prevented both.

## Completeness Checks

- [ ] **TESTS**: Visual regression on an anisotropic-authored material with a smoothing-group seam (hair card, brushed-metal weapon). Pre-fix: directional shift across seam. Post-fix: continuous.
- [ ] **SIBLING**: Both anisotropic sites (`:2500-2501` + `:2740-2741`) get the Gram-Schmidt; the two `perturbNormal` paths are already correct.
- [ ] **ARCH**: Consider the `buildTBN` helper extraction in the same PR to lock the convention across all 4 sites.
- [ ] **SPIRV**: Recompile `triangle.frag.spv` after the GLSL change (per the shader-compilation step in `CLAUDE.md`).

## Related

- #1104 (closed) — Path-2 UV-mirror sign drift from Path-1, same class of bug
- #1250 (closed) — anisotropic GGX path introduction (`c0374d00`); this is a follow-up gap from that landing
- `feedback_shader_struct_sync.md` — shader-struct lockstep contract; relevant cross-cutting hazard

Audit: `docs/audits/AUDIT_RENDERER_2026-05-26_DIM16.md` (REN-D16-2026-05-26-01)
