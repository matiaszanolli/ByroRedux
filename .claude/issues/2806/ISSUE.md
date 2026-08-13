# REN-D17-06: specularAaRoughness missing SPECULAR_AA_THRESHOLD cap can saturate roughness to 1.0

Labels: low, renderer, bug

## Description

The `#2471` doc claims parity with Kaplanyan & Hoffman 2016 / Filament `normalFiltering()`, but two constants in the same expression are not from that reference and carry no citation: the bare `0.25` variance coefficient (Filament uses a *named* `SPECULAR_AA_VARIANCE`, default 0.15), and the **missing** `SPECULAR_AA_THRESHOLD` cap on the *added* kernel term — this shader clamps only the sum, so a high-frequency normal (foliage cutouts, chain-link, fine grating) can drive a polished surface to `roughness = 1.0` in one step, which the reference filter explicitly prevents. `grep -rn "SPECULAR_AA"` → no hits. Every neighbouring constant in the file *is* cited. Propagates into the anisotropic lobe via `deriveAxAy`. **Do not tune blind** (§7).

## Location

`crates/renderer/shaders/include/pbr.glsl` (`specularAaRoughness`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D17-06).

https://github.com/matiaszanolli/ByroRedux/issues/2806
