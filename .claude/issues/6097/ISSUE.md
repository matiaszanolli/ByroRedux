# FO4-SD-02: The exterior worker pre-parses the model of every placed STAT/SCOL, including the precombine-absorbed refs the main thread skips

Labels: medium,performance,terrain-exterior,bug,game:fo4,legacy-compat

## Description
`pre_parse_cell` (`byroredux/src/streaming/pre_parse.rs`) collects the model path of **every** `cell.references` entry whose base has a `MODL` (the `for refr in &cell.references` preflight loop) and parses each unique, uncached one on the stream pool. It never consults `cell.absorbed_ref_bakes`. On FO4 the CK bakes nearly all static architecture into the cell's `_oc.nif` (XCRI), and the reference pass drops those refs (`absorbed_refs.contains(&placed_ref.form_id)` and `precombine_can_replace_record(STAT|SCOL)`, `byroredux/src/cell_loader/references/mod.rs`) before any model lookup. The worker therefore extracts, inflates, parses and imports models the cell never spawns, and the drain then runs `finish_partial_import` (`byroredux/src/streaming_helpers.rs`: BGSM merge, material resolve, registry insert) for each on the main thread's frame budget. Re-validated: `pre_parse.rs` contains no reference to `absorbed*`.

Not the same as PERF-SD-05 (per-slice `effective_absorbed_refs` rebuild), PERF-SD-03 (created-reference stepper) or #5755 (MSWP).

## Evidence
Raw GRUP walk of `Fallout4.esm`, all WRLD groups:
- 629,264 exterior STAT/SCOL placements, **599,618 (95.3 %) in an XCRI pair**; every XCRI ref is STAT/SCOL.
- Per cell, unique model paths a worker request would parse: 251,012 summed over 4,834 cells with models, versus 12,907 for refs not in the cell's XCRI set: **94.9 % are never spawned**. For the 2,716 cells with at least 20 unique models: median 56 models parsed per cell, median **0** needed.
- Globally 9,104 of 11,928 distinct placed exterior bases (76 %) have every exterior placement absorbed.

## Impact
This is the dominant per-cell worker and main-thread cost of an FO4 grid-cross: a 7x7 window (or each cell's `FinishImports` phase) parses and finishes tens of models per cell that are never drawn. The never-used entries also fill the 2048-entry `NifImportRegistry` (LRU, `byroredux/src/cell_loader/nif_import_registry.rs`), evicting genuinely reused statics and the cell's own heavy `_oc.nif` entries sooner, turning later cell loads into main-thread re-parses. Same cost on every FO4 DLC exterior. Not measured on a running engine (census of the ESM data the worker sees). The failure mode the ungated parse "protects" (a bake that fails to decode, so absorbed refs must render per-REFR) is real but rare and already has a main-thread fallback.

## Suggested Fix
In `pre_parse_cell`, build the cell's absorbed ref set from `cell.absorbed_ref_bakes` restricted to hashes whose `_oc.nif` is present in the archives (`precombine_oc_nif_paths` plus an existence/declared-size probe; no parse needed), and skip the model of a STAT/SCOL REFR in that set. Keep parsing the model when the bake's `_oc.nif` is missing, matching `effective_absorbed_refs`' un-skip rule. If bake decode failure must also un-skip, parse the precombines first (they already go first in `inputs`) and defer absorbed-ref models to a second pass only for hashes whose result came back `None`. Add a worker test that an all-absorbed cell requests only its precombine keys.

Source: docs/audits/AUDIT_FO4_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
