# #1591 — FO4-D2-MEDIUM-01: Conductor diffuse-tint blend folds in specular_mult, contradicting #1476 mult-invariance

**Severity**: MEDIUM · **Dimension**: BGSM/BGEM Consumption
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D2-MEDIUM-01)
**Location**: `byroredux/src/asset_provider.rs:1147-1149,1174-1185`

## Description
When saturation-derived `metalness > 0.5`, the merge biases `mesh.diffuse_color` 50/50 toward the authored conductor spec — but it blends toward `spec_r/g/b = specular_color * specular_mult` (the mult-scaled values). The #1476 fix deliberately excludes `specular_mult` from the metalness derivation because `mult` only scales highlight strength and is not an albedo/F0 quantity. Folding `mult` into the diffuse tint reintroduces exactly that error on the conductor population: for `mult < 1.0` the blend darkens diffuse toward black; for `mult > 1.0` it overshoots past 1.0 (no clamp at translate or upload — `diffuse_color` flows verbatim into `GpuMaterial.diffuse_r/g/b`).

## Evidence
Real vanilla strong-metal BGSMs sampled via `dump_bgsm`: `spec=[1.0,0.255,0.255] mult=0.25` → blend target `[0.25,0.064,0.064]` (near-black); `spec=[1.0,0.467,0.318] mult=1.29` → blend target `[1.29,0.60,0.41]` (channel > 1.0, unclamped). The chromaticity the comment intends to recover is `specular_color`, not `specular_color * mult`.
```rust
let spec_r = leaf.specular_color[0] * leaf.specular_mult;  // :1147
// ...
if metalness > 0.5 {
    mesh.diffuse_color = [
        0.5 * mesh.diffuse_color[0] + 0.5 * spec_r,         // :1181
        ...
    ];
}
```

## Impact
Wrong conductor albedo/F0 tint for the ~0.3% strong-metal + part of the 3.7% mid-metal BGSM population (brass/gold/copper/painted-metal trim with non-unit `specular_mult`). Half-weight blend + F0-only use bounds the visual error; not a chrome-scale regression. Loose-NIF and cell paths both affected. FO4 BGSM conductor content only.

## Related
#1476 (commit `08ed03be`) metalness mult-invariance.

## Suggested Fix
Blend toward the mult-free chromaticity — use `leaf.specular_color` (already `[0,1]`-bounded) rather than `spec_r/g/b` at `:1180-1184`, and/or clamp the blended diffuse to `[0,1]`. Keep the mult-bearing `spec_*` for the `leaf.pbr==true` F0-luminance path where mult-as-scale is correct.

## Completeness Checks
- [ ] **SIBLING**: Same `* specular_mult` fold checked in the loose-NIF path and any other diffuse-tint site
- [ ] **CANONICAL-BOUNDARY**: Fix stays in the BGSM-merge / `translate_material` boundary; per-game conductor logic is not pushed into shaders/renderer or re-derived at render time
- [ ] **TESTS**: A regression test pins the mult-free conductor tint (and the `[0,1]` clamp) for `mult<1` and `mult>1` cases
