# NIF-SD-01: `NifImportRegistry`'s 2,048-entry LRU cap is below the Fallout 4 default-radius working set, so the streaming worker's "already cached" skips turn into main-thread re-parses

Labels: medium,nif-parser,nif,performance,game:fo4,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6114

**Severity**: MEDIUM · **Dimension**: Allocation Hygiene (pre-parse / registry contract) · **Games**: Fallout 4 (and any title with a large exterior ring or heavy mod load order; FO3 peaks at 1,587 of 2,048)

## Location
`byroredux/src/cell_loader/nif_import_registry.rs` (default cap `2048`; `insert` eviction); `byroredux/src/streaming/pre_parse.rs` (`pre_parse_model_skip_reason` skip filters and dispatch-batch memo); `byroredux/src/cell_loader/references/synth_child.rs` (inline parse on a registry miss)

## Description
At dispatch, `step_stream` queues every missing cell of the ring at once with one shared `cached_keys` snapshot. The worker then skips any key that is in the snapshot or in the dispatch-batch memo. The memo explicitly relies on "a memoed key is resident when a later apply needs it". Nothing protects that residency from the registry's own LRU: each `finish_partial_import` insert evicts the smallest-tick entry once `len > 2048`. When the ring's unique-key count exceeds the cap, a key parsed for an early cell is evicted before a later cell that the worker skipped it for is applied. That cell then misses at spawn and `synth_child.rs` extracts and parses the NIF inline on the main thread - the cost the worker exists to remove. The NIF was already parsed once, so the work is paid twice.

Fallout 4 adds a second source of pressure: every `_oc.nif` precombine is keyed by a per-cell-unique path (`precombine_oc_nif_paths`) and goes through the same cache. They are never shared between cells, yet they enter the LRU as the newest entries and push the shared statics out.

## Evidence
Census of unique base models per radius-12 window (`DEFAULT_EXTERIOR_RADIUS = 12`; base ESM only):

| Game | Max | Mean | Cap 2048 binds? |
|---|---:|---:|---|
| Oblivion (Tamriel) | 926 | 604 | no |
| Fallout 3 (Wasteland) | 1,587 | 969 | no (77% at peak) |
| Fallout NV (WastelandNV) | 846 | 265 | no |
| Skyrim SE (Tamriel) | 1,535 | 1,050 | no |
| Fallout 4 (Commonwealth) | 9,674 | 4,276 | yes, in 3,587 of 4,707 window centres |

Registry simulation (cap 2048, closest-first apply order, worker batch memo): centre (0,0) gives 9,582 worker parses and 26,353 main-thread fallback parses with base models only, 33,860 including per-cell precombine entries. With cap 12,000 the same run gives 0. This is a model, not an engine measurement; the `mesh.cache` console command (`evictions`) and the per-cell "NIF cache hits/misses" log line in `complete.rs` would confirm it on a device.

## Impact
On the default FO4 exterior load (boot, fast travel, door exit into the Commonwealth), a large share of cell spawns hit inline parses on the main thread, inflating boot time and adding frame hitches inside `ExteriorCellApplyJob`. FNV, Oblivion and Skyrim never reach the cap in vanilla. Workaround: `BYRO_NIF_CACHE_MAX=0` or a large value.

## Related
#5512 (salvage), #3705 (clip release under LRU pressure), #1398 (uncapped registry), `docs/engine/memory-budget.md` ("NIF Import Cache"), PERF-SD-2026-10-19-02 (different cache), NIF-SD-03. Capacity-related like EXT-D6-2026-10-19-01 / REN-SD-2026-10-19-01 but a different cache and fix site (`NifImportRegistry`, not the geometry pool).

## Suggested Fix
Do not let the LRU evict a key a queued request still depends on: pin the snapshot and memo keys until their cell applies (or have the worker re-emit a key when the registry reports it missing at apply time). Separately, keep per-cell-unique `_oc.nif` entries out of the shared 2,048-entry LRU, or size the cap from the ring (`radius_load`, game) rather than a constant.

Source: docs/audits/AUDIT_NIF_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
