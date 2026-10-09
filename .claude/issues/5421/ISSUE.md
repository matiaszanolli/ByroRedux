# #5421: ESM-2026-10-08-D2-02: REGN `CNAM` "climate" decode has no xEdit definition and zero vanilla occurrences; REGN `WNAM` is documented as a weather form but is the Worldspace and is not remapped

**Labels**: low,esm-plugin,terrain-exterior,bug,game:oblivion
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5421

**Source**: `docs/audits/AUDIT_ESM_2026-10-08.md` — `ESM-2026-10-08-D2-02` (HEAD `00f580e09`)

**Publish note**: validation confirmed the `CNAM` arm and the `weather_form` doc, but REGN `WNAM` *is* routed through `remap_fid` by the #3401 post-pass (`crates/plugin/src/esm/records/misc/world.rs` — `out.weather_form = out.weather_form.map(|f| remap_fid(f, remap));`), so the "read without `remap_fid`" / "not remapped" half of the finding does not hold. The remaining fix is the rename to `worldspace_form` + the CNAM arm/doc.

- **Severity**: LOW. The region-climate step is inert on vanilla, and the only `weather_form` reader is an example probe.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `REGN` / `CNAM`, `WNAM`
- **Location**:
  - `crates/plugin/src/esm/records/misc/world.rs:835-844` (field docs) and `:977-987` (arms).
  - Consumer: `byroredux/src/cell_loader/exterior.rs:2015`.
  - `crates/plugin/examples/cell_climate_probe.rs:26,55,76`.
- **Status**: NEW. The `CNAM` arm arrived in b4497ec1d; the `WNAM` decode is older (6d889d70d, pre-baseline).
- **Description**: xEdit TES4 / FO3 / FNV / TES5 `REGN` defines `WNAM` as `wbFormIDCkNoReach(WNAM, 'Worldspace', [WRLD])` and has **no `CNAM`**. The new `climate_form` doc says Oblivion's climate reaches cells through "CELL `XCLR` → REGN `CNAM` → CLMT" and that SI regions carry the link. `weather_form` is documented as "weather form that this region enforces" and is read without `remap_fid`.
- **Evidence** (`scripts/regn.py`):
  - `Oblivion.esm` (GOTY, with SI merged in; `DLCShiveringIsles.esp` is an 85-byte stub) has 211 REGNs, **0 `CNAM`**, and 210/210 `WNAM` → WRLD.
  - A whole-file scan for any CLMT FormID finds references only in CELL `XCCM`, WRLD `CNAM`, SCPT, PGRD and LAND. No REGN references a climate.
  - `FalloutNV.esm` has 0 REGN `CNAM`.
- **Impact**: None on vanilla. The decode reads a sub-record nothing authors (*feedback_no_guessing*), the doc teaches a false data path, and a multi-master load hands the probe an unremapped worldspace id labelled as a weather.
- **Related**: #3314 (the CELL/WRLD remap family, closed); `/audit-exterior` owns the climate resolution.
- **Suggested Fix**:
  - Drop the `CNAM` arm and its chain step, or keep it with a "no vanilla authoring; xEdit undefined" note.
  - Rename `weather_form` to `worldspace_form` and route it through `remap_fid`.
  - Fix the commit-era doc claim.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other REGN sub-records with no xEdit definition; `cell_climate_probe` example)
- [ ] **TESTS**: A regression test pins this specific fix
