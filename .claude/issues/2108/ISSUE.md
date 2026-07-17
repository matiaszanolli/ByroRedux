# SF-D9-01: EFFECT_PALETTE_COLOR/ALPHA derived from LUT-texture presence, not the authored palette-enable flag

**Severity**: MEDIUM
**Labels**: medium, renderer, legacy-compat, bug
**Location**: `byroredux/src/asset_provider/material.rs:790-793` (BGSM), `:1001-1008` (BGEM); `byroredux/src/cell_loader.rs:244-250` (packer)
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF-D9-01)

## Description
The packer sets `EFFECT_PALETTE_COLOR`/`ALPHA` whenever `bgsm_greyscale_lut_path.is_some()` — populated purely on the greyscale texture *slot* being non-empty, with no reference to the authoritative `grayscale_to_palette_color` enable flag (parsed, zero consumers elsewhere). This is asymmetric with the inline NIF effect-shader path (`pack_effect_shader_flags`), which correctly gates the same flag on the real SLSF enable bit. A BGSM/BGEM that fills the greyscale slot but leaves the remap flag off is given a palette LUT remap it should not receive.

## Impact
Wrong diffuse colors (unwanted palette-LUT remap) on any BGSM/BGEM material with an authored-but-disabled greyscale slot — likely on inherited-from-template slots and mis-authored mod content. Blast radius narrow for vanilla but silent and cross-game (FO4/FO76/Starfield-BGEM).

## Suggested Fix
Forward the parsed `grayscale_to_palette_color` (BGEM: `|| grayscale_to_palette_alpha`) bool onto a new `ImportedMesh` enable field and gate the flag pack on it, mirroring the inline-path gate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
