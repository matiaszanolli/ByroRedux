# ESM-SD-2026-10-19-03: `resolve_precombine_owner` treats the FormID slot byte as a load-order position

Labels: low,esm-plugin,bug,game:fo4,legacy-compat

## Description
`resolve_precombine_owner` (`byroredux/src/cell_loader/precombined.rs`, callers in the same file) does `let idx = (form_id >> 24) as usize; load_order_paths.get(idx)`, indexing the load-order-aligned path list by the top byte of a remapped FormID. That byte is a **slot**; `allocate_global_slot` (`load_order.rs`) draws regular and light (ESL) plugins from independent counters, so position equals slot byte only while no ESL precedes a regular plugin. FO4 supports ESL (TES4 flag `0x200`), and ESM-flagged and ESL files load before ESPs. A regular plugin after k ESLs has slot `position - k`, so its cell resolves to a different plugin's path and `<name>.esm` subdirectory; a cell owned by an ESL has top byte `0xFE`, out of range, and falls back to the active plugin at the root directory.

Regression-shaped sibling of #3366 (closed): `plugin_for_form_id` (`load_order.rs`) decodes the slot with `global_slot_of` first; this site was not touched.

## Evidence
The only tests (`precombined.rs` `resolve_precombine_owner_follows_form_id_mod_index` and neighbours) use all-regular orders (Fallout4 + DLCs, slots 0..2). `allocate_global_slot_partitions_medium_light_and_regular` and `esl_plugin_own_forms_land_in_light_space` (`load_order.rs`) show the divergence is real. Vanilla FO4 and its DLCs are all regular masters, so the vanilla gate passes; the defect needs an ESL ahead of a precombine-owning plugin (a CC `.esl` before a settlement `.esp` that bakes new cells). The failure mode is a missed `_oc.nif` lookup, which `PrecombinedSpawnJob` already treats as a miss (refs spawn individually, because `absorbed_ref_bakes` skips only refs whose bake actually spawned, #5484). Not exercised at runtime (no FO4 ESL load order here).

## Impact
Precombined cells owned by mod plugins in an ESL-bearing FO4 load order silently fall back to per-REFR rendering (draw count, BLAS count); the CSG routing (`blobs_for_cell`) can also open the wrong plugin's blob. No vanilla impact.

## Related
#3366, #1590, #2369.

## Suggested Fix
Resolve the owner through `global_slot_of(form_id)` and the slot table (`LoadOrder::slots`) instead of the raw byte; add an ESL-before-regular case beside `resolve_precombine_owner_follows_form_id_mod_index`.

Source: docs/audits/AUDIT_ESM_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
