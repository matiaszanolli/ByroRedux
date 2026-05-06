# FNV Compatibility Audit — 2026-05-04 (Dimension 3 only)

**Scope**: Dimension 3 — Cell loading end-to-end (`byroredux/src/cell_loader*.rs`).
**Other dimensions**: not run (`/audit-fnv --focus 3`).

## Executive Summary

The cell loader is **clean**. Four prior FNV-D3 findings (#626 / #627 /
#632 / #635) all landed with regression tests pinning each fix. M40
streaming is wired end-to-end (`WorldStreamingState` → `step_streaming`
→ `unload_cell` / `load_one_exterior_cell`) with hysteresis,
generation counters, and an off-thread parse worker. 162 byroredux
tests pass; FNV.esm parses to 73,054 records in 8 s; smoke launch into
`GSProspectorSaloonInterior` loads cleanly (461 REFRs → 803 entities,
0 panics).

**No NEW code findings.** Two LOW documentation drifts only — no code
regressions, no half-shipped feature surface.

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 2 (docs only) |
| INFO     | 0 |

## Audit-prompt premise check

Per `feedback_audit_findings.md`, two stale references in the prompt:

- **"Roadmap claims 809 entities"** is false. ROADMAP.md:138 says **1200**;
  cell_loader.rs:865 doc-comment says **809 REFRs**; live cell-loader log
  prints **803 entities** for `entity_count`. The string "809" comes from
  the audit prompt's own copy of the cell_loader.rs comment (which is
  itself stale — see FNV-D3-DOC-02).
- **"`load_cell` for interior, `load_grid` for exterior"** outdated. Current
  API: `load_cell_with_masters` (interior) and `build_exterior_world_context`
  + `load_one_exterior_cell` (exterior, streamed). Single-master `load_cell`
  retired in M46.0 / #561 in favour of always-multi-master entry points.

These don't change scope; the entity-count cross-check below uses the
real numbers.

## Dimension 3 — Cell Loading

### Checklist results

