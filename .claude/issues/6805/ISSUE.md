# ESM-D7-05: the "typed but unconsumed" inventory is incomplete (`media_sets`, `weather_settings`; VOLI/GDRY annotated only in `index.rs`)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6805
Labels: low, esm-plugin, documentation, doc-rot
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Location**: `docs/engine/esm-records.md:438-451`; `crates/plugin/src/esm/records/index.rs:443,547,557,565,570` (the five #6180 annotations), `:627-628` (`media_sets`, no annotation), the `weather_settings` doc (#5364)
- **Status**: NEW
- **Description**: the premise "seven maps" is out of date by design: ENCH, REPU, CSTY, IPCT, IPDS remain (5); BPTD left in #6350/#6451 and EXPL/COBJ in #6564. I re-derived the set. A field-by-field sweep of all 110 `HashMap` fields (reads outside `records/`, tests, examples and comments) leaves 33 with no production reader. 5 are the annotated ones, 2 (`volumetric_lighting`, `god_rays`) are annotated "consumed by nothing (§10 Q8)" in `index.rs` but absent from the esm-records.md list, and about 23 are `MinimalEsmRecord` EDID stubs (covered by the memory-budget caveat). Two **typed** parsers are in neither list: `media_sets` (MSET, `parse_mset`; the doc comment says "what REGN RDSI/RDSB point at" in the present tense, `asset_provider/audio.rs:293-296` confirms no playback policy reads it) and `weather_settings` (Starfield WTHS, #5364; no reader). The re-check grep at `esm-records.md:448` also still lists `body_parts`, which now always matches `systems/vats.rs:423`, so the command no longer isolates the unconsumed set.
- **Impact**: documentation only; no behaviour change. Parsers stay (cross-ref resolution, census tooling).
- **Suggested fix**: list `volumetric_lighting`, `god_rays`, `media_sets`, `weather_settings` with the five; add the #6180-style annotation to `media_sets` and `weather_settings`; drop `body_parts` from the grep.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
