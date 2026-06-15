# #1610 — NIF-D4-01: tint_map/inner_layer_map dropped at ImportedMesh handoff

_Filed from `docs/audits/AUDIT_NIF_2026-06-14.md` via /audit-publish. Immutable snapshot as-filed; GitHub is authoritative for current state._

**Severity**: LOW (latent — both texture slots are currently renderer-unconsumed; no visible artifact today) · **Dimension**: Geometry/Import Handoff · **Status**: NEW (undocumented residual of CLOSED #563 / SK-D3-02 — the fix's "bindless indices to GpuInstance" half never landed)
**Source**: AUDIT_NIF_2026-06-14 (NIF-D4-01)
**Game Affected**: Skyrim / FO4 (FaceTint + MultiLayerParallax shader types; `BSShaderTextureSet` slot 7).

**Location**: [import/material/walker.rs:198-204,225-231](crates/nif/src/import/material/walker.rs#L198-L231) (captures `tint_map` / `inner_layer_map` onto the internal `MaterialInfo`); [import/types.rs](crates/nif/src/import/types.rs) (`ImportedMesh` has no field for either).

## Description
The material walker extracts the FaceTint tint map and the MultiLayerParallax inner-layer map into `MaterialInfo`, but `ImportedMesh` carries no field for them and no extractor forwards them, so they are silently dropped at the import boundary. Same bug class as the historically-fixed #214/#430/#451/#1076 "captured-but-dropped" findings.

## Evidence
`walker.rs:198-204` / `:225-231` assign the two fields; `grep -n 'tint_map\|inner_layer_map' crates/nif/src/import/types.rs` → no match; no consumer in `material_translate.rs`.

## Impact
None today (no renderer consumer reads either slot). Latent: when FaceTint / MultiLayerParallax rendering is implemented, the data is already parsed but won't reach the GPU.

## Related
CLOSED #563; NIFAL boundary (`material_translate.rs` — cross-link `/audit-nifal`, do not duplicate the per-game material classification there).

## Suggested Fix
Either add `tint_map` / `inner_layer_map` to `ImportedMesh` and forward them through the walker (so the data survives to a future consumer), or drop the unused `MaterialInfo` captures and re-add when a consumer lands — and note the deferral on #563.

## Completeness Checks
- [ ] **SIBLING**: Check the other `MaterialInfo` texture slots for the same captured-but-not-forwarded pattern
- [ ] **CANONICAL-BOUNDARY**: If the fix forwards the slots through `material_translate.rs` (`translate_material`), per-game classification stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins `tint_map`/`inner_layer_map` surviving the `ImportedMesh` handoff