| # | Item | Result |
|---|------|--------|
| 1.1 | Prospector Saloon REFR count vs roadmap | **PASS** — 461 REFRs (parser ground truth) |
| 1.2 | Cell-loader `entity_count` for the cell | **PASS** — 803 entities (NPCs + statics + LIGH + collisions + sub-mesh) |
| 1.3 | Scene-ready high-water-mark (`world.next_entity_id`) | INFO — 2,345 (includes UI/particle/NPC body parts; ROADMAP "1200" is closer to live cell-scope count) |
| 1.4 | XCLL/LGTM lighting fallback chain | **PASS** — `resolve_cell_lighting` (#566), 5 pin tests in `cell_loader_lgtm_fallback_tests.rs` |
| 1.5 | NiAlphaProperty decal routing | **PASS** — `render_layer_with_decal_escalation`, 6 tests in `cell_loader_refr_texture_overlay_tests.rs` |
| 2.1 | LAND mesh per cell | **PASS** — 33×33 grid, Z-up→Y-up |
| 2.2 | LTEX/TXST splatting | **PASS** — dedup by `ltex_form_id`, 8-cap, 7 splat tests |
| 2.3 | WTHR → CLMT → WTHR resolution chain | **PASS** — `build_exterior_world_context` resolves all three with logged sunrise/sunset |
| 2.4 | M33 cloud texture resolution | **PASS** — symmetric drop on unload (#626) |
| 2.5 | Reference-count consistency across cell loads | **PASS** — `unload_cell` walks all texture/mesh/normal/dark/extra/terrain handles |
| 3.1 | Entities despawned cleanly on cell transition | **PASS** — `CellRoot` query → `world.despawn` |
| 3.2 | NIF imports kept alive only as long as needed | INFO/LOW (known) — process-lifetime by design (#381); LRU cap opt-in via `BYRO_NIF_CACHE_MAX` (#635) |
| 3.3 | Texture descriptors released | **PASS** — refcounted (#524) with symmetric `drop_texture` |
| 3.4 | Memory leak hot-spots | **PASS** for shipped paths (#626, #627, #495) |
| 3.5 | Unshipped-feature leak surface | **PASS-INFO** — M40 explicit unload now lands; no leftover unshipped hooks |
| 4.1 | `CachedNifImport` Arc-cached, same key → same Arc | **PASS** — `pending_new` + batched commit (#523) |
| 4.2 | Cache invalidation on cell unload | INFO — process-wide by design |
| 4.3 | Cache scope process-wide | **PASS** (#381) |
| 5.1 | `CellLoadResult` exposes `WeatherRecord` | **PASS** — exterior path inserts via `apply_worldspace_weather` |
| 5.2 | `CellLoadResult` exposes `lighting` | **PASS** — `resolve_cell_lighting` always called |
| 5.3 | Terrain mesh + splat data | **PASS** |
| 6.1 | Multi-master / DLC chain handling | **PASS** — duplicate-plugin guard + master-not-in-load-order error |
| 6.2 | FormID remap correctness | **PASS** — `parse_esm_with_load_order_remaps_self_form_ids` test |
| 6.3 | Worldspace auto-pick (#444) + `--wrld` override | **PASS** — priority chain |
| 6.4 | `tex.missing` debug command | **PASS** — present (closed Session 27's chrome-posterized walls) |
| 7.1 | Real-data smoke launch | **PASS** — 0 panics, 5 stat misses logged with FormID + plugin name (#561) |

### Findings

#### FNV-D3-DOC-01: ROADMAP entity-count claim is misaligned with cell-loader telemetry

**Severity**: LOW (documentation drift)
**Dimension**: Cell Loading
**Location**: `ROADMAP.md:138` ("Prospector 1200 entities") ↔ `byroredux/src/cell_loader.rs:52` ("should produce 784 here on every load") ↔ live `entity_count` log = 803 (smoke run today)
**Status**: NEW

Three different "Prospector entity count" numbers across the codebase
reflect three different definitions:

- `entity_count` returned by the loader = REFR-spawned entities. **803** today.
- `mesh_count` (the 784 in the doc-comment) = entities that received a `MeshHandle` insert.
- ROADMAP's "1200" looks like an older snapshot — possibly bench-commit `6a6950a` (172.6 FPS / 5.79 ms), counting support entities the bench fixture creates.

Audit prompt asked us to flag >5% divergence; 803 vs 1200 is a definition mismatch (not a regression — the smoke ran cleanly).

**Fix**: Update ROADMAP.md to either say "461 REFRs / 803 entities" with the cell-loader-log definition, or clarify "1200" is post-fixture render-data count (camera + UI + particle quads + NPC body parts). Reconcile the "should produce 784" comment in `cell_loader.rs:52` against the real 803.

#### FNV-D3-DOC-02: cell_loader.rs:865-866 cites "Prospector Saloon's 809 REFRs" — real count is 461

**Severity**: LOW (documentation drift inside code comments)
**Dimension**: Cell Loading
**Location**: [byroredux/src/cell_loader.rs:865-866](byroredux/src/cell_loader.rs#L865-L866)
**Status**: NEW

The #523 batched-commit comment cites 809 REFRs as the motivating workload. Real REFR count for `GSProspectorSaloonInterior` from FalloutNV.esm is **461** (verified via `cargo run -p byroredux-plugin --example dump_cell_refs`).

The lock-batching argument doesn't depend on the exact number; future maintainers will run `dump_cell_refs` and conclude either the cell or the parser is broken when neither is.

**Fix**: One-line edit. Replace "Prospector Saloon's 809 REFRs that was 809 write-lock cycles" with "Prospector Saloon's 461 REFRs that was 461 write-lock cycles" (or paraphrase as "every REFR took a write lock" without a specific number).

### Existing-issue cross-references (skip-don't-refile)

All landed and regression-tested:

- **#626** SkyParamsRes cloud + sun texture refcount leak. Test: `texture_indices_enumerates_all_five_slots`.
- **#627** Terrain splat-layer texture refcount leak.
- **#632** ESM-fallback LightSource fires even with zero-color NIF placeholder. Test: `placeholder_only_array_counts_zero_so_esm_fallback_fires`.
- **#635** `NifImportRegistry` LRU cap + nested PKIN expansion. Tests: `lru_cap_evicts_least_recently_inserted_entry`, `lru_eviction_drops_clip_handle_for_victim`.
- **#637** / FNV-D5-LOW bundle (D5-01..04) — observability + docs polish.
- **#561** Multi-master `_with_masters` entry points. Verified live.
- **#566** / SK-D6-02 — LGTM lighting-template fallback.
- **#584** REFR texture overlay (XATO/XTNM/XTXR).
- **#585** SCOL placement expansion.
- **#589** PKIN placement expansion.
- **#544** Embedded animation clip handle memoisation.
- **#523** Batched commit of NIF cache hits/misses.
- **#524** Refcounted texture handles.
- **#477** / FNV-3-L2 — `mesh_count` separate from `entity_count`.
- **#444** Worldspace auto-pick + `--wrld` override.
- **#445** FormIdRemap.
- **#372** `CellRoot` stamping for `unload_cell`.
- **#381** Process-lifetime `NifImportRegistry`.
- **#382** Batched BLAS submit for terrain.
- **#386** Bounded sample of unresolved REFR FormIDs.
- **#401** Particle emitter spawn from `NiParticleSystem`.
- **#463** Climate TNAM sunrise/sunset hours.
- **#476** Climate weather sentinel `chance < 0`.
- **#478** CLMT FNAM sun-sprite.
- **#495** BLAS scratch shrink on unload.
- **#609** StringPool interning of overlay slots.
- **#672** `light_radius_or_default` sanitiser.
- **#732** / LIFE-H2 — frames-in-flight teardown ordering.
- **#783**, **#784** Sibling BSA auto-load + chrome-posterized fix. Verified live.
- **#803** Cloud scroll across cell transitions.

### Verified correct (no finding)

- Z-up Bethesda → Y-up renderer conversion in `euler_zup_to_quat_yup` (5 unit tests).
- Streaming hysteresis (`radius_unload = radius_load + 1`).
- Streaming generation counter prevents stale-payload application.
- Cell unload ordering: terrain tile slots freed FIRST (#470 frames-in-flight), BLAS dropped, mesh buffers, texture handles, despawn last.
- All FNV.esm parsing flows clean: 73,054 records / 8 s; 388 interior cells; 30,096 exterior cells; 14 worldspaces; 18,320 statics; 88 LTEX; 31 climates; 63 weathers.
- Smoke launch (`GSProspectorSaloonInterior`) clean: 0 panics, 5 stat misses logged with origin (FormID + plugin name per #561).
- Test density per concern: 18 cell-loader test files, ~95 unit tests (REFR overlays, SCOL/PKIN expansion, LGTM fallback, terrain splat, NIF cache LRU, animation clip memoisation, sky params cleanup, streaming deltas, hysteresis, Z-up→Y-up).

## Suggested next step

```
/audit-publish docs/audits/AUDIT_FNV_2026-05-04_DIM3.md
```

The two findings are documentation-only. Per `feedback_speculative_vulkan_fixes.md`, no Vulkan changes were proposed.
