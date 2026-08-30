# #3639: FO4-2026-08-30-D2-01: smoothness == 1.0 with no resolvable gloss map pins roughness at the 0.04 floor (<=425 vanilla materials)

**Source**: `docs/audits/AUDIT_FO4_2026-08-30.md` — Dimension 2
**Severity**: LOW
**Location**: `byroredux/src/asset_provider/material.rs` (`roughness_override = (1.0 - smoothness).clamp(0.04, 1.0)`), `crates/renderer/shaders/triangle.frag` (the `mat.glossMapIndex != 0u` branch)

## Description

BGSM `smoothness == 1.0` lowers to `roughness = 0.04` — the clamp floor. The shader is built
to modulate that back up from the gloss map (`roughness = mix(1.0, roughness, glossTexel.r)`),
but only when `mat.glossMapIndex != 0u`. Materials whose modulating map is missing stay
pinned at 0.04, i.e. near-mirror dielectric.

## Evidence

Current code (verified 2026-08-30):

```rust
let roughness = (1.0 - leaf.smoothness).clamp(0.04, 1.0);
material.roughness_override = Some(roughness);
```

```glsl
if (mat.glossMapIndex != 0u) {
    ...
    } else {
        roughness = mix(1.0, roughness, glossTexel.r);
    }
}
```

MEASURED over the installed FO4 material corpus (9,023 BGSM/BGEM files, **all version 2**,
zero parse failures):

- **6,203 of 8,330 BGSMs (74.5%) author `smoothness == 1.0`** — the Bethesda Material Editor
  default (`MaterialLib/BGSM.cs`, tooltip "Smoothness of the specular effect", with the
  per-texel data living in the `SmoothSpec` slot).
- Of those, **345 author no leaf `smooth_spec_texture`**, and **80 more name a DDS absent
  from all 15 texture archives** → **≤425 materials (5.1%) end at roughness 0.04 with
  nothing to modulate them.**
- Upper bound only: a template parent can still supply the slot via `resolved.walk()`.

Affected materials are concentrated on hair, eyeballs and creature glow.

## Impact

Up to 425 materials render as near-mirror dielectrics. The translation itself faithfully
mirrors the source authoring — the gap is the absence of a neutral fallback for the case
where the modulating map is unavailable.

(This candidate was originally framed as "74.5% of FO4 materials read as mirrors"; that
premise was narrowed to ≤5.1% by measurement before filing, since 94.7% of BGSMs do author a
`smooth_spec` map.)

## Suggested Fix

When `smoothness == 1.0` and no `smooth_spec` role resolves after the full `resolved.walk()`,
fall back to a neutral roughness rather than the 0.04 floor — the same "no modulating map
available" branch the shader already implies.

## Related

#1476 (saturation metalness), #1241 / #1244 (PBR seeding).

## Completeness Checks
- [ ] **SIBLING**: the metalness half of the same merge (`bgsm_metalness`) has its own map-absent case — check it for the same shape
- [ ] **CANONICAL-BOUNDARY**: the fix belongs at the BGSM merge / `translate_material` boundary or in `Material::resolve_pbr`, never as a render-time fallback in `triangle.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins one of the 425 measured materials resolving to the neutral value, not 0.04
