# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-10-09

**HEAD**: 3bcf6c8e8 · **Baseline**: [AUDIT_EXTERIOR_2026-10-08.md](AUDIT_EXTERIOR_2026-10-08.md) (HEAD `00f580e09`, 81 commits since) · **Audited**: Dim 1 (EXAL boundary), Dim 5 (WATAL, including the streaming LOD-water lifecycle), Dim 6 (distant LOD) · **Unchanged since baseline (skimmed)**: Dim 2 (terrain, no commits), Dim 3 (ground cover; only `3bcf6c8e8`'s grade constants touch `shader_constants_data.rs`), Dim 4 (sky / weather / sun, no commits), Dim 7 (gates / harness, no commits)

**Method.** This run is part of `/audit-suite --preset streaming-deep`.
- It puts the streaming area first: terrain and terrain-LOD rings, the BTR / object / placement LOD rings, LOD-water spawn / rebuild / unload, the default-weather rule, REGN `WNAM`, and authored cover.
- One auditor did every dimension synchronously. No sub-agents were used. No engine launch, smoke script or capture.
- Delta scope: `git log 00f580e09..HEAD -- <Paths>` for each dimension. The exterior-relevant commits are:
  - Closures of the baseline findings: `67afa4b95` (#5374), `217bcbc9f` (#5388), `b3e679dba` (#5423), `302394f94` (#5424), `fbf1bed74` (#5387), `4e32d33ec` (#5425).
  - `bd052048a` (#5421, REGN `WNAM`).
  - `2dab4ff48` (#5170/#5171, the Starfield unit-lift pin).
  - `3031d8976` (#5281, the texture-only boundary twin).
- Read-only Python censuses (scripts in `/tmp/audit/exterior/scripts/`):
  - FO3 and FNV mesh/texture BSA name tables: per (worldspace, level) lattice-residue classes and what the #5387 prune drops.
  - FO3 WRLD bounds and authored exterior-cell grids, from `Fallout3.esm` and `Anchorage.esm`.
  - REGN sub-record census on `Skyrim.esm`, `Fallout4.esm`, `Fallout3.esm` and `Oblivion.esm`.

**Test state (green at HEAD, rustc 1.96.0):**

| Suite | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- env_translate env_health terrain groundcover weather water lod resident_vwd sky spawner climate legacy_lod default_weather` | 575 passed, 0 failed, 20 ignored. The 20 ignored tests need game data or a device. |
| `cargo test -p byroredux-renderer --lib -- groundcover sky_ water terrain memory_budget shader_constants` | 252 passed, 0 failed, 1 ignored (the Vulkan prefilter test) |
| `--ignored legacy_lod_index` (FO3 real data: `dcworld03` ×2, `washmontop`) | 3 passed |

## Executive Summary

**5 findings, all NEW: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 5 LOW.**

All six baseline findings are closed, and each fix holds where it was aimed:
- **#5374**: Oblivion child worldspaces resolve their parent's `NAM2` at Z=0.
- **#5388**: the SEWorld children resolve SEWorldClimate, and the Tamriel children find `TamrielClimate` through the chain root. "Richest" is now a logged last resort.
- **#5423**: both LOD rings and the index scan are table-gated, and climate resolution is one `resolve_exterior_climate` boundary.
- **#5424**: there is one default-weather rule, and the XCCM path gets the WTHS stand-in.
- **#5387**: no same-level overlaps survive on any FO3 worldspace. FNV and both games' terrain tables are single-residue, so they pass through unchanged (census-confirmed).
- **#5425**: the skyal.md entries for the disc floor and the sunset hold are present.

What is left is residue from that fix wave:
- **D1-01**: #5374 implemented Oblivion inherit-all twice. A parse-time flag stamp is the one that runs. A flag-less copy of the chain walk is dead on parsed data, and that dead copy is the one the tests pin.
- **D6-01**: #5387's majority-residue prune decides 3 of its 7 multi-residue sets by an arbitrary tie-break. In two of them the losing generation covers 2 authored cells the winner does not, so those cells get no distant objects.
- **D1-02**: #5424's "shared rule" test compares the function with itself.
- **D1-03**: #5423 gates the legacy-quad index scan on the terrain table, but the object ring consumes the same index.
- **D1-04**: doc rot. exal.md and env_translate.rs still say Oblivion regions author REGN `CNAM`; #5421 disproved that, and this census measured 0 across four games. There is also a dead intra-doc link, and #5374, #5388, #5222 and #5387 are undocumented in exal.md.

**Streaming-area verdict.**
- The LOD-water lifecycle was re-read in full (spawn, recenter/rebuild, emptied ring, unload, drain); every entry path funnels through `assemble_exterior_streaming` → `spawn_lod_water`.
- Old meshes are released through `drop_mesh`'s 2-frame deferred destroy.
- Nothing new was found beyond the open #5289, #5334, #5335, #5337, #5339 and #5341.
- The terrain, object and placement ring code is unchanged apart from the #5423 gate and the #5387 prune.
- Authored cover is unchanged and is reclaimed in the drain batch.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **Holds, and is restored for climate.** Every climate rung now sits in `env_translate::resolve_exterior_climate`, and both default-weather callers go through `resolve_default_weather`. Dented only internally: Oblivion inheritance lives in two mechanisms (the parse stamp plus a flag-less walk) (D1-01). |
| **no-fabrication** | **Mostly holds.** "Richest climate" is now a logged last resort. The #5387 tie-break `Reverse(residue)` is an arbitrary choice standing in for data that is available (D6-01). |
| **no-leak** | **Holds.** No new parsed-but-unconsumed fields. REGN `worldspace_form` has no consumer and needs none. |
| **no-render-time-fallback** | **Holds.** The three baseline `game ==` branches are gone. The one remaining Oblivion branch is at the parse boundary (`wrld.rs:228`), which is allowed. GLSL is unchanged. |

## Per-Category Matrix

✓ = holds, ~ = dented (see the Findings column).

| Category (boundary fn) | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh` + `build_cell_splat_layers`) | ✓ | ✓ | ✓ | ✓ | none (unchanged) |
| **Sky** (`translate_sky`, `pack_sky_dome`) | ✓ | ✓ (skyal.md now records the 1.8 floor) | ✓ | ✓ | none (unchanged) |
| **Weather / sun / image space** (`translate_weather`, `resolve_default_weather`) | ✓ (#5424) | ✓ | ✓ | ✓ | D1-02 (test) |
| **Climate** (`resolve_exterior_climate` → `resolve_worldspace_climate` / `region_climate_for_center` / `oblivion_climate_rungs`; `resolve_cell_climate`) | ~ D1-01 (two inheritance mechanisms) | ✓ | ✓ | ✓ | D1-01, D1-04 |
| **Water** (`default_water_for_worldspace`, `translate_lod_water`, `build_distant_water_mesh`) | ~ D1-01 | ✓ | ✓ | ✓ | D1-01; open #5289/#5334/#5335/#5337/#5339/#5341 |
| **Ground cover** (`groundcover_translate.rs`, `resolve_authored_cover`) | ✓ | ~ #4906 (open) | ✓ | ✓ | none new |
| **Distant LOD** (`select_authored_lod_quads` + `prune_same_level_overlaps`, `terrain_lod_layout`, `object_lod_scheme`) | ~ D1-03 (scan gated on one ring's table) | ~ D6-01 | ✓ | ✓ | D6-01, D1-03 |

## Findings

### EXT-D6-2026-10-09-01: #5387's majority-residue prune breaks 3 of its 7 multi-residue ties on an arbitrary `Reverse(residue)`, and in two of them the dropped generation covers 2 authored cells the kept one does not
- **Severity**: LOW. Two edge cells in each of two small worldspaces lose distant objects. The rule decides with no data in 3 of 7 cases.
- **Dimension**: Distant LOD and trees
- **Location**: `byroredux/src/cell_loader/lod_bands.rs:453-488` (`prune_same_level_overlaps`; the tie-break is at `:470`), called from `select_authored_lod_quads` at `:504`. Test: `same_level_leftover_generation_quads_are_pruned` (`:616`) has no tie case.
- **Status**: NEW. This is a follow-up of #5387 (closed); the fix is correct in every non-tie case.
- **Tier Violated**: no-fabrication. The tie-break is an arbitrary choice even though the selection could be anchored in data.
- **Game Affected**: Fallout 3 (FNV and both games' terrain tables are single-residue)
- **Description**:
  - The prune keeps the residue class with the most quads per level. Ties go to the smallest `(rx, ry)`, "for determinism".
  - Census of every FO3 object table with more than one residue class:
    - **washmontop**: the 13-quad majority (4,4) covers every authored cell that any class covers.
    - **dcworld01, dcworld17**: the majority covers every authored cell that any class covers.
    - **dcworld06 (3/3)**, **dcworld12 (4/4)** and **dlc02baileyscrossroads (7/7, Anchorage)**: exact ties, so the tie-break decides.
- **Evidence** (`tie_loss.py`: FO3 BSA name tables against the `XCLC` grids in `Fallout3.esm` / `Anchorage.esm`):

  | Worldspace | Kept residue → authored cells covered | Dropped residue → authored cells covered | Authored cells only the dropped set covers |
  |---|---|---|---|
  | dcworld06 (L8) | (1,5) → 69 | (1,6) → 71 | 2: (15,−3), (16,−3) |
  | dlc02baileyscrossroads (L4) | (0,2) → 67 | (0,3) → 69 | 2: (12,−8), (12,−7) |
  | dcworld12 (L8) | (2,6) → 87 | (4,6) → 87 | 0 |

- **Impact**:
  - Four edge cells across two worldspaces get no distant-object LOD.
  - The choice of which generation's geometry draws in these three worldspaces does not depend on the data.
  - Which lattice the shipped game itself loads is not established.
- **Related**: #5387, #5222 (closed); EXT-D6-2026-10-08-01.
- **Suggested Fix**:
  - Break count ties on authored-cell coverage. `wctx`'s exterior-cell set is already in hand at both ring call sites. Fall back to the residue order only when coverage also ties.
  - Add a tie fixture. Record in exal.md that which lattice the engine loads is still open.

### EXT-D1-2026-10-09-01: #5374 implemented Oblivion inherit-all twice. The parse-time PNAM stamp is the one that runs; the flag-less `inherit_all_up_chain` copy is dead on parsed data, and only the dead copy is tested
- **Severity**: LOW (tech-debt / test-gap; behaviour is correct)
- **Dimension**: EXAL boundary discipline (and WATAL default water)
- **Location**:
  - Parse stamp: `crates/plugin/src/esm/cell/wrld.rs:215-235`. It ORs `LAND|LOD|WATER|CLIMATE` into `parent_flags` for Oblivion children, using local copies of the bit constants.
  - Second walk: `byroredux/src/env_translate.rs:229-231` (Oblivion water arm) and `:398-436` (`inherit_all_up_chain`).
  - Redundant climate step: `:556-561` (the first step of `oblivion_climate_rungs`).
  - Tests: `:2757` (`oblivion_child_inherits_parents_water_without_pnam`) and `:5571` (`oblivion_child_inherits_the_parent_authored_climate`). Both fixtures leave `parent_flags` at 0.
  - The parse helper hard-codes the game: `crates/plugin/src/esm/cell/tests/wrld.rs:84` (`GameKind::Skyrim`).
- **Status**: NEW (introduced by `67afa4b95`, `217bcbc9f`)
- **Tier Violated**: single-boundary (two mechanisms for one inheritance rule)
- **Game Affected**: Oblivion
- **Description**:
  - Because `parse_wrld_group` stamps the inherit bits, the bit-gated `inherit_up_chain` already resolves an Oblivion child's `NAM2`, and `resolve_worldspace_climate` (rung 1) already resolves its `CNAM`.
  - The `or_else(inherit_all_up_chain)` and the chain step in `oblivion_climate_rungs` can therefore fire only on hand-built records with zero flags, which means test fixtures only.
  - `inherit_all_up_chain` re-copies the cycle guard, the linear `form_id` reverse lookup and the precedence rule. #2814 consolidated exactly that into `inherit_up_chain` ("a future fix to the walk would have landed in one copy and silently missed this one"). Its termination cases also lose `inherit_up_chain`'s `warn!` diagnostics.
  - The stamp's own comment says it exists "without every consumer carrying a second, game-aware walk", yet the same fix added that walk.
  - No test drives the stamp. If it were deleted, every test would still pass through the dead fallbacks.
- **Impact**: None today. It is a maintenance trap: a walk fix can land in one copy, and the production path is untested.
- **Related**: #5374, #5388, #2814, #2735.
- **Suggested Fix**:
  - Keep the parse stamp, which is the boundary-shaped one. Delete `inherit_all_up_chain` and the duplicated chain step.
  - Add an Oblivion-variant `parse_synthetic_wrld` test: an Oblivion child with no PNAM gets `parent_flags == 0x1B`, and a root stays 0.
  - Point the env_translate fixtures at stamped flags.

### EXT-D1-2026-10-09-02: `wths_only_climate_resolves_the_defaultweather_stand_in`'s "both callers share the rule" assertion compares `resolve_default_weather` with itself
- **Severity**: LOW (test-gap)
- **Dimension**: EXAL boundary discipline
- **Location**: `byroredux/src/env_translate.rs:5447` and `:5459-5465` (`default_weather_rule_tests`)
- **Status**: NEW (introduced by `302394f94`). It has the same shape as #5344.
- **Tier Violated**: n/a
- **Game Affected**: Starfield (the WTHS stand-in)
- **Description**:
  - `worldspace_pass` is `resolve_default_weather(&climate, &weathers)`, the identical call with the identical arguments as `resolved` three statements earlier. The equality can never fail.
  - What #5424 needed pinned is that both production callers route through the rule:
    - `build_exterior_world_context` (`cell_loader/exterior.rs:1902`)
    - `apply_cell_climate_override` (`scene/world_setup.rs:497`)
  - Nothing pins either call site. A re-inlined pick in either one, which is how #5424 arose, would still pass.
- **Impact**: The #5424 regression guard is vacuous for its stated purpose. The stand-in's own value is still pinned by the other two assertions.
- **Related**: #5424, #5344.
- **Suggested Fix**: Replace the self-comparison with a `source_scan::production_text` pin. It should assert that both caller files call `resolve_default_weather` and that neither names `default_weather_by_edid`. Alternatively, drive `apply_cell_climate_override`'s pure part through a fixture.

### EXT-D1-2026-10-09-03: The legacy LOD quad index is scanned under the terrain layout table, but the object ring consumes it under its own scheme table. #5423's "a future title adopting one scheme" rationale is applied to only one side
- **Severity**: LOW (discipline; correct today because both tables map only `Fallout3NV`)
- **Dimension**: EXAL boundary discipline / Distant LOD
- **Location**:
  - `byroredux/src/streaming_helpers.rs:115-125`: the scan is gated on `terrain_lod_layout(..) == FalloutLegacy`.
  - `byroredux/src/cell_loader/object_lod.rs:200-216`: the `ObjectLodScheme::FalloutLegacyBlocks` arm reads `input.legacy_lod_quads...unwrap_or_default()`.
- **Status**: NEW (introduced by `b3e679dba`)
- **Tier Violated**: no-render-time-fallback (table-shape rule: the gate and the consumer key on different tables)
- **Game Affected**: none today (structural)
- **Description**:
  - #5423 replaced `game == Fallout3NV` at the scan with the terrain table.
  - A title (or mod profile) whose object scheme is `FalloutLegacyBlocks` but whose terrain layout is not `FalloutLegacy` would never scan. The object ring's `unwrap_or_default()` would then select nothing, silently, with no warn: the "authored path in one ring, nothing in the other" split #5423 set out to remove.
- **Suggested Fix**:
  - Scan when either table selects the legacy family (`terrain_lod_layout == FalloutLegacy || object_lod_scheme == Some(FalloutLegacyBlocks)`).
  - Alternatively, log when a `FalloutLegacyBlocks` ring sees `legacy_lod_quads == None`.

### EXT-D1-2026-10-09-04: Climate / LOD doc rot after the 10-08 fix wave: the "Oblivion-era REGN CNAM" claim that #5421 refuted survives, a dead intra-doc link, and #5374 / #5388 / #5222 / #5387 are not in exal.md
- **Severity**: LOW (doc rot)
- **Dimension**: EXAL boundary discipline (docs)
- **Location**:
  - `docs/engine/exal.md:128-138` (climate paragraph).
  - `byroredux/src/env_translate.rs:489-505` (`resolve_exterior_climate` doc), including the link at `:501`.
  - exal.md §5.2 (`:480-496`, Fallout legacy blocks).
- **Status**: NEW. This is distinct from #5341, whose item 4 (exal.md's LOD-water "entry-time snapshot" text) is not re-reported.
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: all (docs)
- **Description**:
  1. exal.md:133 says the region chain is the REGN `CNAM` that "only Oblivion-era regions author (#5421)". env_translate.rs:497-499 says the same, and `:503-504` says "the only TamrielClimate references on disk are one special-case worldspace and one Shivering-Isles region".
     - #5421 established that xEdit defines no REGN `CNAM` and that vanilla authors none.
     - This census agrees: REGN `CNAM` occurs 0 times in Skyrim.esm (317 REGN), Fallout4.esm (106), Fallout3.esm (139) and Oblivion.esm (211).
     - The rung is inert on all vanilla content. `4e32d33ec` was written after `bd052048a` and misattributes it.
  2. env_translate.rs:501 links [`named_or_richest_climate`], which no longer exists: `217bcbc9f` renamed it to `oblivion_climate_rungs`.
  3. exal.md's climate text predates #5388. It describes only the `"<worldspace>Climate"` naming, not the inherit-all step or the naming along the whole `WNAM` chain.
  4. Neither exal.md nor watal.md records #5374's rule that pre-FO3 children inherit everything (it governs Oblivion default water and climate).
  5. exal.md §5.2 has no text for #5222's authored-quad index (`LegacyLodQuadIndex`) or #5387's majority-residue prune. Both now decide what FO3/FNV distant LOD draws.
- **Suggested Fix**:
  - Rewrite the region-rung sentence (in exal.md and the code doc) as "xEdit-undefined; 0 vanilla occurrences in any game; kept for mod data". Fix the link.
  - Add the #5374/#5388 inheritance and chain-naming rungs, and a §5.2 paragraph on the index and the prune (citing D6-01's tie caveat).

## Findings count

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| D1 EXAL boundary | — | — | — | 4 |
| D2 Terrain (skimmed) | — | — | — | — |
| D3 Ground cover (skimmed) | — | — | — | — |
| D4 Sky / weather / sun (skimmed) | — | — | — | — |
| D5 WATAL | — | — | — | — (D1-01 covers its water half) |
| D6 LOD / trees | — | — | — | 1 |
| D7 Gates / harness (skimmed) | — | — | — | — |
| **Total NEW** | **0** | **0** | **0** | **5** |

**Dedup sources:**
- `/tmp/audit/issues.json` (147 open).
- `gh issue list --state all` searches: "inherit_all_up_chain", "majority residue", "prune_same_level_overlaps", "LegacyLodQuadIndex scan", "REGN CNAM", "resolve_default_weather test", "named_or_richest_climate", "Oblivion inherit-all".
- #5341's body was read to exclude its items.
- #5463 (TES5/FO4 `XCCM` is a REGN reference) is open and is not re-reported.
- #5482 is closed today (`3bcf6c8e8`, the presentation grade) and is not re-reported.

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #5374, #5387, #5388, #5423, #5424, #5425 (2026-10-08 findings) | **CLOSED and verified.** The residue is D1-01 through D1-04 and D6-01. |
| #5421 (REGN `WNAM` = owning WRLD; `CNAM` xEdit-undefined) | CLOSED, verified. `worldspace_form` has no engine consumer, and there is no remaining `weather_form` reader. |
| #5289, #5334, #5335, #5337, #5339, #5341 (LOD water / docs) | OPEN. The LOD-water code is unchanged since the baseline and was re-read for the streaming focus. |
| #5343, #5344 | OPEN, unchanged. |
| #5170 / #5171 (Starfield WATR tile unit / LGTM lift pin) | **CLOSED** by `2dab4ff48`: the deferral is pinned and the LGTM/XCLL lift is tested through `parse_esm`. |
| #5463 (TES5/FO4 `XCCM` → REGN) | OPEN. `resolve_cell_climate` still falls a REGN FormID back to the worldspace climate, which is safe. |
| #3301 (REGN RDAT Weather and other non-Sound kinds have no selector) | OPEN. Region weather is still decoded and unconsumed. |
| #4913 (Skyrim `.btt` tree LOD), #4906 (LTEX→GRAS), #3307 (VWD culling), #5364 (WTHS decode) | OPEN, unchanged. |
| #5222 live FO3/FNV visual verification | Still owed (no engine launch this pass). |
| Ground-cover washout precheck / backlit re-mint | Not re-measured. Note: `3bcf6c8e8` regrades every game's presentation (exposure before grade, shadow toe). Captures from before 2026-10-09 are therefore not A/B-comparable with new ones, and the 0.039 washout line was calibrated under the old grade. This is renderer-owned and informational. |

## Skill drift (for the next `/audit-exterior` edit)

- **Dim 1**:
  - Climate resolution is now `env_translate::resolve_exterior_climate`. `named_or_richest_climate` is gone (now `oblivion_climate_rungs` + `named_climate`). `region_climate_for_center` moved out of `cell_loader/exterior.rs`; its test still lives there.
  - Record that REGN `CNAM` is 0 in vanilla across Oblivion, FO3, Skyrim and FO4, so the region rung is mod-only.
  - Record #5374's Oblivion inherit-all parse stamp (`wrld.rs`).
- **Dim 5**: add `byroredux/src/streaming/mod.rs` (`spawn_lod_water` / `recenter_lod_water`) to `Paths:`. This is the third report asking.
- **Dim 6**:
  - Both legacy rings are now gated on tables (`terrain_lod_layout` / `ObjectLodScheme`), not `game == Fallout3NV`.
  - Add `prune_same_level_overlaps` (#5387) next to the #5222 text, with the guards `same_level_leftover_generation_quads_are_pruned` and the `washmontop_level8_spans_multiple_residues_and_all_select` no-overlap pin.

## Cross-audit routing

- **`/audit-fo3`**: D6-01 (the dcworld06 / Baileys Crossroads tie-break coverage loss).
- **`/audit-oblivion`**: D1-01 (two Oblivion inheritance mechanisms; the parse stamp is untested).
- **`/audit-renderer`**: `3bcf6c8e8`'s grade change invalidates pre-10-09 exterior reference captures (renderer-owned).

Suggested next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-09.md`.

**Labels:**
- All findings: `terrain-exterior`, `low`.
- D6-01: `bug`, `game:fo3`.
- D1-01: `tech-debt`, `test-gap`, `game:oblivion`.
- D1-02: `bug`, `test-gap`, `game:starfield`.
- D1-03: `tech-debt`.
- D1-04: `documentation`, `doc-rot`.
