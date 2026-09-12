# SF-2026-09-11-D9-01: the BGSM merge arm assigns (rather than ORs) the greyscale-palette enable bit when the BGSM fills a role the NIF left empty, silently clearing a NIF-authored SLSF1 palette remap

**Issue**: #4286 — https://github.com/matiaszanolli/ByroRedux/issues/4286
**Labels**: medium,nifal,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 9 — BGSM/BGEM External Material Flow
**Location**: `byroredux/src/asset_provider/material/merge.rs:631-641 (bgsm_greyscale_lut_enabled assignment vs OR)`
**Status**: NEW

## Description
`byroredux/src/asset_provider/material/merge.rs`'s BGSM merge arm handles the greyscale-palette enable bit two different ways depending on which side (NIF or BGSM) filled the greyscale-lut texture slot: when the NIF already filled it, the BGSM's bit is correctly OR'd in (`material.bgsm_greyscale_lut_enabled |= bgsm.base.grayscale_to_palette_color`, the #3898 fix). But when the BGSM fills a role the NIF left empty (`material.textures.greyscale_lut.is_none()`), the code plain-**assigns** the bit (`material.bgsm_greyscale_lut_enabled = bgsm.base.grayscale_to_palette_color`) instead of OR-ing — clobbering any enable bit the NIF's own SLSF1 forwarding (#3897) had already set on that same field. This is reachable on any Skyrim-layout BGSM mesh, where wire slot 3 is `Height`, not `GreyscaleLut`, so the NIF texture side can never fill the greyscale-lut role and the merge always takes the clobbering assignment branch. It is a third case #3898's "neither source may silently disable the other's remap" fix didn't cover.

## Evidence
`byroredux/src/asset_provider/material/merge.rs:631-641`: `if material.textures.greyscale_lut.is_none() { material.bgsm_greyscale_lut_enabled = bgsm.base.grayscale_to_palette_color; ... } else if nif_supplied_greyscale_lut { material.bgsm_greyscale_lut_enabled |= bgsm.base.grayscale_to_palette_color; ... }` — the `is_none()` branch (BGSM wins the slot) is a plain assignment; only the `else if` branch (NIF already won the slot) ORs.

## Impact
On any Skyrim-layout BGSM mesh where the NIF's own SLSF1 bit forwarding (#3897) already enabled the greyscale-palette remap before this merge code runs, this assignment silently clears that enable bit if the BGSM's own `grayscale_to_palette_color` happens to be false — producing wrong diffuse colors (a missing palette-LUT remap the content author intended).

## Related
Adjacent to the already-fixed #3898 ("neither source may silently disable the other's remap", which fixed the NIF-wins-slot branch but not this BGSM-wins-slot branch) and #2108 (a related but distinct earlier fix in the same area).

## Suggested Fix
Change the `is_none()` branch to OR the bit in as well (`material.bgsm_greyscale_lut_enabled |= bgsm.base.grayscale_to_palette_color`) rather than assigning, consistent with #3898's stated invariant that neither source may silently disable the other's remap.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
