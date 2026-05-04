# Investigation — R-N2 / #786

**Conclusion: not shipping a fix.** Per `feedback_speculative_vulkan_fixes`, this requires a RenderDoc capture before any TBN math change ships. Analytical evidence below identifies a likely root cause; visual validation is still required to confirm.

## Analysis

### Storage convention mismatch (likely root cause)

Reading `nifly/src/Geometry.cpp:2026-2106` (`NiTriShapeData::CalcTangentSpace`):

```cpp
Vector3 sdir = ((t2*x1 - t1*x2)*r, ...);   // = ∂P/∂U
Vector3 tdir = ((s1*x2 - s2*x1)*r, ...);   // = ∂P/∂V
sdir.Normalize(); tdir.Normalize();
tan1[i] += sdir;                            // tan1 accumulator → sdir (∂P/∂U)
tan2[i] += tdir;                            // tan2 accumulator → tdir (∂P/∂V)
...
bitangents[i] = tan1[i];                    // ★ Bethesda labels ∂P/∂U as BITANGENT
tangents[i]   = tan2[i];                    // ★ Bethesda labels ∂P/∂V as TANGENT
```

This is opposite the standard Lengyel convention (T = ∂P/∂U, B = ∂P/∂V). nifly preserves Bethesda's on-disk convention so the format round-trips losslessly — Bethesda's runtime/shader knows about the swap and compensates.

Our [crates/nif/src/import/mesh.rs:264-272](crates/nif/src/import/mesh.rs#L264-L272) ports nifly verbatim:

```rust
let bitangent_zup = tan_u[i];   // tan_u = sdir = ∂P/∂U
let tangent_zup   = tan_v[i];   // tan_v = tdir = ∂P/∂V
```

So our `Vertex.tangent.xyz` carries **∂P/∂V** in Bethesda convention.

### Shader interprets it as standard convention

[crates/renderer/shaders/triangle.frag:566-601](crates/renderer/shaders/triangle.frag#L566-L601) Path 1:

```glsl
vec3 T = normalize(vertexTangent.xyz);              // T = ∂P/∂V (per import above)
T = normalize(T - dot(T, N) * N);
vec3 B = vertexTangent.w * cross(N, T);             // B ≈ ±∂P/∂U
mat3 TBN = mat3(T, B, N);                           // col 0 = T (∂P/∂V), col 1 = B (∂P/∂U)
return normalize(TBN * tangentNormal);
// = T*tn.x + B*tn.y + N*tn.z = (∂P/∂V)*tn.x + (∂P/∂U)*tn.y + N*tn.z
```

But Path 2 (screen-space derivative fallback) at [triangle.frag:609-617](crates/renderer/shaders/triangle.frag#L609-L617) uses the **standard** convention:

```glsl
vec3 T = normalize(dPdx*dUVdy.y - dPdy*dUVdx.y);    // T = ∂P/∂U
vec3 B = normalize(dPdy*dUVdx.x - dPdx*dUVdy.x);    // B = ∂P/∂V
B = cross(N, T);
mat3 TBN = mat3(T, B, N);                           // col 0 = ∂P/∂U, col 1 = ∂P/∂V
```

**The two paths produce different conventions for the same shader code.** Path 1 swaps tn.x and tn.y relative to Path 2 — equivalent to a 90° rotation of the normal-map perturbation around N. On surfaces with directional bump detail (wood grain, plaster trowel marks) this could plausibly produce the chrome / wet-look symptom.

### Timeline check matches the analysis

- `91e9011` (2026-05-02) — #783 authored-tangent path landed, **visually verified** on `GSDocMitchellHouse` fireplace (FNV).
- `82a4563` (2026-05-02) — followup added `synthesize_tangents` for FNV/FO3/Oblivion (no authored tangents).
- `77aa2de` (2026-05-03) — chrome regression reported on the same scene → workaround disables perturbNormal.

The fireplace surround in `91e9011` may have been one of the few FNV meshes with authored tangent extra-data; once the synthesize path lit up the rest of the scene, the regression appeared. Both paths put ∂P/∂V into `Vertex.tangent.xyz`, but the user only sampled the authored path on a small surface for the initial validation.

## Why I'm not shipping the fix

Two equally-plausible fix shapes:

**Shape A** — swap T/B in shader Path 1:
```glsl
mat3 TBN = mat3(B, T, N);   // recognize T = ∂P/∂V
```

**Shape B** — swap accumulator → field in import:
```rust
// extract_tangents_from_extra_data: read Bethesda's "bitangent" half as our tangent
let t_yup = [bx, bz, -by];   // was [tx, tz, -ty]
let b_yup = [tx, tz, -ty];   // was [bx, bz, -by]
// synthesize_tangents:
let tangent_zup   = tan_u[i];   // was tan_v[i]
let bitangent_zup = tan_v[i];   // was tan_u[i]
```

Either fix should produce identical math, but **the bitangent sign derivation also depends on which is which.** Sign is `sign(dot(authored_B, cross(N, T)))`. If we swap T/B definitions, the sign flips too, and `vertexTangent.w * cross(N, T)` in Path 1 ends up multiplied by `-1` relative to today.

Without RenderDoc evidence of which fragments produce chrome (and what `DBG_VIZ_TANGENT` shows red vs green on those fragments), there's no way to know whether Shape A, Shape B, or some additional sign flip is actually correct. Shipping any of them risks a fourth round of broken graphics on the user's machine — which `feedback_speculative_vulkan_fixes` exists specifically to prevent.

## Recommended user action

1. `BYROREDUX_RENDER_DEBUG=0x28 cargo run -- --esm FalloutNV.esm --cell GSDocMitchellHouse --bsa "FalloutNV - Meshes.bsa" --textures-bsa "FalloutNV - Textures.bsa"` (force-on + tangent viz).
2. Identify a chrome-affected fragment. Note its `DBG_VIZ_TANGENT` colour:
   - **green** (Path 1 fires) → almost certainly the convention-swap above. Try Shape A first; if surface details look mirrored along U or V, add a sign flip on `vertexTangent.w` in the same site.
   - **red** (Path 2 fires) → bug is elsewhere; not the convention swap. Suspect the BC5 Z reconstruction sign or mesh boundaries on synthetic content.
3. Also worth capturing: a known-good Skyrim SE mesh side-by-side, since SE ships authored tangents and would isolate Path 1 vs Path 2.

Once the colour is known, the fix is one line and can be shipped with confidence.

## SIBLING check (per issue checklist)

> verify `synthesize_tangents` in `crates/nif/src/import/mesh.rs` doesn't have a `tan_u`/`tan_v` swap (nifly port, never visually validated)

**Verified against `nifly/src/Geometry.cpp:2074-2085`.** Our import matches nifly verbatim — same accumulator labelling, same swap at output. The "bug" (if one exists here) is a *deliberate* preservation of nifly's Bethesda-convention swap, which is correct for round-tripping NIFs but wrong for our shader's standard-convention TBN construction. Fixing it requires changing import and shader together, which is what the visual validation above gates.
