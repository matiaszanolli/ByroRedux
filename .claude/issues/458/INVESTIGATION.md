# #458 — WATR/NAVI/NAVM/REGN/ECZN/LGTM/HDPT/EYES/HAIR stubs

## Finding
All 9 record types fall through `records/mod.rs:276-278` catch-all `reader.skip_group(&group)`.

## Scope
- Single new module `crates/plugin/src/esm/records/misc.rs` with 9 small parsers
- 9 new `Index::*` hashmaps on `EsmIndex` (lines 53-85)
- 9 new match arms on `parse_esm`'s group dispatch (line 177+)
- 9 new type re-exports
- Regression tests: synthetic record + real FNV.esm parse-count check

## Decisions

- **Keep extraction minimal.** Issue says "EDID + relevant form-ID refs, no sub-record deep parse." LGTM lives closest to XCLL's data layout (`cell.rs`); reusing `CellLighting` here would couple two tickets. Go with a lean `LgtmRecord { form_id, editor_id, ambient_rgb, directional_rgb, fog_color_rgb, fog_near, fog_far, directional_fade, fog_clip, fog_power }` — same field surface as `CellLighting`'s FNV prefix. The per-field inheritance fallback stays with #379.

- **WATR damage / NAVM geometry / REGN point data** — skip. Stubs capture only what rename-dangling-refs would need (EDID + form refs + the one or two scalar fields a caller would actually branch on).

- **Reuse `read_f32_at` / `read_form_id` / `read_zstring`** from `common.rs` / `cell.rs`. No new helpers.

## Schema references (nif.xml / xEdit / UESP)

| Record | Minimal shape | Notes |
|---|---|---|
| WATR | EDID, FULL, TNAM texture, NNAM shader | `XCWT` form refs this |
| NAVI | EDID, NVER u32 version | Master nav list |
| NAVM | EDID, NVER u32, parent cell form ref | Per-cell nav mesh |
| REGN | EDID, WNAM weather form, RCLR color u8[3] | Region definition |
| ECZN | EDID, DATA (owner-form u32, rank u8, flags u8, min-level u8) | Encounter zone |
| LGTM | EDID + XCLL-shaped DATA | Ties to #379 |
| HDPT | EDID, FULL, MODL model path, DATA flags | Head part |
| EYES | EDID, FULL, ICON texture path, DATA flags | Eyes |
| HAIR | EDID, FULL, MODL model path, ICON texture path, DATA flags | Hair |
