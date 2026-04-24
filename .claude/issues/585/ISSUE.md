# #585: FO4-DIM4-01: Cell loader never expands SCOL placements — mod-added SCOLs disappear

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/585
**Labels**: bug, high, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 4 / Dim 6)
**Severity**: HIGH
**Location**:
- Unconsulted map: `crates/plugin/src/esm/cell.rs:426` (`pub scols: HashMap<u32, ScolRecord>`)
- Missing consumer: `byroredux/src/cell_loader.rs:1152-1210` (REFR base resolution)

## Description

Closed #405 (`FO4-D4-C2: SCOL body not parsed`) landed the parser side — `parse_esm_cells_with_load_order` now populates `EsmCellIndex.scols` with every `(ONAM, DATA)` child placement. The cell-loader side never followed: `rg -ni scol byroredux/src/` returns **zero** hits. The REFR spawn path queries only `index.statics.get(&base_form_id)`; when a REFR's base FormID is a SCOL, the lookup misses and the record silently falls through.

`crates/plugin/src/esm/records/scol.rs:26-29` documents "the cell loader expands an SCOL REFR into N synthetic placed refs when the cached `CM*.NIF` isn't present. See the `scol_parts` field on `crate::esm::cell::StaticObject`." The referenced `scol_parts` field **does not exist** on `StaticObject` (verified at `cell.rs:330-347`). See follow-up FO4-DIM4-04.

## Evidence

- `EsmCellIndex.scols` populated at `cell.rs:469` + merge loop.
- `cell_loader.rs:1152-1174`: `let stat = match index.statics.get(&placed_ref.base_form_id) { … };` — no fallback through `index.scols`.
- `cell_loader.rs:1194`: if `stat.model_path.is_empty()`, loader falls back to light-only or `continue`.

## Impact

- **Vanilla works** because 2616/2617 SCOLs ship `SCOL\Fallout4.esm\CM*.NIF` cached-combined meshes registered in `statics` via the `parse_scol_group` MODL fallback (`cell.rs:1675-1690`).
- **Mod-added SCOLs** (common for new content — previsibine bypass is author-gated) render as nothing.
- **Previsibine-bypass loadouts** (ENB, community patches) drop cached CM files → full regression to zero visibility for the 2617 vanilla SCOLs.

## Suggested Fix

In `cell_loader.rs::load_references`, after the REFR base lookup:

1. Query `index.scols.get(&placed_ref.base_form_id)`.
2. If hit **and** `statics[base].model_path` is empty (or behind a config flag), iterate `scol.parts[*].placements[*]`. For each `ScolPlacement`:
   - Compose `ref_transform × placement_transform`.
   - Resolve child `onam_form_id` through `index.statics` as a synthetic placement.
   - Euler → quaternion via `euler_zup_to_quat_yup` (already used at `cell_loader.rs:1182`; SCOL `rot` is Z-up Euler-radian per `records/scol.rs:41-42`).
   - `synth_scale = ref_scale * placement.scale`.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Analogous expansion for PKIN bundle contents when that parser lands (FO4-DIM6-05 family)
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Fixture SCOL with empty `model_path` + 3 placements → assert 3 synthetic REFRs emit with composed transforms. Covered end-to-end by `parse_real_fo4_esm_surfaces_scol_placements` corpus test (`cell.rs:3432`) once consumer side lands.

## Related

- Parser-side landed as closed #405 (FO4-D4-C2).
- Doc drift follow-up: FO4-DIM4-04.
