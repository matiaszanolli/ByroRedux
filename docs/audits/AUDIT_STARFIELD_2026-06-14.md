# Starfield Compatibility Audit — 2026-06-14

**Scope**: All 9 dimensions. Engine HEAD on `main` (post-#1510 Starfield shader over-read fix, post-#1289 CDB consumer wiring, post-#1291..#1295 walkable-Cydonia bring-up).
**Methodology**: Orchestrated 9-dimension agent fan-out (general-purpose / legacy-specialist / renderer-specialist). Every finding was re-read against the live code path and adversarially disproved before inclusion; closed-issue fixes were re-verified in place.
**Live data**: `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` PRESENT — all 5 vanilla mesh BA2s, `Starfield - Materials.ba2` (CDB), texture BA2s, and `Starfield.esm` (1.36 GB) + DLC ESMs. Real-data validation was exercised on Dimensions 1, 3, 4, 6, 7.
**Dedup baseline**: `gh issue list` → `/tmp/audit/issues.json` (300 issues); 97 Starfield-relevant filtered to `/tmp/audit/starfield/dedup.txt`. Prior report: `docs/audits/AUDIT_STARFIELD_2026-05-28.md`.

## Executive Summary

Starfield is a first-class `GameKind` and the bring-up surface is **materially healthier than the 2026-05-28 audit**. The two top blockers from that report are gone: the CDB consumer is now wired (#1289), and a walkable Cydonia interior ships (#1291–#1295). This audit confirms the regression guards hold and surfaces the **next** wave of correctness gaps — which have shifted from "geometry/parser" to "ESM base-form coverage and per-field material extraction."

State map vs the 2026-05-28 audit:

- **BA2 v2/v3 + LZ4 block** — production-correct. Exhaustive `Ba2Compression` match (zlib/LZ4/error), 12-byte v3 extension at the right offset, GNRL + DX10 unified decompress, per-chunk raw-vs-LZ4 selection. **Empirical corpus sweep: 129/129 archives OK, 0 failures.** The "undersized LZ4 max_size → heap overflow" concern was investigated and **disproven** (clean `Err(OutputTooSmall)` → `InvalidData`, no UB). 0 findings.
- **NIF parse rate** — holds and **improved**. Live sweep (all 5 vanilla mesh archives, 89 276 NIFs): **99.64% clean / 100% recoverable**, vs ROADMAP's 98.6% aggregate. Meshes01 97.21%→**100.00%** (residual tail gone); MeshesPatch 98.11%→**98.91%**. The #746/#747 truncation tail did not grow — it shrank to 325 recoverable `BSWeakReferenceNode` terrain-overlay LOD blocks in MeshesPatch only.
- **NIF shader BSVER 155+** — #1510 regression confirmed genuinely closed and byte-perfect: 189 801 + 13 713 `BSLightingShaderProperty` and 748 `BSEffectShaderProperty` blocks across Meshes01 + FaceMeshes parse with **0 NiUnknown, 0 stream drift**. CRC32 flag arrays correctly gated (`FO4_CRC_FLAGS=132`, `FO76_SF2_CRCS=152`); a name table *does* exist (`crates/nif/src/shader_flags.rs`). 0 actionable findings.
- **BSGeometry mesh extraction** — clean. All five prior fixes (#1292 `geometries\` path, #1209 all-LOD iteration, #1203 skin chain, #1232 Mikkelsen tangent fallback, #1263 push-loop consolidation) remain closed with live regression coverage. 1 LOW (cross-cutting index range-check, not SF-specific).
- **CDB materials** — parser is solid (parses the real 105 MB `materialsbeta.cdb`: 97 classes / 1 438 486 instances, no panic). Consumer is wired but **for presence-detection only** — the 1.44M authored material instances are parsed and discarded; `.mat` meshes reach the Disney lobe with NIF-keyword-*guessed* metalness/roughness (the known #1289-Phase-2 forward-blocker). 1 HIGH (cited as known), 1 MEDIUM (NEW brittleness), 1 LOW (NEW DLC path).
- **Starfield ESM resolve rate** — **first published Cydonia baseline: 88.8% (24 781 / 27 898 REFRs)** via the live `--sf-smoke` harness. ~47% of the 11.2% gap is legitimately non-mesh (audio/NPC/markers). The two *real* renderable gaps are NEW: ESM-placed **LIGH** lights never indexed (656 dropped, HIGH) and **PDCL** decals undispatched (1 846, MEDIUM). GBFM is a non-issue here (2 REFRs / 0.07%).
- **ESM + cell bring-up** — all seven spawn/cell regression guards verified still in place (XCLL-108 full decode, NAVM collection, `base_layer`-gated collider, ghost-entity BLAS exclusion, SkinSlotPool ceiling, spawn-time component stamps). 3 LOW (dead `IsCollisionOnly` marker + doc rot; decode-ahead staging; exact-108 brittleness).
- **NIFAL material translation** — `translate_material` is the single boundary; metalness/roughness are plain resolved `f32`, no surviving per-draw `classify_pbr` plumbing; `GpuMaterial` `#[repr(C)]` size-pinned (300 B), no drift; both target collision shapes translate. 0 findings.
- **BGSM/BGEM external flow** — correct and well-tested; all five prior forwarding fixes (#1454/#1455/#1453/#1358/#1353) intact, glass stuck-flag regression test passes. 1 LOW + 1 INFO (FO4-mod edge cases, no vanilla SF impact).

## Findings by Severity

| Dim | Area | CRITICAL | HIGH | MEDIUM | LOW | INFO/verify |
|-----|------|---------:|-----:|-------:|----:|------------:|
| 1 | BA2 v2/v3 LZ4 | 0 | 0 | 0 | 0 | 2 disproved |
| 2 | BSGeometry mesh | 0 | 0 | 0 | 1 | — |
| 3 | CDB materials | 0 | 1 | 1 | 1 | — |
| 4 | SF ESM resolve-rate | 0 | 1 | 1 | 2 | — |
| 5 | ESM + cell bring-up | 0 | 0 | 0 | 3 | — |
| 6 | NIF shader 155+ | 0 | 0 | 0 | 0 | 4 verify |
| 7 | Real-data validation | 0 | 0 | 0 | 1 | 3 positive |
| 8 | NIFAL translation | 0 | 0 | 0 | 0 | 6 verify |
| 9 | BGSM/BGEM flow | 0 | 0 | 0 | 1 | 1 info |
| **Total** | | **0** | **2** | **2** | **9** | |

**Counts (actionable): CRITICAL=0 HIGH=2 MEDIUM=2 LOW=9 TOTAL=13.**
(One HIGH — SF-D3-01 — is an *existing* known forward-blocker re-confirmed, not a new defect.)

---

## HIGH

### SF-D4-01: ESM-placed Starfield LIGH base records never indexed — 656 Cydonia lights silently dropped
- **Severity**: HIGH
- **Dimension**: SF ESM Resolve-Rate
- **Location**: `crates/plugin/src/esm/cell/support.rs:23-160` (`build_static_object_from_subs`), dispatched from `crates/plugin/src/esm/records/mod.rs:297-300`
- **Status**: NEW (not in dedup; #1291/#1293 cover XCLL cell ambient lighting, #1294 colliders — none cover ESM-placed LIGH *base forms*)
- **Description**: `build_static_object_from_subs` builds `light_data` for `LIGH` records by reading a `DATA` subrecord at UESP-Skyrim offsets. Starfield LIGH records carry **no `DATA` and no `MODL`** — they use a component-block layout (`BFCB`…`BFCE` wrappers around `FLCS`/`FLTR`/`FLLD`/`DAT2`/`FLBD`/`FLRD`/`FLGD`/`LLLD`/`FLAD`/`FVLD`). With no `MODL` and no `DATA`-derived `light_data`, the function returns `None`, the LIGH form is never inserted into `cells.statics`, and every REFR pointing at it misses at `references.rs:362` and is silently skipped.
- **Evidence**: Live subrecord dump of unresolved Cydonia LIGH forms: `LIGH 000027BB subs: EDID OBND ODTY BFCB FLCS BFCE BFCB INTV FLTR BFCE FLLD DAT2(76) FLBD FLRD FLGD(88) LLLD FLAD FVLD` — no `DATA`, no `MODL` (identical skeleton on `00024F71`, `0003657A`). FormID→FourCC classifier over the unresolved set: **656 LIGH REFRs across 62 distinct forms**. Distinct from the NIF-embedded `NiPointLight` path (#721), which is unaffected.
- **Impact**: Cydonia's ESM-placed interior lighting (sconces / lamps / practical lights authored as LIGH REFRs) is entirely absent — only NIF-embedded lights + XCLL ambient survive, so the cell renders markedly under-lit. Largest *functional* (renderable) contributor to the 11.2% resolve gap and a direct blocker to "Cydonia interior looks right."
- **Related**: #721 (NIF-embedded lights), SF-D4-03 (same `BFCB` component-block root cause).
- **Suggested Fix**: Add a `GameKind::Starfield`-gated LIGH decode that walks the `BFCB`/`BFCE` component blocks and extracts color/radius from `DAT2`/`FLGD`/`FLLD` (byte-audit against the Gibbed.Starfield LIGH component schema first — no guessing offsets), emitting `light_data` so the existing `references.rs:386-404` light-only spawn path lights the cell. Leave FO4/Skyrim DATA-layout LIGH untouched.

### SF-D3-01: Starfield `.mat` materials reach the Disney lobe with NIF-keyword-guessed PBR, not CDB-authored values
- **Severity**: HIGH
- **Dimension**: CDB Material Correctness
- **Location**: `byroredux/src/asset_provider.rs:1016-1028` (the `.mat` arm); `:680,712-714,733` (`sf_cdb` held but read only as a presence boolean)
- **Status**: Existing: #1290 (forward-blocker chain) / #1289 Phase 2. Cited as the known forward-blocker, NOT re-reported as new.
- **Description**: Vanilla Starfield ships every material inside the binary CDB (verified: 1 438 486 instances / 97 classes). The consumer parses and holds the full tree in `sf_cdb: Option<Arc<ComponentDatabaseFile>>` but uses it only via `has_starfield_cdb()`. The `.mat` arm flips `mesh.is_pbr = true` and returns immediately — it never indexes the CDB by material path, so it forwards zero authored fields. `metalness_override`/`roughness_override` stay `None` → become the NaN sentinel in `translate_material` → `Material::resolve_pbr`'s keyword classifier invents metalness/roughness from texture-path keywords + specular-color saturation. Textures still come from the NIF `BSGeometry` slots. Net: the Disney lobe runs (better than Lambert) but on *guessed* parameters, divergent from Bethesda's authored material.
- **Evidence**: grep shows `sf_cdb` touched only at decl/init/`is_some()`/`Some(Arc::new(cdb))` — no `.classes`/`.instances`/`.strings` read in the consumer. The `.mat` arm's own comment says "Phase 2 will walk the CDB to extract authored values." Real-data test confirms the data IS available (1.44M instances parsed) — only extraction is absent.
- **Impact**: A divergent Material crosses the single `translate_material` boundary for ALL vanilla Starfield content (the CDB is the sole material source — no loose BGSM fallback). Per the severity rubric a divergent Material out of translate = HIGH. The full 1.44M-instance authored dataset is parsed every load and thrown away.
- **Related**: #1289 Phase 2, #1290, SF-D3-03 (DLC CDB path).
- **Suggested Fix**: Implement #1289 Phase 2: (a) re-export `Class`/`Field`/`BuiltinType`/`ObjectInstance`/`Ref`/`TypeReference` from `crates/sfmaterial/src/lib.rs` (currently only `ChunkType`/`Error`/`Result`/`ComponentDatabaseFile`/`Value` are public — a walk needs `Class.fields` + `ObjectInstance` accessors); (b) build a `material_path → {metalness, roughness, texture slots, flags}` index from the `BSMaterial::*` instances at load (mirror `resolve_bgsm` forwarding); (c) look the `.mat` path up and fill the overrides before returning.

---

## MEDIUM

### SF-D4-02: Starfield `PDCL` (BGSProjectedDecal) has no dispatch arm — 1 846 placed decals silently dropped
- **Severity**: MEDIUM
- **Dimension**: SF ESM Resolve-Rate
- **Location**: `crates/plugin/src/esm/records/mod.rs:223-300` (no `b"PDCL"` arm; falls through to warn-once/skip default)
- **Status**: NEW (not in dedup)
- **Description**: `PDCL` (Gibbed `FormType.cs`: `BGSProjectedDecal`, 0x4C434450) is a Starfield-new base type with no dispatch arm and no consumer. It is the **single most frequent unresolved base type in Cydonia** (1 846 REFRs / 67 distinct forms — 59% of the unresolved count). Decals are projected onto surrounding geometry, so they would never enter the `statics` MODL path even if dispatched; a real decal-projection system would be needed to consume them.
- **Evidence**: Classifier output `PDCL 1846 REFRs 67 distinct forms` unresolved in `citycydoniamainlevel`; `grep b"PDCL"` over `crates/plugin/src/esm/` returns zero hits. Gibbed `FormType.cs`: `PDCL = 0x4C434450`.
- **Impact**: All grime/blood/poster/signage decals in Cydonia absent. Cosmetic-only (no missing collision or structural geometry) and no visible garbage (silent skip), so MEDIUM. Numerically the biggest unresolved bucket but lowest structural impact.
- **Related**: SF-D4-04 (silent-skip behaviour); the `Decal` marker in `byroredux/src/components.rs`.
- **Suggested Fix**: Defer until a decal-projection system exists. In the interim add a warned-once skip arm (the `warned_scol`/`warned_movs` pattern) so PDCL stops inflating the silent-skip count and is visible in telemetry. Do NOT route PDCL into `statics` — it has no MODL.

### SF-D3-02: Monolithic CDB parse aborts the entire material set on the first unknown chunk-type / builtin tag / class-flag bit
- **Severity**: MEDIUM
- **Dimension**: CDB Material Correctness
- **Location**: `crates/sfmaterial/src/reader.rs:153` (`ChunkType::from_raw`), `:255-258` (`UnknownClassFlags`), `:443-450` (`UnsupportedBuiltin`); `crates/sfmaterial/src/types.rs:55` (`BuiltinType::from_u32`)
- **Status**: NEW (brittleness; the *panic* concern is DISPROVEN — see Evidence)
- **Description**: The CDB is a single flat chunk stream with no per-instance recovery: `parse()` walks the whole queue and propagates the first `Err` via `?`. One unrecognised FourCC chunk type, undocumented `BuiltinType` low-byte, or any class-flag bit outside `IsUser|IsStruct` aborts the ENTIRE parse, dropping all 1.44M materials. Vanilla parses cleanly today, but the format is content-addressed and version-evolving: a future patch or a Creations/DLC CDB adding one new reflection class flag or builtin would zero out Starfield materials wholesale rather than degrading the single affected class.
- **Evidence**: `from_raw`/`from_u32` and the `UnknownClassFlags` guard all `return Err(...)`; the `while !state.chunks.is_empty()` loop uses `?` on every dispatch. Panic impact disproven: no `unwrap`/`expect`/`panic!`/`unreachable!` in the crate, and `load_starfield_cdb` (`asset_provider.rs:735-743`) catches the `Err` with `warn!` + Lambert fallback. Not a panic, not HIGH — a "lose everything on one unknown byte" brittleness.
- **Impact**: All-or-nothing CDB load. Zero impact on vanilla; on a future patch/DLC CDB with one unrecognised tag the whole material set silently falls back to keyword-guessed PBR with a single warn line — hard to diagnose because nothing names the offending class.
- **Related**: SF-D3-01, SF-D3-03.
- **Suggested Fix**: Per-instance skip is non-trivial (positional instances desync on a wrong skip). Minimum viable: include the failing chunk index / class-flag raw value in the warn message (the `Error` variants already carry `index`/`raw`), and add a `cargo test` baseline pinning the vanilla class/flag/builtin set so a new tag is caught at test time, not silently at runtime.

---

## LOW

### SF-D5-01: `IsCollisionOnly` marker is dead — never attached by any spawn path; two doc comments describe the deleted MeshHandle-piggyback pattern as live
- **Severity**: LOW
- **Dimension**: ESM + Cell Bring-up
- **Location**: `byroredux/src/components.rs:107-126`; `byroredux/src/cell_loader/precombined.rs:24-25`; query/count sites `byroredux/src/render/static_meshes.rs:135,207-210` and `byroredux/src/commands.rs:105,140`
- **Status**: NEW (#1317/#1324 cover dead code in debug-ui/sfmaterial/scripting — different scope)
- **Description**: When R6a-stale-14 converted the synthesized-trimesh collider to a separate ghost entity (`spawn.rs:1073-1080`), the `IsCollisionOnly` insert was removed. `grep` confirms it is never inserted anywhere — defined, queried, counted, but the query is always empty and the `static_meshes.rs:207-210` gate is dead. The `components.rs:107-126` comment and `precombined.rs:24-25` cross-ref still describe the OLD pattern ("the entity keeps its `MeshHandle`… Set in `crate::cell_loader::spawn` after `synthesize_static_trimesh`") — exactly what the ghost pattern replaced.
- **Evidence**: `grep -rn "insert.*IsCollisionOnly" byroredux/ crates/` → no matches. Live spawn site spawns a no-MeshHandle ghost; `physics-only (no MeshHandle)` is the real diagnostic count.
- **Impact**: No functional bug — collider-cost fix is correctly achieved via the absent MeshHandle. Pure doc-rot + dead code; the stale comment misleads anyone reasoning about why synthesized colliders stay out of BLAS.
- **Related**: ROADMAP R6a-stale-14; #1317/#1324.
- **Suggested Fix**: Delete `IsCollisionOnly` and its 4 query/count/import sites and rewrite the comments to point at the ghost-entity pattern (`spawn.rs:1073` / bhk `spawn.rs:463`); or, if retained for a future combined path, replace the doc body with "currently unused; synthesized colliders use a MeshHandle-free ghost entity instead."

### SF-D3-03: CDB extraction path is hardcoded to the base-game location — DLC / Creations CDBs are never extracted
- **Severity**: LOW
- **Dimension**: CDB Material Correctness
- **Location**: `byroredux/src/asset_provider.rs:504`
- **Status**: NEW (latent; currently masked by the Phase-1 boolean gate)
- **Description**: `build_material_provider` extracts only the hardcoded `materials\materialsbeta.cdb`. Each DLC/Creation ships its own CDB at a namespaced path (`materials\creations\shatteredspace\materialsbeta.cdb`, `…\sfbgs003\…`, `…\sfbgs00d\…`) inside its `* - Main.ba2` (passed via `--bsa`, not `--materials-ba2`). Neither the path nor the archive class is reached.
- **Evidence**: `a.extract("materials\\materialsbeta.cdb")` is the only extraction call. Archive enumeration this audit: `ShatteredSpace - Main01.ba2`, `SFBGS003 - Main.ba2`, `SFBGS00D - Main.ba2` each contain one `.cdb` under `materials\creations\<plugin>\materialsbeta.cdb`.
- **Impact**: None observable today (Phase 1 only flips a global boolean; once the base CDB loads, `has_starfield_cdb()` returns true for DLC `.mat` meshes too). The moment SF-D3-01's Phase-2 lookup lands, DLC materials will be absent from the index and silently fall back to keyword-guessed values — a regression that would hide inside the Phase-2 change.
- **Related**: SF-D3-01.
- **Suggested Fix**: When implementing Phase 2, scan every loaded archive (`--materials-ba2` and `--bsa`) for `materials\**\materialsbeta.cdb` and merge in load order rather than extracting one fixed path.

### SF-D4-03: Model-less STAT/BNDS/ACTI/ARMO Starfield forms drop because geometry lives in a `BFCB` component block, not a top-level `MODL`
- **Severity**: LOW
- **Dimension**: SF ESM Resolve-Rate
- **Location**: `crates/plugin/src/esm/cell/support.rs:38-160` (only top-level `MODL` is read)
- **Status**: NEW
- **Description**: `build_static_object_from_subs` extracts the model only from a top-level `MODL` subrecord. Some Starfield STAT/BNDS/ACTI/ARMO records put the model reference inside a `BFCB`-wrapped component, so they return `None` and their REFRs drop.
- **Evidence**: `STAT 00000021 subs: EDID OBND ODTY OPDS BFCB BFCE FLLD PRPS DNAM` (no MODL); `BNDS 000001F9 subs: EDID OBND ODTY DNAM(28) MNAM(4)`. Counts: STAT 44/2, BNDS 60/2, ACTI 33/11, ARMO 3/1 — ~140 REFRs (~0.5% of cell).
- **Impact**: Small. The two unresolved STAT forms are very low FormIDs (0x21/0x43 — likely default/template/marker statics); BNDS is bendable-spline (needs a generator). Tail content; no structural architecture lost.
- **Related**: SF-D4-01 (shares the `BFCB` walker need).
- **Suggested Fix**: When SF-D4-01's `BFCB` component walker lands, reuse it to recover a model reference for STAT/ACTI/ARMO. BNDS needs a dedicated spline-mesh generator — track separately.

### SF-D4-04: docstrings claim unresolved REFRs spawn a "3D-unit-cube placeholder"; the live path silently skips them
- **Severity**: LOW
- **Dimension**: SF ESM Resolve-Rate (doc accuracy)
- **Location**: `byroredux/src/sf_smoke.rs:9,132`; `docs/engine/starfield-esm-phase0-baseline.md:132`
- **Status**: NEW (doc rot)
- **Description**: Several docstrings state an unindexed-base REFR "will spawn the 3D-unit-cube placeholder." The actual cell-loader behaviour on a `statics.get` miss is a silent `continue` — no placeholder mesh is created.
- **Evidence**: `byroredux/src/cell_loader/references.rs:362-378` — `None => { stat_miss += 1; … continue; }`. No spawn/cube on the miss branch.
- **Impact**: Misleading diagnostics only — an operator would expect to *see* unit cubes for the 11.2% gap and wrongly conclude rendering is fine when content is invisibly missing.
- **Suggested Fix**: Reword to "silently skipped (no geometry spawned)"; optionally add a debug-only placeholder-cube spawn behind a flag so the documented behaviour is actually available for visual triage.

### SF-D5-02: Starfield `StarfieldLighting` payload (gravity_scale + volumetric height-fog) decoded but not forwarded past the runtime resource boundary
- **Severity**: LOW (informational — decode-ahead-of-consumer, intentional staging)
- **Dimension**: ESM + Cell Bring-up
- **Location**: decode `crates/plugin/src/esm/cell/walkers.rs:560-575`; runtime boundary `byroredux/src/components.rs:320-345` (`CellLightingRes::from_cell_lighting`)
- **Status**: NEW
- **Description**: The SF-specific XCLL tail (gravity_scale, near/far height-fog mid/range, high-density fog colours, interior_type) is decoded into `CellLighting.starfield` and pinned by a test. The shared fog fields are forwarded to `CellLightingRes`, but `from_cell_lighting` does not copy the `.starfield` sub-struct, so gravity_scale and the height-fog model stop at the plugin layer.
- **Evidence**: `from_cell_lighting` enumerates every field except `starfield`; no consumer of `.gravity_scale`/height-fog exists outside the parser + its test.
- **Impact**: None today — the engine has no interior volumetric height-fog or cell-driven gravity model, so there is nothing to forward to. Consistent with the parse-ahead pattern (NAVM, IMGS, FNV/Skyrim extended XCLL).
- **Suggested Fix**: No action now. When a consumer lands, add `starfield: lit.starfield.clone()` to `from_cell_lighting`.

### SF-D5-03: SF XCLL decode requires `len == 108` exactly; a non-108 SF cell silently falls to the Skyrim `>= 92` arm
- **Severity**: LOW (defensive gap; zero observed instances in vanilla SF masters)
- **Dimension**: ESM + Cell Bring-up
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:519` (`game == Starfield && len == 108`) → fall-through `:605` (`len >= 92` Skyrim)
- **Status**: NEW
- **Description**: The dedicated SF decode branch is gated on exact `== 108`. All ~11 985 vanilla SF cells ship exactly 108. A modded/future-DLC SF cell at any other size ≥ 92 would skip the SF arm and be decoded by the Skyrim ambient-cube/specular/fresnel path, misreading the height-fog bytes. `xcll_size_sanity_warn` fires, so the symptom is at least logged.
- **Evidence**: exact-equality gate at `:519` vs `>= 92` Skyrim gate at `:605`.
- **Impact**: Negligible for vanilla (no non-108 SF cell exists); only a hypothetical mod would get mis-lit fog, and the sanity-warn surfaces it.
- **Suggested Fix**: Optional hardening — broaden to `game == Starfield && len >= 108` so any SF-classified cell takes the SF path regardless of trailing pad.

### SF-D7-02: #746/#747 truncation tail did NOT grow — shrank to 325 recoverable `BSWeakReferenceNode` terrain-overlay blocks
- **Severity**: LOW
- **Dimension**: Real-Data Validation
- **Location**: `crates/nif/src/blocks/node.rs:843-880` (`BsWeakReferenceNode::parse`)
- **Status**: Existing: residual of #746/#747 (both CLOSED) — confirmed not regressed
- **Description**: The audit checklist asks whether the Meshes01/MeshesPatch truncation tail grew. It has not — it shrank. Meshes01 now has **zero** truncations; MeshesPatch has 325 (down from ROADMAP's implied ~564). Every one is `BSWeakReferenceNode` (7 227 clean / 325 NiUnknown = 4.3% of that type), and every truncated file is a `meshes\terrain\overlay*` / `lc*world` / `oe*world` terrain-overlay LOD mesh dropping exactly 1 block.
- **Evidence**: All-meshes sweep log (325 truncated, MeshesPatch only). Isolated `overlaytraitprimedlifesm01.1.0.0.nif`: parses NiNode (clean) + `BSWeakReferenceNode` (NiUnknown). Drift histogram reports "No drift detected" — not byte-stride drift; the parse arm returns `Err` (over-read past block boundary / EOF) on the terrain-overlay variant tail. 100% recoverable.
- **Impact**: 325 terrain-overlay LOD blocks fall to NiUnknown recovery — a far-LOD splat layer, not near-field geometry. Low visible impact; bounded and stable.
- **Suggested Fix**: Byte-audit the `BSWeakReferenceNode` tail on a terrain-overlay sample against nif.xml to recover the last block (likely an SF-specific `SF_FORM_ID`/water-ref tail field). Low priority — recoverable and far-LOD.

### SF-D2-01: BSGeometry external-mesh triangle indices not range-checked against vertex count before BLAS/index-buffer upload
- **Severity**: LOW (informational; shared with all NIF mesh paths, not SF-specific)
- **Dimension**: BSGeometry Mesh Extraction
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs` (index flattening from external `.mesh`)
- **Status**: NEW
- **Description**: Flattened triangle indices from external `.mesh` files are not range-checked against vertex count before BLAS/index-buffer upload. Attribute arrays themselves are bounds-checked (no parse panic), but a corrupt `.mesh` could feed an out-of-range index to the GPU.
- **Evidence**: Audit trace of `extract_bs_geometry` Stage B; no `idx < vertex_count` assertion before handing indices to the renderer.
- **Impact**: Robustness only — vanilla content is well-formed (99.64% clean parse). A malformed/attacker-supplied `.mesh` could produce out-of-range GPU indexing. Shared across all NIF mesh paths.
- **Suggested Fix**: Add a single `max(index) < vertex_count` validation at the import→renderer handoff (one check covers all mesh paths), erroring/clamping rather than uploading. Flag for a general robustness sweep, not a Starfield-specific change.

### SF-D9-02: BGEM `grayscale_to_palette_alpha` bool parsed but not forwarded
- **Severity**: LOW
- **Dimension**: BGSM/BGEM External Flow
- **Location**: `crates/bgsm/src/bgem.rs:49` (parsed) / `byroredux/src/asset_provider.rs:1399-1501` (not forwarded)
- **Status**: NEW
- **Description**: BGEM parses `grayscale_to_palette_alpha: bool` but the merge arm forwards only the LUT *texture* (→ `EFFECT_PALETTE_COLOR`), never the alpha-variant bool, so `EFFECT_PALETTE_ALPHA` is set only from the inline `BSEffectShaderProperty` SLSF1 source, never from the `.bgem` file.
- **Evidence**: grep shows `grayscale_to_palette_alpha` has zero consumers outside the parser; `EFFECT_PALETTE_ALPHA` is only ORed from `es.effect_palette_alpha`.
- **Impact**: Narrow — only a FO4/Starfield-mod `.bgem` that sets palette-*alpha* (not color) and lacks the inline SLSF1 bit would remap by luminance into color instead of alpha. Vanilla FO4 palette-alpha effects use the inline path; near-zero visible impact.
- **Suggested Fix**: In the BGEM arm, when `bgem.grayscale_to_palette_alpha`, set a corresponding `ImportedMesh` flag and OR in `EFFECT_PALETTE_ALPHA`. Confirm against a real `.bgem` corpus first — may be empty in practice. Enhancement.

---

## INFO / Verification (no action; recorded for the trail)

- **SF-D9-01 (INFO)** — BGEM effect materials set the `BGSM_AUTHORED` telemetry flag (a slight misnomer for the effect path). No shader branch reads it; affects only debug-server material inspection. `asset_provider.rs:1407`.
- **SF-D6-01..04 (verify)** — #1510 shader over-read confirmed byte-perfect closed (0 NiUnknown / 0 drift on 204 K real shader blocks); CRC32 flag arrays correctly gated; `BSEffectShaderProperty` FO76+ tail reads at Starfield empirically correct; WetnessParams + refraction-power byte consumption matches nif.xml. `crates/nif/src/blocks/shader.rs`.
- **SF-D7-01/03/04 (positive)** — parse rate holds and exceeds ROADMAP (99.64% clean / 100% recoverable, 89 276 NIFs); no new NiUnknown block types since the FO76/Starfield baseline; texture archives extract 129/129, 0 failures.
- **SF-D8 (verify ×6)** — `translate_material` single boundary clean; metalness/roughness plain `f32`; no surviving per-draw `classify_pbr`; `EmissiveSource` discriminator (#1280) correct; `BhkMultiSphereShape` + `BhkConvexListShape` both translate (not dropped); `GpuMaterial` `#[repr(C)]` 300 B size-pinned, no shader drift.
- **D1-X1/X2 (disproved)** — LZ4 undersized-`max_size` heap-overflow concern → clean `Err`, no UB; no alternate BA2 reader bypassing the codec dispatch.

## CRC32 Flag Table

Correcting prior audits: the CRC32 hashes are **NOT opaque**. A maintained name table exists at `crates/nif/src/shader_flags.rs` (`bs_shader_crc32`), pinned to nif.xml literals — it maps known hashes to flag names (DECAL / PBR / TWO_SIDED / etc.) for `BSLightingShaderProperty` / `BSEffectShaderProperty` SF1/SF2 arrays. The 2026-05-28 audit's claim that "no empirical mapping table is maintained in-tree" is now stale. The arrays are parsed via `read_u32_array` under the allocate-vec budget guard (#764, #981); gates are `FO4_CRC_FLAGS = 132` (SF1) and `FO76_SF2_CRCS = 152` (SF2) in `crates/nif/src/version.rs`. No new derivation needed this audit.

## Remaining-Work Chain (per `starfield-esm-roadmap.md`)

Phases 0+1 done; Phases 2-4 invalidated by the 99.9%-parity measurement. In priority order (both the BGSM parser and the ESM parser have shipped — do NOT frame this as "BGSM-first / ESM-very-far"):

1. **Per-field CDB extraction** (#1289 Phase 2 — SF-D3-01). `.mat`-resolved materials currently reach the Disney lobe with NIF-keyword-guessed PBR; the authored 1.44M-instance CDB dataset is parsed and discarded. Top renderable-fidelity blocker. Scope DLC CDB paths (SF-D3-03) in the same change.
2. **ESM-placed LIGH decode** (SF-D4-01). 656 Cydonia lights dropped on the `BFCB` component-block layout. Largest functional gap to "Cydonia looks right." Reuse the same `BFCB` walker for model-less STAT/ACTI/ARMO (SF-D4-03).
3. **PDCL + decal-projection** (SF-D4-02). 1 846 placed decals; needs a projection system, defer; add a warned-skip arm in the interim.
4. **Exterior worldspace tiles / space-cell / planet / GBFM records** — unchanged, deferred (GBFM is only 2 REFRs on Cydonia — do not promote on its account).
5. **#746/#747 NIF truncation tail** (SF-D7-02) — shrank to 325 recoverable far-LOD terrain-overlay blocks; low priority.

## What's Possible Today (verified live)

- **Walkable Cydonia interior** — `--esm Starfield.esm --sf-smoke citycydoniamainlevel` resolves **88.8%** of 27 898 REFRs to base statics; geometry + colliders + door teleports + XCLL lighting all wire. Gaps: ESM-placed lights (SF-D4-01) and decals (SF-D4-02).
- **Individual mesh + texture visualization** — geometry + textures resolve correctly; Disney BSDF runs, but on keyword-guessed PBR for `.mat` content until #1289 Phase 2 (SF-D3-01).
- **BA2 v2/v3 extraction** — 129/129 archives, 0 failures.

## References

- Per-dimension outputs: `/tmp/audit/starfield/dim_1.md` … `dim_9.md`
- Dedup baseline: `/tmp/audit/issues.json` (300 issues) → `/tmp/audit/starfield/dedup.txt` (97 relevant)
- Prior audit: `docs/audits/AUDIT_STARFIELD_2026-05-28.md` (its two top blockers — CDB consumer wiring, walkable Cydonia — both now shipped)

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-06-14.md` (will file 2 HIGH + 2 MEDIUM + 9 LOW; SF-D3-01 is Existing #1290 — confirm before filing a duplicate).
