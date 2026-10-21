# Tech-Debt Audit — 2026-10-21

**HEAD**: 4a3e89d56 · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718; 119 commits / 425 files / +23.5k −3.0k lines since) · **Audited**: 1, 2, 3, 4, 8, 9 (delta-scoped to the 119 commits) · **Unchanged since baseline (skimmed)**: 5, 6, 7 (recipes re-run, guards spot-checked, no new instance) · **Depth**: deep · Run solo (suite rules: no sub-agents, read-only, no engine launch).

In one sentence: the 10-19 backlog was published (#6139–#6193) and the audit-skill items in it were fixed by two `/audit-sync` passes, but the 10-20/10-21 fix wave (WTHR, ragdoll, terrain heights, eviction parking, Starfield remap) turned the blocking clippy gate red, added four closed-HIGH real-data guards to no pin list, and grew a third copy of the same per-game WTHR decode.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 8 |
| **Total** | **10** |

By dimension: Dim 1 ×1 · Dim 2 ×4 · Dim 3 ×3 · Dim 4 ×1 · Dim 9 ×1. Dims 5/6/7: no new findings. Dim 8: the red clippy gate is cited as REG-2026-10-21-01 (regression audit), not re-filed.

- **MEDIUM (2)**
  - `TD2-2026-10-21-01`: the two WTHR decoders (`parse_wthr` 306 LOC, `parse_wthr_skyrim` 176 LOC) hand-copy the NAM0 sky-colour unpack (×3), the FNAM fog head (×2, plus a legacy HNAM copy) and the Creation cloud controls (×2). They have a divergent fix history: #6039 (FO4/FO76/SF cloud textures existed only in the Skyrim arm), #6038 (Skyrim NAM0 sizes; FO4 has its own shape table) and today's OBL-D4-2026-10-21-01 (Oblivion NAM0 groups).
  - `TD9-2026-10-21-01`: four closed-HIGH fixes (#6118, #5755, #6100, #6075) landed with an `#[ignore]`d real-data guard that is not in `.github/real-data-pins.txt`. Three of the four skip silently when the data is missing.
- **Gates**: the blocking clippy gate is **red**. That is REG-2026-10-21-01 (4 plugin-lib lints, which leave 5 crates unlinted and hide 6 more bin lints); this run reproduced it and does not re-file it. CI `Test + Check + Clippy` has failed on every push since 28e1f413e. The Miri and ABBA jobs are red as well, but those belong to ECS-2026-10-21-01/02/03. `cargo machete --with-metadata` is clean. The non-blocking `--all-targets` step now has 36 unique sites, up from 20 (Existing #6193).
- **Prior 10-19 items closed and verified in the tree**: #6174–#6181 and #6183 (skill split-hub pathspecs, owner row, tech-debt baseline, agents, scripting LOC, `PlayingScriptedIdle`, typed-map annotations, the loading-screen TODO issue, the pin-list repair and its guard). `TD4-2026-10-19-01` (#6173) looks fixed in substance (see `TD4-2026-10-21-01`).
- **Still live from 10-19 (Existing, not re-filed)**: #6139–#6145 (Dim 1), #6146–#6160 (Dim 2), #6161–#6172 (Dim 3), #6182, #6184–#6193. Numeric movement is recorded in the delta table below.

### Delta vs the 2026-10-19 baseline

| Metric | 10-19 (12677d718) | 10-21 (4a3e89d56) | Note |
|---|---:|---:|---|
| markers (TODO/FIXME/…) | 23 | 26 | +3 `XXXX` false positives (`dump_wths_refl.rs --type XXXX`, which already existed at the 10-19 HEAD but sat outside the count); the 2 real TODOs cite open #6194; 0 shader hits; `triangle.frag` MIT block intact (L28–40) |
| `allow(dead_code)` | 41 | 41 | per-file distribution identical |
| `unimplemented!` / `todo!()` | 0 | 0 | |
| `#[ignore]` tests | 343 | 363 | +20, all data gates (`TD9-01` covers the four that guard closed-HIGH fixes) |
| production files >2000 LOC | 4 | 4 | same members: `shader_constants_data.rs` 2868→2958 (data table), `mq101_conformance.rs` 2157, `audio/lib.rs` 2034 (#6140), `nif/import/types.rs` 2001→2011 (#6141) |
| production fns >200 / >300 / >500 / >1000 | 210 / 84 / 29 / 5 | 212 / 85 / 29 / 5 | same scanner, both trees (calibrated: `apply_effect` 578, `parse_block_inner` 1102, `register_update_systems` 852→854, `build_pipelines_and_finish` 1373→1375) |
| test-heavy files >2000 total LOC | 86 | 92 | none escalates (no new prod_loc >2000) |
| blocking clippy gate | green | **red** | REG-2026-10-21-01 |
| `--all-targets` (non-blocking) unique sites | 20 | 36 | #6193 |
| `file.rs:LINE` refs past end of file | 18 | 36 | broader scan this run (crates, byroredux, tools, docs/engine, skills); #6167 |
| `/mnt/data/SteamLibrary` literal lines added | — | +14 | #6184 |

## Baseline Snapshot (Phase 1, measured at 4a3e89d56)

```
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 26
allow(dead_code):      41
unimplemented!/todo!(): 0
#[ignore] tests:        363
files >2000 production LOC: 4
test files >2000 total LOC (lower priority, separate bucket): 92
```
`prod_loc_self_test`: ok. The figures match the skill's own 2026-10-21 paragraph, which was measured at 1be33af66.

## Top 10 Quick Wins (trivial / small)

1. **REG-2026-10-21-01 (trivial, filed by the regression audit)**: fix the 4 plugin lints and the 6 bin lints hidden behind them in one commit, then re-run the whole workspace gate.
2. **`TD9-01` (trivial)**: append the four rows (`plugin-lib installed_starfield_small_masters_do_not_collide`, `bin fo4_mswp_canvas_swap_matches_the_picture_frame_nif`, `bin installed_default_idle_clips_exist_in_the_meshes_archive`, `bin installed_fallout_spawner_shells_resolve_a_model`). `every_real_data_pin_names_an_ignored_test_in_its_lane` then checks them.
3. **`TD3-01` (trivial)**: `docs/feature-matrix.md:178` and `ROADMAP.md:103` still say FO4 ragdolls wait on `BhkSystemBinary`. #6070 shipped them live.
4. **`TD4-01` (trivial)**: close #6173 after a one-line re-check (the 10-20/10-21 syncs fixed the callouts it lists).
5. **`TD3-03` (trivial)**: `groundcover_bench.rs:1623` documents the deleted `push_bytes` / "unsafe slice"; `skyal.md:369` backticks a memory slug.
6. **`TD2-02` (small)**: delete `terrain_height::bc4::decode_block` and `decode_top_mip`'s hand-rolled DX10 header parse in favour of one BC4/DDS home; move `parse_bgs_reflection_with_remap` out of `weather_settings.rs`.
7. **`TD2-04` (small)**: one `shipped_asset_path(rel)` / `exe_relative_dir()` helper replacing five spellings of the cwd-then-exe-dir resolution.
8. **`TD3-02` (small)**: document the loose `Data\` override tier in `archives.md`, the terrain-height cache directory and `BYRO_CACHE_DIR`, and give the 59 `BYRO*` env reads one table in `debug-cli.md` or `testing.md`.
9. **Existing #6188 (trivial)**: weather.rs added two more hand-indexed `u32::from_le_bytes(sub.data[0..4]…)` reads (`IAD` arm, `NAM1`, `parse_image_spaces`). Fold them into the same `SubReader` sweep.
10. **Existing #6164 / #6165 (trivial; CLAUDE.md is user-owned)**: the workspace is now **35** members (`crates/terrain-height`, 2026-10-20), and neither `architecture.md` ("27") nor the CLAUDE.md tree names it.

## Top 5 Medium Investments

1. **`TD2-01`: one Creation-era WTHR decoder** (medium). Route FO4/FO76/Starfield through the Skyrim decoder, which already owns the 32-layer cloud list, IMSP, JNAM/RNAM/QNAM/PNAM and DALC, driven by a per-version NAM0/FNAM shape table. Leave `parse_wthr` for Oblivion/FO3/FNV only. Extract `read_sky_color_table(data, groups, slots)` and `read_fog_head(r)` for the three NAM0 loops and the two fog heads. This removes 10 `matches!(game, …)` guards from one function. Pin the change with the existing per-game WTHR tests plus the Oblivion NAM0 cloud-colour fix (OBL-D4-01) once it lands.
2. **`TD1-01`: split `cell_loader/exterior.rs` before it crosses** (medium). It grew 1460 → 1687 production LOC in two days. The axis is persistent-cell window and actor stubs (`PersistentCellApplyJob`, `PersistentCellWindow`, `ensure_persistent_actor_stubs`, tile helpers, L227–1450) vs the `ExteriorCellApplyJob` streaming apply (L1451, L2160) vs worldspace selection/context (`select_worldspace_key`, `build_exterior_world_context`, L1500–2120). In the same pass, extract `spawn_terrain_mesh`'s (535 lines) vertex build, base-texture resolve and ground-cover density inputs.
3. **`TD2-03`: table-driven eviction parking** (medium). One `PARKED_COLUMNS` row per component (name, capture, restore, park, keep predicate) replaces the six hand-maintained spellings in `reference_state.rs`, so the #6085 guard checks the table, not the prose. Coordinate with SAVE-2026-10-21-04 (the nine `UNAUDITED` rows).
4. **Existing #6143 / #6144** (from 10-19, still the largest functions): `build_pipelines_and_finish` 1375, `parse_block_inner` 1102.
5. **Existing #6184 + `TD9-01`: a `test_paths::require_or_skip` helper** (small, mechanical). +14 hard-coded Steam paths in two days, three of them inside the new closed-HIGH guards. A hosted hygiene scan would stop the growth.

## Findings

### MEDIUM

| ID | Title | Effort |
|---|---|---|
| TD2-2026-10-21-01 | WTHR decoders duplicate NAM0/FNAM/cloud decode across two functions with divergent fix history | medium |
| TD9-2026-10-21-01 | Four closed-HIGH fixes guarded only by `#[ignore]`d real-data tests missing from the pin list | trivial |

### LOW

| ID | Title | Effort |
|---|---|---|
| TD1-2026-10-21-01 | `cell_loader/exterior.rs` +227 prod LOC in two days (1687), with `terrain.rs`/`reference_state.rs` close behind | medium |
| TD2-2026-10-21-02 | BC4 block decoder written twice in one commit; generic codecs homed in consumer modules | small |
| TD2-2026-10-21-03 | Eviction parking spells each parked component in six places | medium |
| TD2-2026-10-21-04 | Exe-relative asset/cache path resolution spelled five ways across four crates | small |
| TD3-2026-10-21-01 | Status floors still say FO4 ragdolls are blocked after #6070 | trivial |
| TD3-2026-10-21-02 | New runtime surfaces with no reference doc (loose `Data\` tier, height-map cache, env vars) | small |
| TD3-2026-10-21-03 | Stale doc from the 10-20/21 wave: deleted `push_bytes`, a backticked memory slug | trivial |
| TD4-2026-10-21-01 | #6173 remains open after the two syncs fixed it | trivial |

---

## Dimension 1: File / Function / Module Complexity

Primary bucket (prod_loc >2000) has the same four members. None is new and two are filed (#6140, #6141). `shader_constants_data.rs` is a data table and `mq101_conformance.rs` is unchanged at 2157. Watch list (≥1850): `debug-ui/src/panels.rs` 1934, `vulkan/context/init.rs` 1921, `records/misc/world.rs` 1912, `vulkan/gpu_timers.rs` 1874 (+57), `inventory.rs` 1863 (flat), `material/merge.rs` 1860. **Fastest risers in the window**: `cell_loader/exterior.rs` 1460→1687, `cell_loader/terrain.rs` 1338→1538, `cell_loader/reference_state.rs` 842→1117, `records/weather.rs` 1131→1338, `scene/nif_loader.rs` 1679→1799, `npc_spawn.rs` 1707→1762. Functions that crossed or grew: `parse_wthr` 251→306, `spawn_terrain_mesh` 520→535, `app_step` 315→339, `live_load` 316→335, `ragdoll.rs:327` 230→262, and `load_nif_with_skeleton_from` 202 (moved from `load_nif_bytes_with_skeleton`).

#### TD1-2026-10-21-01: `cell_loader/exterior.rs` grew +227 production LOC in two days and is the next 2000-line file
- **Severity**: LOW
- **Dimension**: 1
- **Location**: `byroredux/src/cell_loader/exterior.rs` (2669 total, 1687 prod); siblings `byroredux/src/cell_loader/terrain.rs:1000` (`spawn_terrain_mesh`, 535 lines), `byroredux/src/cell_loader/reference_state.rs` (1117 prod)
- **Status**: NEW (watch-list escalation; not in #6139–#6145)
- **Age**: 11 commits since 12677d718 (`ca12e5638` #6076, `611769b16` #6071–4, `2efed9843` #6102, `67e724c3e` #6127, `588c87bc4` #6123, `3b9060701` #6085, `79477b4a3` #6043, `e62e258bd` #6199, `c53d87679` #6048, `17bd40f4f` #5757, `dc829fc83` #6132)
- **Effort**: medium
- **Description**: The file has three jobs interleaved with five inline test modules:
  - the persistent-cell window and logical actor stubs (`PersistentCellApplyJob` L227, `resolve_persistent_cell` L500, `begin_worldspace_persistent_cell` L1132, `PersistentCellWindow` L1201, `ensure_persistent_actor_stubs` L1293, tile helpers L1336–1366);
  - the streaming `ExteriorCellApplyJob` (L1451, impl L2160–2535, whose `begin` is 207 lines and `advance` 152);
  - worldspace selection and context (`select_worldspace_key` L1617, `build_exterior_world_context` L1959, `load_one_exterior_cell` L2119).

  At the window's rate it crosses 2000 within a week. In `terrain.rs`, `spawn_terrain_mesh` keeps four phases inline: vertex build L1066–1160, base texture resolve L1211–1264, ground-cover density inputs L1281–1330, then tile allocation and spawn.
- **Evidence**: `prod_loc` at base vs HEAD: 1460→1687, 1338→1538, 842→1117. fnscan: `spawn_terrain_mesh` 535, `ExteriorCellApplyJob::begin` 207.
- **Impact**: This is the file every exterior streaming fix touches. Eleven fixes in two days all edited it, and the interleaved test modules make each diff harder to review.
- **Related**: #6145 (`cell_loader.rs` facade), SCR-2026-10-21-05 (`PersistentCellApplyJob` bug, scripting-owned).
- **Suggested Fix**: Split along the three jobs above. Make the test modules siblings (`exterior/persistent_tests.rs` …). Re-point any `include_str!("exterior.rs")` source scans in the same commit (`grep -rn 'exterior.rs"' byroredux/src`). Extract `spawn_terrain_mesh`'s four phases as named helpers.

## Dimension 2: Logic Duplication

The new renderer pass (`sky_cube/cloud_shadow.rs`) goes through the shared helpers (`compute_binding`, `clamp_sampler`, `write_*`, `GpuImage`, `image_barrier_to_general_write_layers`). Its three hand-spelled `vk::DescriptorImageInfo` builders are the #6148 class. No new weak `#[cfg(test)]` cut was found among the added `include_str!` scans. The new `live_reload_tests.rs` scans slice from `pub fn execute_pending_save_loads(`, past `live_load.rs`'s only test module (L243), so they cannot see their own needles.

#### TD2-2026-10-21-01: WTHR decoders duplicate NAM0 / FNAM / cloud-control decode across `parse_wthr` and `parse_wthr_skyrim`, with divergent fix history
- **Severity**: MEDIUM (promotion: duplicated logic with divergent bug-fix history)
- **Dimension**: 2 (also Dim 1: `parse_wthr` crossed 300 lines)
- **Location**: `crates/plugin/src/esm/records/weather.rs:455-765` (`parse_wthr`), `:1131-1306` (`parse_wthr_skyrim`); NAM0 loops at `:502-521` (FO4/FO76), `:522-547` (classic), `:1160-1195` (Skyrim); fog heads `:570-623` and `:1197-1215`, legacy HNAM fog `:663-674`; Creation cloud controls `:697-716` vs `:1224-1240`; IMSP `:480` vs `:1251`
- **Status**: NEW
- **Age**: `parse_wthr` 251→306 lines, `weather.rs` 1131→1338 prod LOC in 7 commits since 12677d718 (`7f4c25e3e` #6037, `67433ee3a` #6038, `3ab1b5557` #6039, `f16741886` #6040–2, `1dd217412` #6045, …)
- **Effort**: medium
- **Description**: FO4, FO76 and Starfield author Skyrim's Creation-era WTHR layout: the 32-layer `00TX`..`O0TX` cloud list, IMSP, JNAM/RNAM/QNAM/PNAM and the FNAM fog extension. They are still routed through the FO3/FNV-shaped `parse_wthr`, which bolts the Creation fields on behind a `creation_era` flag and 10 `matches!(game, …)` guards. Each Creation feature therefore exists twice, and the two copies have already drifted:
  - **#6039**: FO4/FO76/SF cloud textures were never decoded because only the Skyrim arm handled `xxTX`. The fix added a third call site through `set_creation_cloud_texture`.
  - **#6038**: Skyrim NAM0 accepted only the version-37 272-byte form and gained `skyrim_nam0_groups`. The FO4 arm has its own `creation_nam0_shape`.
  - **OBL-D4-2026-10-21-01** (today): Oblivion's NAM0 groups 2/9 are cloud colours, so a third per-game NAM0 rule is about to land in the same function.

  The `SkyColor { r: data[o], g: data[o+1], … }` unpack and the HIGH_NOON/MIDNIGHT synthesis are written three times. The four-float fog head (`fog_day_near`, `fog_day_far`, `fog_night_near`, `fog_night_far` with the 10000.0 defaults) is written three times.
- **Evidence**: `grep -n 'TOD_HIGH_NOON\] = ' weather.rs` returns 518, 540 and 1187. #6039's commit message: "Only Skyrim was routed to the xTX cloud-list decode; the generic WTHR arm matched FO3/FNV's DNAM/CNAM/ANAM/BNAM only."
- **Impact**: Every WTHR fix has to be applied once per arm and per game guard. The last three fixes each found a game the other arm had left behind. The weather table feeds SKYAL sky, clouds and fog in every exterior.
- **Related**: OBL-D4-2026-10-21-01, ESM audit's WTHR "verified clean" (byte layout is correct; this is shape), #5796 (the WATR equivalent, closed), Existing #6188 (two new hand-indexed reads in the same arms).
- **Suggested Fix**: Make `parse_wthr_skyrim` the Creation decoder for Skyrim, FO4, FO76 and Starfield, with a per-version NAM0/FNAM shape table, and leave `parse_wthr` for Oblivion/FO3/FNV. Extract `read_sky_color_table` and `read_fog_head` helpers. Keep the per-game WTHR tests as the pin.

#### TD2-2026-10-21-02: a BC4 block decoder was written twice in the same commit, and generic codecs live in consumer modules
- **Severity**: LOW
- **Dimension**: 2
- **Location**: `crates/terrain-height/src/bc4.rs:112-131` (`decode_block`) vs `crates/menuxml/src/tex.rs:279-307` (`bc4_block`); `crates/terrain-height/src/lib.rs:197-221` (`decode_top_mip` re-parses the DX10 header by hand: `148`, `84..88`, `rd(128) != 80`) vs `menuxml::tex::Rgba8::decode_dds` → `decode_dx10`; `crates/plugin/src/esm/records/lmsw.rs:32-33` importing `parse_bgs_reflection_with_remap` / `ReflectionObject` from `weather_settings{,_instance}.rs`
- **Status**: NEW
- **Age**: `5017a64c2` (2026-10-20, #6195) added both BC4 decoders and widened menuxml's decoder to BC4/BC5/DX10 "for engine textures"; `17bd40f4f` (2026-10-21, #5757) made LMSW/MTPT the second consumer of the WTHS reflection decoder
- **Effort**: small
- **Description**: The two `decode_block`/`bc4_block` bodies are identical: same selector unpack, same 8- and 6-value palettes. `decode_top_mip` re-implements the DX10 header walk that `Rgba8::decode_dds` already does, and its only callers (`commands/terrain.rs:202`, tests, the example) run in the bin, which already depends on menuxml. More broadly, the workspace's CPU DDS codec now lives in the MenuXml UI crate. It serves `tex.dump`, terrain heights and HUD art, while `byroredux/examples/msn_basis_probe.rs` keeps a fourth BC1 decoder. The Bethesda reflection decoder (`parse_bgs_reflection*`) lives in the WTHS weather-settings module while it decodes LMSW and MTPT material records.
- **Evidence**: `grep -rn 'fn decode_block\|fn bc4_block\|fn decode_dds' crates byroredux` lists 2 BC4 decoders and 3 DDS front doors (renderer `vulkan/dds.rs` parses headers for upload only).
- **Impact**: A BC4 correctness fix (six-value-mode edge cases) has to be found twice. Nothing in the module names tells a reader where the general texture codec lives.
- **Related**: PERF-D3-01 (height-map full DDS decode on cache hit), CONC-2026-10-21-01/02 (height job panics / temp name).
- **Suggested Fix**: Move the CPU DDS codec (`menuxml::tex` decode half) to a neutral home, such as a `byroredux-bsa`-adjacent `texdecode` module or `core`. Have `terrain-height` take decoded `Rgba8`/BC4 bytes from it, or keep only the encoder. Rename `weather_settings.rs`'s reflection half to `records/reflection.rs` and re-export it.

#### TD2-2026-10-21-03: eviction parking spells each parked component in six places
- **Severity**: LOW
- **Dimension**: 2
- **Location**: `byroredux/src/cell_loader/reference_state.rs:28-96` (`ReferenceState` / `ScriptState` fields), `:172-364` (`capture`, one guarded pass per component), `:439-603` (`restore`), `:898-1080` (`park_unresolved_snapshot_rows`: 18 `column::<T>(snapshot, "Name")` string keys plus the `has_*` keep predicate at ~`:985`), `byroredux/src/save_io/registry_completeness_tests.rs:1143-1270` (`PARKED`, `NOT_PARKED`, `FIELDS_WITHOUT_A_COLUMN`)
- **Status**: NEW
- **Age**: grew 842→1117 prod LOC in the window (`611769b16` #6071–4, `3b9060701` #6085 +267 lines, `a44d4a20d`, `dc829fc83`)
- **Effort**: medium
- **Description**: Adding a parked component (#6085 added three) edits the struct, the capture pass, the restore insert, the save-load park column (keyed by a **string** that must equal the save-registry name in `save_io/registry.rs`), the keep-rule predicate and the guard's `PARKED` table. `every_delta_column_is_parked_or_explicitly_not` (#6085) proves the partition and that each named field exists. It does not prove that `capture`, `restore` and `park_unresolved_snapshot_rows` each handle every field. A field added to the struct and to `capture` but missed in `park_unresolved_snapshot_rows` would silently drop that state on a save load. That is the class #6085's own doc says drifted twice (GAME-SD-01/02, SAVE-SD-03). All 17 columns are wired today.
- **Evidence**: `column::<byroredux_scripting::SpellList>(snapshot, "SpellList")` (`:925`) and `("SpellList", "ReferenceState.spells")` (guard `:1150`) are separate literals for the same fact.
- **Impact**: This costs work on every new gameplay component and risks a silent save-load state drop.
- **Related**: SAVE-2026-10-21-04 (nine `UNAUDITED` rows in the same partition), SAVE-2026-10-21-06, *save_registry_order_sensitive* memory.
- **Suggested Fix**: Use one `const PARKED_COLUMNS: &[ParkedColumn]` (name, field accessor, capture/restore/park fns) or a small macro. Generate `capture`/`restore`/`park` loops from it, and have the guard iterate the table instead of a parallel literal list.

#### TD2-2026-10-21-04: exe-relative shipped-asset and cache path resolution is spelled five ways across four crates
- **Severity**: LOW
- **Dimension**: 2
- **Location**: `crates/settings-io/src/presets.rs:84-94` (cwd, then exe dir), `crates/game-detect/src/profiles.rs:304-316` and `:403-415` (the same candidate list twice in one file), `crates/renderer/src/vulkan/context/helpers.rs:808-815` (`pipeline_cache_path`, exe dir only), `byroredux/src/asset_provider/terrain_height.rs:88-98` (`BYRO_CACHE_DIR`, then exe dir, then `.`), `tools/byro-launcher/src/engine.rs:39-42`
- **Status**: NEW (none of the crates was named by the 10-19 report)
- **Age**: the newest copy is `5017a64c2` (2026-10-20); the oldest is #735 (pipeline cache)
- **Effort**: small
- **Description**: The same "resolve next to the executable so a launcher or read-only cwd works" rule is re-derived per subsystem, with three different fallback orders and only one env override. `game-detect` repeats its own two-candidate loop in `load_default_from` and `ordered_config_paths_with`.
- **Evidence**: `grep -rn 'current_exe()' --include='*.rs' crates byroredux tools` returns 6 production sites.
- **Impact**: Packaging rules (TOOL-D4-2026-10-21-02 found the release docs say `assets/` is cwd-only) cannot be stated once. A future portable/read-only install mode has five places to patch.
- **Related**: TOOL-D4-2026-10-21-02, #735 (closed), #5910 (closed, profiles user-path).
- **Suggested Fix**: Add `game_detect::paths::{shipped_asset(rel), cache_dir(sub)}` (or put them in `settings-io`, which the bin and launcher both use) with one documented order (`BYRO_CACHE_DIR` override → exe dir → cwd), and route all five through it.

## Dimension 3: Stale Documentation & Comments

`_audit-validate.sh`: **OK, all path references valid**. Advisories: 37 deleted-file basenames in `docs/engine` (unchanged class), about 349 `docs/engine` symbols (5 new since base, all from `b6d6f8320`, `5017a64c2` and `ae557b935`: `environment_temperature_grading`, `portal_sun`, `SkyrimFog`, `realfft`, `xxhash64`) and 1 workspace-tree symbol (`CRHoldExpansion`, an ESM editor ID backticked in CLAUDE.md's smoke list; italicise it, user-owned). GPU-size prose: `GpuWaterParams` 368 B (`shader-pipeline.md:490,786`, `water.vert:141`) and `Vertex` 104 B (`mesh.rs:1753`, `skin_compute.rs:17`) match their layout pins (`water.rs:1071`, the vertex tests). The new cloud-shadow map figures in `memory-budget.md:1140` and `shader-pipeline.md:784` match `CLOUD_SHADOW_MAP_SIZE` 512 / R16F.

#### TD3-2026-10-21-01: status floors still say FO4 ragdolls are blocked, after #6070 shipped them live
- **Severity**: LOW (no audit shown misled yet; the feature matrix is the status floor audits read, so it becomes MEDIUM if one is)
- **Dimension**: 3
- **Location**: `docs/feature-matrix.md:178` ("FO4+ blocked on BhkSystemBinary"), `ROADMAP.md:103` ("FO4+ ragdolls wait on the `BhkSystemBinary` decoder")
- **Status**: NEW for the two status docs. They are siblings of PHYS-D3-2026-10-21-04 (code comments: `combat.rs:1055`, and per the orchestrator `asset_provider/actor_graph.rs:161`) and NIF-D5-2026-10-21-03 (`physal.md` item 6). A third stale code comment, `byroredux/src/systems/graph_locomotion.rs:446` ("FO4 ragdolls do not activate yet to override the pose"), belongs with PHYS-D3-04 and is not filed here.
- **Age**: `7c6e6c3e1` (2026-10-20, #6070 "live FO4 ragdoll smoke; keep the decoded articulation and retire the stand-in capsule"), `019b087e3` (#6066), `fc4a15104` (#6067); CLAUDE.md already lists `docs/smoke-tests/fo4-ragdoll.sh`
- **Effort**: trivial
- **Description / Evidence**: FO4 skeleton ragdolls are decoded (`decode_np_ragdoll`, #5015), articulated and activated on death, and gated by a live smoke (two feral ghouls and a human corpse posed on 18-body ragdolls, kills activate finite ragdolls). The matrix row and the ROADMAP gap list still say they are blocked.
- **Impact**: Per-game audits (fo4, physics) and the next `/session-close` read the matrix as the floor.
- **Related**: PHYS-D3-2026-10-21-04, NIF-D5-2026-10-21-03, #6161 (other matrix rows).
- **Suggested Fix**: Flip `:178` to "✓ Oblivion → Skyrim classic chain; FO4 decoded `bhkRagdollSystem` (#6070); FO76/SF tagfile payloads open (NIF-D5-01)". Drop the clause from `ROADMAP.md:103`.

#### TD3-2026-10-21-02: new runtime surfaces have no reference doc (loose `Data\` tier, height-map disk cache, env vars)
- **Severity**: LOW
- **Dimension**: 3
- **Location**: `docs/engine/archives.md` (no mention of the loose tier or provider precedence; `grep -c 'loose\|LooseDir\|last-listed' = 0`); `byroredux/src/asset_provider/archive.rs` (#6200 loose tier); `byroredux/src/asset_provider/terrain_height.rs:84-98` (`terrain-height/` cache directory, `BYRO_CACHE_DIR`); `docs/engine/index.md` (the `exal-terrain-height.md` row was dropped as "never written", `e6f74069d`)
- **Status**: NEW
- **Age**: `48e422a71` (2026-10-20, #6200), `5017a64c2` (2026-10-20, #6195)
- **Effort**: small
- **Description**: Loose `Data\` files now outrank every BSA/BA2 (#6200). The "authoritative" archive reference describes only the archive formats, so the precedence rule a modder or the parsers audit needs is undocumented. SF-2026-10-21-D2-01 and PAR-D6-01/02 already found behaviour gaps in that tier. The terrain height maps write a persistent on-disk cache next to the executable, keyed on the source texture hash and `GENERATOR_VERSION`, and no doc mentions where it lives or that `BYRO_CACHE_DIR` moves it. More broadly, 18 of the 59 `BYRO*`/`BYROREDUX_*` env reads appear in no doc, including production switches (`BYROREDUX_PRESENT_MODE` in `vulkan/swapchain.rs:207`, `BYRO_INTERIOR_AMBIENT_SCALE` in `render/mod.rs:572`, `BYRO_RT_FRUSTUM_ONLY` in `render/static_meshes.rs:469`, `BYRO_CACHE_DIR`).
- **Evidence**: the `env::var("BYRO…")` sweep above, cross-checked against `docs/`, `README.md` and `CLAUDE.md`.
- **Impact**: Users cannot clear or relocate the height cache. Audits of the loose tier have no spec to check against. Diagnostic switches are found only by reading code.
- **Related**: PAR-D6-01, PAR-D6-02, SF-2026-10-21-D2-01, PERF-D3-01, #6195/#6196 (open trackers).
- **Suggested Fix**: Add an "Asset provider precedence" section to `archives.md` covering last-listed wins, the loose tier and its case folding. Add a terrain-height paragraph to `exal-groundcover.md` or a short `exal-terrain-height.md` (source tiers, cache dir, `BYRO_CACHE_DIR`, `GENERATOR_VERSION`). Add one env-var table to `debug-cli.md` (or `testing.md` for test-only vars).

#### TD3-2026-10-21-03: stale doc from the 10-20/21 wave (deleted `push_bytes`, a backticked memory slug)
- **Severity**: LOW
- **Dimension**: 3
- **Location**: `crates/renderer/src/vulkan/groundcover_bench.rs:1623-1626` ("`push_bytes` hands the driver … the unsafe slice would start reading uninitialised padding"); `docs/engine/skyal.md:369` (`` `environment_temperature_grading` ``)
- **Status**: NEW
- **Age**: `02b828fff` (2026-10-21, #6033) deleted `push_bytes` and routed the push through `byte_view` with a `NoUninit` impl; `b6d6f8320` (2026-10-20) added the slug reference
- **Effort**: trivial
- **Description**: The test doc names a function that no longer exists and an unsafe slice that `NoUninit` now replaces. The padding invariant is now enforced by the `NoUninit` impl's SAFETY comment at `:174`. The skyal.md reference backticks a memory slug, which the Path-Reference Convention says must be italic, never backticked (`_audit-common.md`). The validator surfaces it as a missing symbol.
- **Suggested Fix**: Re-word the test doc to "`byte_view` hands the driver `size_of::<BenchPush>()` bytes; the `NoUninit` impl relies on no padding". Italicise the slug.

## Dimension 4: Audit-Finding Rot

Two syncs since base (`c4df4c91e` 10-20, `4a3e89d56` 10-21) touched 30+ skills. The gate reports no skill path or symbol advisories. A closed-issue callout scan of every `.claude/commands/**/*.md` against the 119-issue open snapshot (keyword "open / known-open / existing / do not re-file" within 60 chars of a closed `#NNNN`) produced 32 heuristic hits. All are false positives: correctly marked fixed, "open the xEdit definition", or historical. Sampled numeric claims are all within 20%: save hub 107 (skill "~107"), `crates/save` 3.2k ("~3.2k"), plugin `src` 78.0k ("~78k"), spt `src` 2.8k ("~2.8k"), scripting `src` 112.0k ("~111k"), `fragment/vm.rs` hub 136 ("~136"). All new code is owned in `_audit-owners.md` (`crates/terrain-height/`, `cloud_shadow.*`, `records/lmsw.rs`, `commands/terrain.rs`, `asset_provider/terrain_height.rs`).

#### TD4-2026-10-21-01: #6173 ("~25 known-open callouts name CLOSED issues") remains open after the syncs fixed it
- **Severity**: LOW
- **Dimension**: 4
- **Location**: GitHub #6173; `.claude/commands/**`
- **Status**: Existing: #6173 (tracker-hygiene follow-up, not a new defect)
- **Effort**: trivial
- **Description**: The scan above finds no callout that frames a closed issue as open. `c4df4c91e` and `4a3e89d56` rewrote the affected skills, and #6174–#6178 (the sibling TD4 items) were closed by their own commits. #6173 was not closed, so the next `/audit-tech-debt` dedups against a stale open tracker.
- **Suggested Fix**: Re-run the 10-19 list from `.claude/issues/6173/ISSUE.md` once and close #6173 with the commit hashes.

## Dimension 5: Stale Markers (skimmed: no new instance)

26 hits: 18 `XXXX` FourCC false positives (3 of them the `--type XXXX` usage placeholder in `dump_wths_refl.rs`), 2 FO4 `HACK` FourCC, 3 quoted upstream FIXMEs (`bgem.rs:137`, `bs_geometry.rs:646`, `misc/world.rs:279`), 1 documented `TBD` (`items.rs:378`), and the 2 `loading_screen.rs` TODOs that cite open #6194. Shader grep: 0. The `triangle.frag` GLSL-PathTracer MIT + Burley block is intact (L28–40).

## Dimension 6: Stub & Placeholder Implementations (skimmed: no new instance)

`unimplemented!`/`todo!()`: 0. One `panic!("not a resumed run")` is in `fragment/vm/tests.rs:4877` (test). No stub/placeholder comment was added in the window. Among the 209 new `pub` items, only the WRLD flag catalogue consts (`worldspace.rs`, #6046) and a hygiene fixture have no non-test reference. The constants are format documentation, so they are not debt. The prior dead items (#6189: `watr_to_params`, `populate_scene_fragments_from_pex`, `PackageFragmentSet`, …) are still live.

## Dimension 7: Magic Numbers (skimmed: no new instance)

Every new shader define (`CLOUD_SHADOW_*`, `ATMOSPHERE_*`, `CLOUD_STRATIFORM_*`, `CLOUD_OVERCAST_*`) is generated from `shader_constants_data.rs`. The Rust side sizes and dispatches the cloud-shadow map from `shader_constants::CLOUD_SHADOW_MAP_SIZE` (`cloud_shadow.rs:12,138,224`). `TERRAIN_HEIGHT_BLEND_BAND_DEFAULT` has one home (`core/settings/builtin.rs:39`). ESM: two more hand-indexed `u32::from_le_bytes(sub.data[0..4]…)` reads landed in `weather.rs` (`IAD` arm, `NAM1`, `parse_image_spaces`). They are folded into Existing #6188.

## Dimension 8: Dead Code & Backwards-Compat Cruft

`cargo machete --with-metadata`: clean. `allow(dead_code)`: 41 (unchanged per file). No `#[deprecated]` items, `// removed:` breadcrumbs or `_unused` survivors.

**Blocking clippy gate: red, already filed as REG-2026-10-21-01 (regression of #5849); not re-filed here.** This run reproduced it independently with `cargo clippy --workspace --keep-going -- -D warnings` on rustc 1.96.0 (exit 101). The four `byroredux-plugin` lib lints are `crates/plugin/src/esm/cell/index.rs:714,715`, `crates/plugin/src/esm/reader.rs:621` and `crates/plugin/src/esm/records/weather.rs:808`. Behind them, `cargo clippy -p byroredux` surfaces six bin lints: `asset_provider/object_graph.rs:260`, `cell_loader/load.rs:676`, `cell_loader/spawn.rs:403`, `commands/terrain.rs:17`, `scene/nif_loader.rs:519` and `terrain_height.rs:47`. Both lists match REG-01. One detail for REG-01's fix commit, already implied by the plugin dependency: the crates `--keep-going` leaves unlinted are `byroredux`, `byroredux-scripting`, `byroredux-game-detect`, `byro-launcher` and `byro-detect` (30 of 35 packages linted), so re-run the whole workspace gate after the fix, not only the bin. The non-blocking `--all-targets` step re-accrued to **36** unique sites, up from 20 on 10-19; that belongs to Existing #6193.

## Dimension 9: Test Hygiene

`every_ignore_attribute_carries_a_reason`, `no_tmp_scratch_examples_are_committed`, `no_committed_example_self_describes_as_disposable`, `no_test_attribute_is_stacked_on_another`, `example_targets_hold_their_per_crate_ceilings` (terrain-height ceiling 134→150 re-measured in the same commit, `e6f74069d`) and `every_real_data_pin_names_an_ignored_test_in_its_lane` are all live and none is `#[ignore]`d. The +20 new `#[ignore]`s all carry data-gate reasons. The non-data triage list holds only known items (#6185 skeever probe, the VM timing census, Workshop Framework e2e). Golden images are unchanged since 2026-09-21 (`cube_demo_60f.png`), with 24 shader commits since 10-15 (Existing #6182). No feature or CI-lane change in the window (the only workflow edit is #6183's pin move).

#### TD9-2026-10-21-01: four closed-HIGH fixes are guarded only by `#[ignore]`d real-data tests that are missing from `.github/real-data-pins.txt`
- **Severity**: MEDIUM (skill severity row: an `#[ignore]`d closed-HIGH guard absent from the pin list)
- **Dimension**: 9
- **Location**:
  - `crates/plugin/src/esm/reader.rs:1691` `installed_starfield_small_masters_do_not_collide` (#6118, HIGH, `0a2955d33`, lane `plugin-lib`)
  - `byroredux/src/asset_provider/tests/fo4_mswp_corpus.rs:19` `fo4_mswp_canvas_swap_matches_the_picture_frame_nif` (#5755, HIGH, `f81e6e722`, lane `bin`)
  - `byroredux/src/npc_spawn/tests.rs:921` `installed_default_idle_clips_exist_in_the_meshes_archive` (#6100, HIGH, `abd3d5001`, lane `bin`)
  - `byroredux/src/npc_spawn/tests.rs:3426` `installed_fallout_spawner_shells_resolve_a_model` (#6075, HIGH, `011ad0c07`, lane `bin`)
- **Status**: NEW (the 10-19 drift, #6183, was repaired on 10-20, and these four HIGH fixes landed the same and next day)
- **Effort**: trivial
- **Description**: The pin file's contract ("New closed-HIGH pins append here") was not followed by any of the window's four closed-HIGH fixes with real-data tests. Each fix also has hosted synthetic tests (for example `starfield_two_small_masters_do_not_share_a_global_id`, `mswp_bare_bnam_matches_every_nif_spelling`, `oblivion_and_fallout_idle_paths_are_not_shared`, `a_model_less_spawner_shell_takes_the_leveled_picks_model`), so the logic is pinned. The real-data leg that proved each fix against vanilla data is now runnable only by name. Three of the four skip silently when data is missing (`eprintln!` + `return`, Existing #6184 pattern); `installed_default_idle_clips…` calls `require_game_data`. The other 16 new ignored tests guard MEDIUM/LOW fixes or features and are correctly unpinned.
- **Evidence**: `grep -c '<fn>' .github/real-data-pins.txt` returns 0 for all four; issue labels were checked with `gh issue view`.
- **Impact**: `scripts/run-real-data-pins.sh` (the only real-data regression run, since no runner is registered, #6182) does not exercise these HIGH fixes. A data-dependent regression of #6118 (Starfield small-master FormID collisions) or #6075/#6100 (Fallout spawn models / Oblivion idles) would go unnoticed.
- **Related**: #6183 (closed), #6182, #6184, FNV-2026-10-21-03 (`run-real-data-pins.sh` miscount).
- **Suggested Fix**: Append the four rows. Optionally make `every_real_data_pin_names_an_ignored_test_in_its_lane` also fail when a test whose doc comment cites a `#NNNN` labelled `high` is ignored but unpinned. That needs a checked-in label list, since a hosted test cannot query GitHub; simpler is a `/fix-issue` checklist line.

## Deferred

- **`TD2-01`**: do it with, or right after, OBL-D4-2026-10-21-01's Oblivion NAM0 fix so that rule lands in the consolidated decoder rather than as a fourth arm.
- **`TD2-03`**: gated on SAVE-2026-10-21-04's decision for the nine `UNAUDITED` rows (park or justify). Table-drive the parking once the partition is settled.
- **`TD3-02`** loose-tier doc: write it after PAR-D6-01/02 and SF-2026-10-21-D2-01 settle the tier's behaviour (symlinks, case folding, Starfield layout), so the doc describes the fixed contract.
- **Not verified here**: golden-image currency (no engine launch); per-game corpus counts (need game data). The 2026-07 reports' CRITICAL/HIGH GitHub traces were not re-walked this run (Dim 4 checked them on 10-19).

## Cross-references (already reported this suite; not re-filed)

PHYS-D3-2026-10-21-04 (`combat.rs` FO4 ragdoll comment, plus sibling sites `asset_provider/actor_graph.rs:161` and `systems/graph_locomotion.rs:446`), REG-2026-10-21-01 (red clippy gate), NIF-D5-2026-10-21-03 (physal.md / nif-parser.md ragdoll doc rot), ESM-2026-10-21-05, REN-L-02, EXT-D6-2026-10-21-06, SCR-2026-10-21-11, AUD-2026-10-21-D3-03, PEX-D4-2026-10-21-02, LC-2026-10-21-D1-03, CHAR-2026-10-21-L-03, TOOL-D4-2026-10-21-02 (doc rot in their own areas); SAVE-2026-10-21-04/06 (parking partition); OBL-D4-2026-10-21-01 (WTHR); PERF-D3-01, CONC-2026-10-21-01/02 (terrain heights); ECS-2026-10-21-01/02/03 (the other red CI jobs); SF-2026-10-21-D2-01, PAR-D6-01/02 (loose tier).

Publish with `/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-10-21.md`.
