# #5243 — WATAL W2-01: distant LOD water is a single worldspace-default-height sheet — ignores per-cell XCLW (phantom ocean on 15,962 authored-dry FNV cells; distant Lake Mead 4,900 BU underwater)

https://github.com/matiaszanolli/ByroRedux/issues/5243

Source: W2 evidence pass 2026-10-04 (W0/W1 captures + `xclw_census.rs` five-game census). Full body on the issue.

**WATAL W2 — the first defect chosen by evidence from the W0/W1 captures, per the selection rule "coverage/seam before local shading polish"** (`docs/engine/playable-vertical-slice.md` step 3).

- **Severity**: HIGH (visual correctness on every exterior vista in 4 of 5 audited worldspaces)
- **Dimension**: W2 distant-water coverage — default-water / CELL override / LOD seam, all three at once.
- **Location**:
  - `byroredux/src/streaming.rs:962` — `spawn_lod_water` passes only the WRLD `NAM3` height to the annulus builder; no per-cell data reaches it.
  - `byroredux/src/cell_loader/water.rs:866-931` — `build_lod_water_frame` emits one flat worldspace-wide annulus at that single height, with a translated hole.
  - `byroredux/src/streaming.rs:987-1007` — `recenter_lod_water` translates the entity; the mesh is never rebuilt.
- **Description**: The distant LOD water annulus ignores the per-cell XCLW tri-state entirely. Every exterior cell resolves its own water height as **override → that height; authored dry sentinel → no water; absent → worldspace default** — and vanilla draws distant water per cell at that effective height. We collapse all of it to one flat sheet at the worldspace default, which the census shows is almost never the right answer:
- **Evidence** (XCLW census over installed masters, `crates/plugin/examples/xclw_census.rs`):

  | Worldspace | default | inherit | dry-sentinel | override | wet-at-distance at default |
  |---|---:|---:|---:|---|---:|
  | FNV WastelandNV (16,396 cells) | −2300 | **0** | 15,962 | 434 (Lake Mead 412 @ 2600; ~350 wet) | **0** |
  | FO3 Wasteland (38,463) | 10500 | 34,835 | 3,346 | ~170 | 2,954 |
  | Skyrim Tamriel (11,186) | −14000 | **0** | 10,689 | ~500 (rivers/coast, nearly all wet) | **0** |
  | FO4 Commonwealth (36k+) | 450 | **0** | 36,319 | ~150 | **0** |
  | Oblivion Tamriel (14,686) | 0 | 14,563 | 0 | ~123 (lakes @ 500..5200+) | coastal thousands |

- **Live proof** (FNV Lake Mead, grid (19,13), radius 3, byro-dbg elevated captures, 2026-10-04):
  - `render.debug water_term` from 3400 BU above the lake looking away (135°): a bright continuous water sheet runs from midground to horizon across dry desert, with hard linear boundaries where the flat −2300 plane intersects rising terrain — **phantom water over 15,962 authored-dry cells' basins**.
  - Looking across the lake (45°): near-field water is correct (per-cell 2600 planes), but distant Lake Mead beyond the streaming hole renders at −2300 — 4,900 BU below its own basin rim — so the lake effectively vanishes at distance (**coverage gap**).
  - `m-exteriors.sh fnv water` stays green throughout: its frozen poses are near-field, so the gate never sees the annulus.
- **Impact**:
  - Every FO3/FNV/Skyrim/FO4 exterior vista: phantom ocean in dry basins where terrain dips below the worldspace default, and missing/underwater distant lakes and rivers wherever cells override the default.
  - The hole edge is a 4,900-BU vertical step on FNV (2600 → −2300) wherever the streamed cells override.
  - Oblivion is the least-affected worldspace (99% inherit), which is why the W0 Oblivion captures never surfaced it.
- **Suggested Fix**: replace the single-height annulus with a per-cell distant-water mesh: one quad per distant cell at its **effective** height (override → that height; absent → worldspace default; dry sentinel → skipped), confined to the ring between the streaming hole and the terrain-LOD reach, optionally masked by the cell's LAND min height (skip quads the terrain fully occludes). Worst case measured: FO3's ~3,000 contiguous default-height quads (~1.2 MB mesh). The translated-hole trick becomes a rebuild-on-grid-crossing (the mesh registry swap the terrain LOD ring already does every reconcile). `water.dump` should print the distant mesh's height histogram.

## Completeness Checks
- [x] **TESTS**: a pure geometry-builder test over synthetic tri-state cells (override/dry/inherit, hole radius, LOD reach) asserts the exact quad set; an `#[ignore]`d real-data test asserts WastelandNV distant water contains 2600 quads for known Mead cells, zero quads at −2300, and no dry-sentinel cells
- [x] **SIBLING**: `recenter_lod_water` rebuilds instead of translating; the mesh-handle release/re-register path matches `unload_lod_water_plane`'s contract
- [x] **LIVE**: elevated 135° `water_term` phantom sheet gone; 45° vista shows distant Mead at 2600; `m-exteriors.sh fnv water` green
- [x] **DOCS**: watal.md §5.2 and the `LodWaterPlane` doc state the per-cell effective-height model

## Resolution (2026-10-04)

Per-cell distant water landed: `build_distant_water_mesh` (tri-state XCLW, streaming-hole window, terrain-LOD reach, LAND-occlusion mask) + `rebuild_lod_water_mesh` on grid crossing. Live-verified on FNV Lake Mead: the 135° phantom sheet is gone and the 45° vista shows the distant lake continuous to its shores at 2600 (`water.dump` histogram: 2600×262 leading, no −2300 bucket). `m-exteriors.sh fnv/skyrim water` green.
