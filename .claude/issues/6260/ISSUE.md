# #6260: SF-2026-10-21-D4-02: Pack-in template children drop their own `XLMS` layered material swaps; 2,107 of Cydonia's 12,372 instanced children render the un-swapped material

**Labels**: medium,bug,esm-plugin,import-pipeline,game:starfield,legacy-compat

**Source**: docs/audits/AUDIT_STARFIELD_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 4 (ESM + cell bring-up) · **Location**: `byroredux/src/cell_loader/refr.rs:633-690` (`expand_pkin_placements_with_depth`: template references are reduced to `(base_form_id, pos, rot, scale)` at line 671)

## Location
- `byroredux/src/cell_loader/refr.rs:633-690` (`expand_pkin_placements_with_depth`: template references are reduced to `(base_form_id, pos, rot, scale)` at line 671)
- `byroredux/src/cell_loader/references/mod.rs:551-604` (one `refr_overlay`, built from the *outer* REFR, shared by every synthetic child)

## Status at filing
NEW. This is a residual of #5757 (closed 2026-10-21) and #5489 (closed). The skill records that "per-child state beyond the base/transform (XPCS, XMSP, enable parents) is not carried". That was dormant for material swaps until #5757 made `XLMS` live, and Starfield's per-child swaps are `XLMS`, not `XMSP`.

## Description
- #5757 decodes `XLMS` and applies the flattened LMSW entries per shape through the REFR's texture overlay.
- For a pack-in, `build_refr_texture_overlay` runs once on the outer REFR and that overlay is handed to every synthetic child. So the pack-in's own `XLMS` does reach its contents (164 Cydonia pack-in REFRs author one).
- The template cell's references are re-placed from `template.references` with only `placed.base_form_id` and the composed transform, so each child's own `layered_material_swap_refs` is discarded.
- In Starfield these template-cell references are where most kit recolouring is authored.

## Evidence
- **Game-wide** (`probe/pkin_xlms.py` over `Starfield.esm`): 11,281 PKIN template cells hold 473,299 REFRs, and **111,026** of them carry `XLMS` (of 689,041 `XLMS` REFRs total).
- **Cydonia `citycydoniamainlevel` (0x2B3DA2)** (`probe/cydonia_pkin_xlms.py`): 370 pack-in REFRs expand (recursively, skipping initially-disabled) to **12,372** leaf children. That matches the engine's own #5533 count exactly. **2,107** of those children author an `XLMS` that is dropped.

## Impact
- 17% of Cydonia's pack-in content renders with its default kit material instead of the authored layered swap. Examples are the colour-coded industrial trim and painted-metal variants, the same class of visual #5757 fixed for top-level references.
- The loss is silent: no log line, and `sf_smoke` counts these children as resolved.

## Related
#5757, #5489, #5533. The same carry gap applies to FO4 SCOL/PKIN `XMSP`, but FO4 pack-ins have no template cells.

## Suggested Fix
- Carry the template child's `layered_material_swap_refs` (and its `material_swap_ref`) alongside the transform in the expansion tuple.
- Build each child's overlay as the outer overlay with the child's own entries merged in. Choose and document the order: outer-after-child (outer wins) or the reverse. The order is unsourced, so pin it as a decision rather than a fact.
- Extend the `#[ignore]`d `starfield_cydonia_pack_ins_instance_their_template_cells` guard to count children carrying swaps.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
