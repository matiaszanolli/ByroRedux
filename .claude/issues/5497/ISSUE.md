# #5497: FO4-2026-10-09-D1-02: FO4's "Color Remapping Index" (the palette row) is dropped on both carriers, the precombine per-instance scale and MSWP `CNAM`

**Labels**: bug, esm-plugin, game:fo4, legacy-compat, medium, renderer

**Source**: `docs/audits/AUDIT_FO4_2026-10-09.md` — finding `FO4-2026-10-09-D1-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. The defect is visual only (wrong colours). No crash and no state loss.
- **Dimension**: 1 (precombine texturing), plus 2 and 4 for the MSWP decode.
- **Location**:
  - Precombine:
    - `crates/nif/src/blocks/extra_data.rs:843` parses `BSPackedGeomDataCombined.grayscale_to_palette_scale`.
    - `crates/nif/src/import/precombine.rs:275` and `:322` lift it onto `PrecombineInstance`.
    - `byroredux/src/cell_loader/precombined.rs:864-868` drops it: `into_imported_mesh(&inst.transform, …)`.
    - `merge_precombine_materials` (`:918-945`) → `merge_bgsm_arm` then sets the BGSM's scale unconditionally
      (`byroredux/src/asset_provider/material/merge.rs:1268-1274`).
  - MSWP:
    - `crates/plugin/src/esm/records/mswp.rs:21` and `:54-61` decode `CNAM` as `color_intensity`, documented as an
      "intensity / colour multiplier".
    - Its only reference outside the parser is a test literal (`byroredux/src/cell_loader/spawn/mesh_instance.rs:2298`).
- **Status**: NEW.
  - #3927 (closed) established that `grayscaleToPaletteScale` is the palette row.
  - #1455 (closed) forwarded the BGSM value.
  - Neither carries the per-placement override.
- **Description**:
  - `triangle.frag:1368-1392` samples the greyscale LUT at `v = grayscaleToPaletteScale`. The row selects the colour
    variant.
  - FO4 authors the per-placement row in two places:
    - xEdit FO4 `MSWP` (`wbDefinitionsFO4.pas:12448`) defines `wbFloat(CNAM, 'Color Remapping Index')`.
    - The CK bakes that row into each precombine instance's `grayscale_to_palette_scale`.
  - The engine keeps neither. Every placement renders the BGSM's default row.
- **Evidence**:
  - Probe `palette` (all `_oc.nif` in MeshesExtra and four DLC Main archives; BGSMs from `Materials.ba2` and the DLC Main
    archives; leaf BGSM):
    - 176,542 bakes and 2,867,238 instances.
    - 119,299 instances sit on a palette-enabled BGSM (`grayscale_to_palette_color` plus a greyscale texture).
    - **65,526 of those (15,549 objects in 3,092 files) author a row different from the one rendered.** Examples:
      `cratelarge01.bgsm` 1.0 vs 0.493, and `machinekitquad03.bgsm` 1.0 vs 0.805.
  - REFR side (`Fallout4.esm`):
    - 73 MSWPs carry `CNAM`.
    - **12,302 REFRs** point at one. 10,153 of them are XCRI-baked and carried by the per-instance value; **2,149 are not
      baked**.
    - The top swaps are identity swaps whose only payload is the row:

      | Swap | Row | REFRs |
      |---|---|---|
      | `MachineKitGray01` | 0.805 | 6,684 |
      | `MachineKitBlack01` | 0.711 | 1,880 |
      | `MachineKitRed01` | 0.945 | 1,618 |
      | `MachineKitWhite01` | 0.852 | 927 |
      | `MachineKitBlueLight01` | 0.664 | 358 |

    - The per-instance 0.805 seen on `machinekitquad03` equals `MachineKitGray01`'s `CNAM`. It is the same datum.
- **Impact**: palette-tinted set dressing collapses to one colour: machine kits, crates and hi-tech panels. The 25-row
  station-wagon and 11-row brick atlases noted at `triangle.frag:1371-1375` lose their per-placement variation.
- **Related**: #3927, #1455, #973 (MSWP per-shape apply), and FO4-2026-10-09-D1-01.
- **Suggested Fix**:
  - **Precombine**: snapshot each mesh's per-instance row before the merge in `merge_precombine_materials`, and restore
    it after, just as the blend triple already is. Both routes share that function.
  - **MSWP**: rename `color_intensity` to a colour-remap index. Carry it on the REFR overlay and apply it to the swapped
    shape's `grayscale_to_palette_scale` after its BGSM merge, still before `translate_material`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
