**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-07.md` (INC-1). Adjacent to #6727, which tracks FNV only.

- **Severity**: MEDIUM (per-game floor)
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs` — `is_fxlight_sprite_placement` (caller in the same file)
- **Changed in**: commit 3bb692bbb

## Description
`is_fxlight_sprite_placement` now returns `false` for any model whose path contains `fxlightbeam` and that has no LIGH data, so the placement loads its NIF instead of becoming a bare synthetic child. The predicate takes no game input. The commit's justification is FNV-specific (14 `FXLightBeamSuperWide01` in Vault11a; converter coverage measured on FNV meshes only), and it claims a beam the converters decline "is still dropped from the raster by the `IsFxMesh` texture rule".

## Evidence
A name scan of the shipped archives finds `fxlightbeam01.nif`, `fxlightbeam02.nif` and more in `Oblivion - Meshes.bsa`, and `fxlightbeam01..05.nif` in FO3's `Fallout - Meshes.bsa`, as well as FNV. Skyrim SE has none. `IsFxMesh` is set by `texture_path_is_fx_mesh` (`byroredux/src/components/render.rs`), a texture-path substring test (`effects\fx*`, `fxlightrays`, ...). The audit did NOT check which texture paths the Oblivion/FO3 beam NIFs use, and did not render either.

## Impact
If an Oblivion or FO3 beam NIF's texture path misses that substring list, the card now draws as real alpha-blended geometry (and the converters, measured on FNV only, may decline it): a visible content change in interiors that previously showed nothing there. If the path does match, the cost is one extra entity plus one NIF import per placement.

## Suggested Fix
Gate the exemption on the game variant (FNV only until measured), or run a `fnv_baked_beam_converter_coverage`-style census on Oblivion and FO3 and add a per-game assertion to `baked_beam_cards_load_their_nif_while_fxlight_sprites_stay_synthetic`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked for other `fx*` model families that share the substring test
- [ ] **CANONICAL-BOUNDARY**: Any per-game gating lives at the import/loader boundary, not in shaders or the renderer
- [ ] **TESTS**: A per-game regression test pins this specific fix
