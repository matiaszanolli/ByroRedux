# Runtime Telemetry Audit — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: [AUDIT_RUNTIME_2026-09-22.md](AUDIT_RUNTIME_2026-09-22.md) (HEAD `ee6d3fb39`) · **Audited**: telemetry diff (fnv, fo3, oblivion, skyrim_se, fo4), harness/baseline integrity, bench schema tests, `capture.sh --self-test`, golden-frame default-lane guards, one validation-layer capture, commit attribution of every moved row · **Unchanged since baseline (skimmed)**: `byroredux/src/bench.rs`, `byroredux/tests/golden_frames.rs` (no commits since 2026-09-22)

This is the last leg of `/audit-suite --preset comprehensive` for 2026-09-29. It ran serially and spawned no sub-agents. Every engine launch went through `.claude/commands/audit-runtime/capture.sh`, one at a time. Before each launch the pre-flight checks were clean: no `byroredux` process (`pgrep -x`) and port 9876 free. After each launch the teardown was verified the same way. Scratch results are in `/tmp/audit/runtime/{fnv,fo3,oblivion,skyrim_se,fo4,starfield,bisect}.md`.

## Headline

1. **The harness could not run as documented.** `63c0aee3b` (2026-09-27) made the release debug server opt-in. `capture.sh` does not set `BYRO_DEBUG_SERVER=1`, so every capture ends in `FATAL debug server never came up` (RT-1). Every capture below was taken through the unmodified script, with `BYRO_DEBUG_SERVER=1` exported into its environment.
2. **All five baselined games show REGRESSION against the 2026-09-16 TSVs, and every moved row is attributed to a commit.** A bisect in a scratch worktree (still through `capture.sh`, 16 extra captures) traced every move to one of four intentional engine changes. None of them refreshed a TSV:
   - `b9e961eeb` (2026-09-23): new interior spawn ladder. The spawn now uses COCMarkerHeading, else the partner door's XTEL. This moves the `renderer-static` bench camera, so the whole draw split moves (RT-3). It also reclassifies 5 FO4 point lights as spot lights (RT-6).
   - `5570c221c` (2026-09-23): dismemberment caps are now identified from body-part data. FNV AtomicWrangler loses 92 entities, 92 draw commands and 92 skin slots.
   - `a070baaad` (2026-09-28): player body attachment. It adds about 177–195 entities per cell. **On Oblivion it also adds 4 duplicate `__max_default_light` directional lights to the GPU light list (RT-2, HIGH).**
   - `f87490826` (2026-09-28): REFR/ACHR "Initially Disabled" is now decoded. FNV loses 532 entities and FO4 loses 2301.
3. **R6a-regress-22 is very likely the camera move, not a performance regression (RT-4).** On FO4 `DmndDugoutInn01`, `b9e961eeb` alone takes the draw split from `1866/126b/7c` to `2379/651b/41c` and p50 from 8.60 to 18.20 ms. Entities (11603), lights (156) and TLAS (1818) are identical on both sides. That is R6a-regress-22's signature: batching "halved" at flat content and a fence-bound frame time that doubled. By the same route, FNV-D6-01's Prospector content drift (`cb44d99f6..a37fcba3c`) is most likely `5570c221c`. That attribution is inferred from AtomicWrangler, not measured on Prospector.
4. **Validation.** One Oblivion capture ran with `BYRO_VALIDATION=1` (sync validation) on the RTX 4070 Ti. It produced **0 validation errors**: neither `VUID-vkCmdDispatch-None-08114` nor the ReSTIR `SYNC-HAZARD-READ-AFTER-WRITE` appeared. See §Validation capture for what this does and does not settle for CONC-D2-2026-09-29-01/-02.
5. **Starfield: NOT RUN.** It was skipped before launch because of memory pressure (§Not run).

## Per-game status table

Current HEAD capture vs `.claude/audit-baselines/runtime/<game>-<cell>.tsv` (all rows from the 2026-09-16 renderer-static capture). Every run reported `mode=renderer-static` and `upscaler=fsr3/quality (source: default)`, with render extent 853x480 and output extent 1280x720. That is the engine's no-flag default since `5c7acfe2b`, so it is not a harness fault. The entity cross-check passed with delta 0 on every run.

| Game | Cell | Status | Δ vs baseline (gating first; advisory last) |
|------|------|--------|---------------------------------------------|
| fnv | FreesideAtomicWrangler | REGRESSION | entities 7414→**6985 (−5.8 %)**; tex_missing_base_color 1→1; mesh_cache_failed 0→0; lights 30 pt/0 dir exact; skin max/overflow 1364/0 exact; draws cmds 2204→2069 (improved), raster 283→1284, **batches 167→500 (×3.0)**, **gpu_calls 36→120 (×3.3)**; advisory: skin_live 217→106, fps 79.8→155.8, frame p50/p95/max 6.56/8.10/48.20 ms, tex_missing_all_slots 1→1 |
| fo3 | MegatonPlayerHouse | REGRESSION | entities 3543→**3626 (+2.3 %)**; tex/mesh 0/0 exact; lights 11/0 exact; skin 1364/0; draws cmds 1579→1514 (improved), raster 123→1285, **batches 114→511 (×4.5)**, **gpu_calls 12→65 (×5.4)**; advisory: skin_live 7→3, fps 74.7→95.6, frame 10.42/11.53/40.33 ms, all_slots 0→0 |
| oblivion | ICMarketDistrictTheGildedCarafe | REGRESSION | entities 745→**934 (+25.4 %)**; tex/mesh 0/0; light_count_point 8→8; **light_count_directional 2→6**; skin 1364/0; draws cmds 330→335 (+1.5 %, inside ×1.1), raster 132→20, batches 78→18, gpu_calls 5→2 (all improved); advisory: skin_live 4→9, fps 109.0→168.2, frame 6.77/7.88/16.68 ms, all_slots 0→0 |
| skyrim_se | WhiterunDragonsreach | REGRESSION | entities 9461→9499 (+0.4 %, in band); tex/mesh 0/0; lights 28/0 exact; skin 1364/0; draws cmds 2458→2494 (+1.5 %, inside), raster 14→1786, **batches 13→643 (×49)**, **gpu_calls 3→35 (×11.7)**; advisory: skin_live 133→122, fps 110.9→192.5, frame 4.95/6.51/42.45 ms, all_slots 2→2 |
| fo4 | InstituteBioScience | REGRESSION | entities 18969→**16885 (−11.0 %)**; tex_missing_base_color 1→1; mesh 0→0; **light_count_point 685→680** (dir 0→0); skin 1364/0; draws cmds 3969→3971 (inside), raster 256→**3618 (now above the 3000 `DRAW_SORT_PARALLEL_THRESHOLD`)**, **batches 196→723 (×3.7)**, **gpu_calls 13→110 (×8.5)**; advisory: skin_live 264→132, fps 79.5→25.2, frame 39.23/40.50/160.50 ms, all_slots 2→2 |
| starfield | citycydoniamainlevel | NOT RUN | no baseline; skipped before launch for memory pressure (§Not run) |

**Rows that held on all five games:** `bench_mode` (renderer-static), `tex_missing_base_color`, `mesh_cache_failed_count`, `skin_pool_max` (1364), `skin_pool_overflow_attempts` (0). The draw-split invariant held in every capture (`batches ≤ raster ≤ cmds`, `gpu_calls ≤ 2 × batches`), so no stale or partial baseline row was found. **Improved `≤` rows are not a signal to tighten yet:** fnv/fo3 `cmds` and the whole Oblivion draw split are camera or content moves (see below). Regenerate them in one reviewed capture, not piecemeal.

**Precombine runtime coverage (requested by the FO4 audit):** the HEAD InstituteBioScience load ran the worker-side precombine path end to end:
- CSG opened with 32370 objects / 6841 chunks.
- 13 hashes produced 1722 spawned entities, and 2 REFRs were absorbed.
- `precombine_timing` reported `prepare_total_ms=72.90`, `spawn_total_ms=1085.74` and `tex_extract_ms=993.89`.
- `tex_missing_base_color` stayed at 1 and `mesh_cache_failed` at 0 (the baseline).

This is not a test of `merge_precombine_materials` (FO4-D1-01): no visual check was made.

## Attribution bisect (how every moved row was assigned)

**Method.**
- A detached worktree was checked out at each probe commit and built in release mode with the main `target/` shared.
- HEAD's `capture.sh` was run from the worktree root, with the same renderer-static camera, 240 frames, isolated settings and cross-check as the main captures.
- Every probe passed its pre-flight and cross-check.
- One probe (`a35778d2e`) **failed to build**, so its reported numbers came from a stale binary. They were discarded; this is noted because an earlier reading from that probe looked non-monotonic.
- `cb44d99f6` (= `b9e961eeb^`) reproduces the committed baselines **bit-for-bit**:
  - FNV: 7414 / 2204/167b/36c raster 283 / skin 217.
  - FO4: 3969/196b/13c raster 256 / 685 Point / skin 264.
  - Oblivion: 757 / 335/78b/5c raster 132 / 2 dir.

  The 2026-09-22 report's pass is consistent with this, and every move below lies after `cb44d99f6`.

| Commit | Oblivion GildedCarafe | FNV AtomicWrangler | FO4 InstituteBioScience | FO4 DmndDugoutInn01 |
|---|---|---|---|---|
| `cb44d99f6` | 757 · 335/78b/5c r132 · dir 2 · gpu lights 10 · camera faces a wall 3.9 BU away | 7414 · 2204/167b/36c r283 · skin 217 | 18999 · 3969/196b/13c r256 · 685 Pt · skin 264 · p50 9.97 ms | 11603 · 1866/126b/7c r163 · p50 8.60 ms · gpu_main 8.82 ms |
| `b9e961eeb` spawn ladder | 757 · **335/18b/2c r20** · camera at partner-XTEL door arrival, 468.5 BU to centroid | 7414 · **2244/700b/122c r1376** | 18999 · **4142/894b/115c r3777** · **680 Pt + 5 Spot** · p50 41.58 ms | 11603 · **2379/651b/41c r1785** · p50 18.20 ms · gpu_main 15.71 ms |
| `5570c221c` caps | — | **7322** · 2152/513b/119c · **skin 125** | — | — |
| `a070baaad` player body | **934 · dir 6 · gpu lights 14** | (+195 in `373f6a4cc..6ceee1df5`, window contains it) | (+187 in window) | — |
| `f87490826` Initially Disabled | — | **6985** · 2069/500b/120c · skin 106 | **16885** · 3971/723b/110c · skin 132 | — |
| HEAD `9fcfdc3fc` | 934 · 335/18b/2c · dir 6 (state_hash identical to `a070baaad`) | 6985 (identical to `f87490826`) | 16885 (identical to `f87490826`) | — |

Batch counts at a fixed camera also drift between `b9e961eeb` and HEAD (FNV 700b → 525b → 513b → 500b). That drift is small next to the camera step and was not bisected. FO3 was not bisected: its moves have the same signature (camera = COCMarkerHeading per the engine log, entities +2.3 %). Skyrim was not bisected either: its camera is COCMarkerHeading per the log, and entities are in band.

## Gate matrix

| Gate | Result |
|------|--------|
| Telemetry diff (`capture.sh`), 5 baselined games | **REGRESSION ×5**. All five are attributed to intentional commits except RT-2. The plain script **fails** without `BYRO_DEBUG_SERVER=1` (RT-1). |
| Telemetry diff, starfield | **NOT RUN**: memory pressure, no baseline |
| Harness self-test (`capture.sh --self-test`) | PASS |
| Baseline schema tests (`cargo test -p byroredux bench::`) | PASS, 9/9 (`every_baseline_carries_the_full_gating_metric_set`, `every_baseline_records_the_harness_bench_mode`, `draw_split_rows_are_internally_ordered` included) |
| Golden frames, default-lane guards (`cargo test --release -p byroredux --test golden_frames`) | PASS, 4/4 (`committed_baseline_matches_the_current_invocation` included) |
| Golden frames, device captures (`--ignored`: `cube_demo_golden_frame`, `combustion_lab_golden_frame`) | **NOT RUN**. They launch the engine outside `capture.sh`, which this run's constraints forbid. |
| Playable slice (`p0`, `p1`, `p2`, `p3-hud`, `p3-player-body`, `p5`, `w1`) | **NOT RUN**. Same constraint. Not SKIP, and not pass. |
| Smoke contracts (`scripts/check-playable-smoke-contracts.sh`) | **NOT RUN, deliberately.** Re-verified statically that it still neutralises only `BYROREDUX_{SKYRIMSE,FNV,FO3,FO4}_DATA` (lines 37-40, 62), not `BYROREDUX_OBLIVION_DATA`. On this box it would launch the real Oblivion P0 gate. This is Existing: REG-2026-09-29-02 (not re-filed). |
| Milestone smokes, renderer-correctness gates (`cornell_rt_oracle`, `interior-godrays.sh`, `check-bench-determinism.sh`, `material-provider-matrix.sh`, `fsr-bench-matrix.sh`), CI lanes | **NOT RUN**. Same constraint. Note that `m34-day-night.sh`, `m-trees.sh`, `m43`/`m47` and `scripts/material-provider-matrix.sh` carry the same missing opt-in as RT-1 (EXT-D7-2026-09-29-01 / SPT-D3-01). |
| Extra: validation-layer capture (`BYRO_VALIDATION=1`, Oblivion, HEAD) | PASS: 0 validation errors, 9 benign SPIR-V-interface warnings (§Validation capture) |

## Harness and baseline integrity

- `git log --since=2026-09-22` on the integrity paths:
  - `capture.sh`: touched once, by `efc059f3a` (#4947 settings isolation). It is present and working: each run wrote a fresh `<label>.settings.toml`, and `upscaler=` reported `source: default`.
  - `.claude/audit-baselines/runtime/`: touched once, by `c37714ba6` (#4771, the README path fix for the 09-22 RT-1; verified fixed).
  - `bench.rs` and `golden_frames.rs`: no commits.
  - `scripts/check-playable-smoke-contracts.sh`: touched by `bb5c2c706`, `63c0aee3b` and `55dd8fa7e`. The Oblivion neutralisation gap persists (above).
- **Nothing refreshed the TSVs, while four engine commits moved gating rows.** The README rule is that a TSV diff is committed in the same engine commit. `b9e961eeb`, `5570c221c`, `a070baaad` and `f87490826` each broke it. `b9e961eeb`'s subject line ("Enhance physical lighting documentation…") does not mention the spawn-ladder change it carries. That is part of why neither the bench-of-record nor this harness caught the camera move.
- **The old renderer-static camera was not representative.** At `cb44d99f6` the Oblivion bench camera sat 3.9 BU from a wall (the `forward ray cast` subject distance) and pointed down-forward (`0,-0.447,-0.894`, the old loose-NIF framing offset). That explains Skyrim's historical raster 14. The new poses frame the room (Oblivion 468.5 BU, FO4 2587 BU, FNV 1215 BU, centroid-measured). Regenerating onto them is an improvement in gate quality, not only a bookkeeping step.
- The skill's metric list, `REQUIRED_METRICS` and the TSV keys agree (the schema tests pass). One gap: the schema has no spot-light row, so a Point→Spot reclassification reads as "lights lost" (RT-6).
- Harness side effect handled during the bisect:
  - Sharing `target/` between the worktree and the main tree left stale workspace rlibs. The next main-tree build then regenerated `crates/renderer/shaders/include/shader_constants.glsl` from the stale core crate.
  - The file was restored with `git checkout --`, and the release artifacts of all 34 workspace members were cleaned (`cargo clean --release -p …`).
  - HEAD was rebuilt, and a recheck capture reproduced HEAD's Oblivion `state_hash f98ca13dc5616c5a` exactly.
  - The working tree is clean apart from the audit reports. Anyone reusing this bisect recipe should use a separate `CARGO_TARGET_DIR` or clean afterwards.

## Validation capture (CONC-D2-2026-09-29-01 / -02)

`BYRO_VALIDATION=1 BYRO_DEBUG_SERVER=1 capture.sh --game oblivion --cell ICMarketDistrictTheGildedCarafe` ran at HEAD on the RTX 4070 Ti with `VK_LAYER_KHRONOS_validation` and sync validation active. The engine logged `Vulkan VALIDATION ENABLED — sync-validation`. Conditions were `rt_tier=3`, TLAS 335, caustic splat dispatched (`gpu_caustic_splat=0.014`), and ReSTIR reservoirs created. The telemetry matched the non-validation run exactly (same `state_hash`).
- **0 `ERROR`-level validation messages** over the load plus 240 frames plus the held session. There was no `VUID-vkCmdDispatch-None-08114` and no `SYNC-HAZARD-*`.
- 9 `WARN` messages, all `(SPIR-V Interface) VK_SHADER_STAGE_VERTEX_BIT has an Output value declared at Location N … no corresponding Input`. They are emitted right after `Graphics pipelines created (opaque early tests=true …)`, i.e. by the `186234944` early-test variant, whose fragment stage reads fewer varyings. The layer itself says "This is not invalid". This is informational and not filed.
- **What this settles:**
  - For -02, it is consistent with the CONC report's trigger analysis: 08114 needs "no global geometry SSBO", and real cells always build one. The pipeline attribution (caustics) **cannot be confirmed from a cell capture**, because the error does not fire here. Confirming it needs the bare demo scene, which `capture.sh` (`--game` required) cannot launch.
  - For -01, the ReSTIR RAW hazard does not reproduce on a real device with real content. Either it is lavapipe/demo-specific, or NVIDIA's path does not trip it. The -01 finding stands on its code reading. This run is a data point, not a refutation.

## Findings

Severity follows `_audit-severity.md` and the skill's Phase 4 rules. Per-metric moves that share one root cause are grouped under one heading, and the heading names every affected game.

### RT-1: `capture.sh` never opts the release debug server in — every capture FATALs since `63c0aee3b`
- **Severity**: MEDIUM
- **Dimension**: Harness integrity
- **Location**: `.claude/commands/audit-runtime/capture.sh:388-392` (launch env), `:425` (the resulting die); gate at `byroredux/src/main.rs:78-80` (`debug_server_allowed`) and `:1022-1039`
- **Status**: NEW. It is the runtime-harness instance of the class filed today as EXT-D7-2026-09-29-01 (m34, m-exteriors) and SPT-D3-01 (m-trees). Neither report names `capture.sh`. There is no issue for `BYRO_DEBUG_SERVER` / `capture.sh` in open or closed issues.
- **Description**: `63c0aee3b` (2026-09-27) made the debug server opt-in for release builds: `debug_build || opt_in ∈ {1,true,yes}`. `capture.sh` requires a release build and launches it with only `BYROREDUX_SETTINGS_PATH` set. The engine therefore logs `Debug server disabled (set BYRO_DEBUG_SERVER=1 to opt in)` and then `bench-hold-unavailable: … the debug server did not bind`. The script's 90 s ping loop then dies. The skill's documented invocation, and CLAUDE.md's `--bench-hold` → `byro-dbg` pattern, are both silently broken for this audit. The smoke scripts under `docs/smoke-tests/` received the opt-in in the same commit; this harness did not.
- **Evidence**: first FNV attempt at HEAD, log kept as `/tmp/audit/runtime/fnv-attempt1-no-optin.engine.log`. It shows the `bench:` line printed, then `bench-hold-unavailable`, then `capture: FATAL debug server never came up`. Re-running the unmodified script with `BYRO_DEBUG_SERVER=1` in the environment succeeded (`dbg up at 1s`).
- **Impact**: `/audit-runtime` cannot produce a result as documented. An auditor who follows the skill gets a FATAL on every game and either reports "NOT RUN" or hand-rolls a launch, which the skill forbids. The engine log already carries a `bench-hold-unavailable` line that says exactly why, but the script spends the full 90 s before failing on a generic message.
- **Related**: EXT-D7-2026-09-29-01, SPT-D3-01, `63c0aee3b`.
- **Suggested Fix**: Add `BYRO_DEBUG_SERVER=1` to the `capture.sh` launch environment beside `BYROREDUX_SETTINGS_PATH`. Fail fast when the engine log shows `bench-hold-unavailable`. Add a self-test or static pin that the launch line carries the opt-in, and mention the opt-in in the SKILL.md invocation paragraph.

### RT-2: `light_count_directional` 2 → 6 on oblivion ICMarketDistrictTheGildedCarafe — the player body re-spawns the `__max_default_light` exporter-artifact pair
- **Severity**: HIGH (rendering correctness: four extra full-intensity directional lights reach the GPU light buffer)
- **Dimension**: Telemetry diff (exact metric)
- **Location**: baseline row `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv` `light_count_directional 2`. Code: `byroredux/src/player_body.rs` (`attach_player_body`, `a070baaad`) → `load_nif_bytes_with_skeleton` → `byroredux/src/scene/nif_loader.rs:565` → `spawn_nif_lights` (`byroredux/src/cell_loader/spawn.rs:1093-1195`, name de-dup at `:1125-1131`).
- **Status**: NEW (Related: #3557 / RT-11, CLOSED. Its de-dup exists but does not collapse these. #4938 is a different LIGH-falloff issue.)
- **Game / Cell / Baseline / Current**: oblivion / ICMarketDistrictTheGildedCarafe / 2 / 6
- **Description**: `light.dump` at HEAD lists six `kind=Directional source=nif/synthetic` emitters, all named `__max_default_light`, as three ± pairs:
  - 120/121: the baseline's NPC pair.
  - 839/840 and 855/856: new.

  Entity 839's direction `[-0.4467,0.7444,0.4963]` and radiance are byte-identical to entity 120's. That is exactly the duplicate #3557 was meant to suppress with `is_known_exporter_artifact_light_name(..) && world.find_by_name(..)`. Bisected: `69266fb82` (= `a070baaad^`) has dir 2 and `lights=10`; `a070baaad` has dir 6 and `lights=14`. The bench line's `lights=` is `gpu_lights.len()` (`byroredux/src/bench.rs:391`), so the four extras are submitted to the GPU. `HiddenFirstPerson` hides the body's meshes but not the lights under them.
- **Evidence**: `/tmp/audit/runtime/oblivion-ICMarketDistrictTheGildedCarafe.telem.txt` (`LightSource emitters: 14`); bisect rows above. `state_hash` at HEAD equals `a070baaad`'s.
- **Impact**: Every Oblivion cell with the player character gains two white directional lights per body NIF that carries the exporter artifact. They add unauthored fill light, and they add it in first person, where the body itself is invisible. The fact that the pre-existing NPC pair 120/121 also survives suggests the #3557 de-dup is not effective on the NPC/loose-NIF path at all. That is a wider question than this commit.
- **Related**: #3557, #4938, `a070baaad`.
- **Suggested Fix**: Find out why `find_by_name("__max_default_light")` does not match on the `load_nif_bytes_with_skeleton` path: pool lookup, `Name` timing, or storage. Either fix the de-dup there, or skip exporter-artifact lights for actor-part NIFs altogether, since actor bodies should not emit scene lights. Pin it with a test that assembles an actor with the artifact twice and asserts one emitter.

### RT-3: `bench_draws_batches` / `bench_draws_gpu_calls` over ×1.1 on fnv, fo3, skyrim_se, fo4 (and the whole Oblivion split moved) — the bench camera moved with `b9e961eeb`'s spawn ladder
- **Severity**: MEDIUM (skill Phase 4: count against direction; the baselines are now stale, with no code regression shown)
- **Dimension**: Telemetry diff (draw split) / baseline integrity
- **Location**: `bench_draws_batches` / `bench_draws_gpu_calls` / `bench_draws_raster_cmds` rows in all five `.claude/audit-baselines/runtime/*.tsv`. Code: `byroredux/src/cell_loader/interior_spawn.rs` (added in `b9e961eeb`).
- **Status**: NEW (no issue or audit names the spawn ladder as the draw-split or bench mover)
- **Game / Cell / Baseline → Current (batches, gpu_calls, raster)**: fnv 167→500, 36→120, 283→1284 · fo3 114→511, 12→65, 123→1285 · skyrim_se 13→643, 3→35, 14→1786 · fo4 196→723, 13→110, 256→3618 · oblivion 78→18, 5→2, 132→20 (decreased)
- **Description**: `renderer-static` holds the authored camera, and for a direct interior load that camera sits at the player's eyes on the spawn pose. `b9e961eeb` replaced the old door-placement/bbox heuristic with a ladder: COCMarkerHeading first, then the partner door's XTEL. HEAD's engine logs show COCMarkerHeading for AtomicWrangler, MegatonPlayerHouse, WhiterunDragonsreach and InstituteBioScience, and a partner-XTEL arrival for GildedCarafe. A new pose means a new frustum, which moves the raster-visible prefix and with it batching and the indirect-call count. Bisect: the draw split is unchanged at `cb44d99f6` (= baseline) and moves at `b9e961eeb` with entity and light counts identical. Oblivion 335/78b/5c r132 → 335/18b/2c r20, FNV 2204/167b/36c → 2244/700b/122c, FO4 3969/196b/13c → 4142/894b/115c.
- **Evidence**: bisect table above; `Interior spawn for '<cell>': …` and `bench camera 'static' … from (…)` lines in each `*.engine.log`.
- **Impact**: None of the draw rows is a usable regression guard until regenerated. The old camera for several cells faced a wall (Oblivion subject distance 3.9 BU; Skyrim raster 14), so the old gate barely exercised raster batching. FO4's raster prefix is now 3618, which puts the renderer-static gate on the **parallel** sort branch (threshold 3000) for the first time. The `render/mod.rs` note says the 2026-09-03 captures were all serial. Any R6a-style "batching regression" read across `b9e961eeb` compares two different views (RT-4).
- **Related**: RT-4, RT-5, FNV-D6-01, PERF 2026-09-29 (R6a-regress-22 static attribution).
- **Suggested Fix**: Review a HEAD screenshot of each new pose, then regenerate all five TSVs in one renderer-static `--regen` run, with a `# regenerated:` header naming `b9e961eeb` / `5570c221c` / `a070baaad` / `f87490826`. Consider recording `camera_pos` / `camera_forward` in the TSVs, or asserting them in `capture.sh`, so a pose change fails loudly instead of masquerading as a batching regression.

### RT-4: R6a-regress-22 ("FO4 frame time doubled, batching halved at flat draw counts") reproduces as a pure bench-camera move at `b9e961eeb`
- **Severity**: MEDIUM (the bench-of-record and its open regression rest on a false "content unchanged" premise; the prescribed bisect targets the wrong kind of change)
- **Dimension**: Benchmark integrity / attribution
- **Location**: `ROADMAP.md:93-94` (Next: bisect R6a-regress-22) and `:152-160` (the "Entity, light and TLAS counts match … so the content is unchanged" inference); spawn ladder `byroredux/src/cell_loader/interior_spawn.rs`
- **Status**: NEW. R6a-regress-22 is a ROADMAP item with no issue. PERF 2026-09-29 could not attribute it. This also supplies the likely cause for FNV-2026-09-29-D6-01.
- **Description**: The ROADMAP rules out content change because entity, light and TLAS counts match. The camera origin, though, is not content. The bench-of-record's stepped camera starts from the spawn pose (the pre-ladder door-frame view was also its orbit origin, per the project's interior-spawn notes), and `b9e961eeb` (2026-09-23) moved that pose into the room for every COC-marked interior. It lies inside R6a-regress-22's window `4c9a5b36..99933f87b`. Measured on the R6a scene itself, FO4 `DmndDugoutInn01`, renderer-static:
  - `cb44d99f6`: `1866/126b/7c` r163, p50 8.60 ms, gpu_main 8.82 ms.
  - `b9e961eeb`: `2379/651b/41c` r1785, p50 18.20 ms, gpu_main 15.71 ms.
  - Entities 11603, lights 156 and TLAS 1818 are identical on both.

  The ROADMAP's own Dugout figures (`1935/326b/12c` → `2367/659b/43c`, fence 4.13 → 18.95 ms) have the same shape. FO4 InstituteBioScience shows the same: p50 9.97 → 41.58 ms at `b9e961eeb`, with nothing else changed. For FNV-D6-01 (Prospector −32 entities / −24 draws / −35 TLAS in `cb44d99f6..a37fcba3c`), the only entity-changing commit in that window on the AtomicWrangler bisect is `5570c221c` (dismemberment caps: −92 entities, −92 draws). `a070baaad` and `f87490826` both land after `a37fcba3c`.
- **Evidence**: `/tmp/audit/runtime/bisect-fo4-DmndDugoutInn01-{cb44d99f6,b9e961eeb}/`; bisect table above.
- **Impact**: Time spent bisecting renderer code for a "regression" is misdirected. The before/after rows in the bench-of-record compare different views, so "FSR recovery" and per-scene deltas across `b9e961eeb` are not apples to apples. Some real cost may still hide under the camera move: the 186234944 reservoir-clear barrier and early-test split named by PERF. That can only be read once both sides use the same pose.
- **Related**: RT-3, FNV-2026-09-29-D6-01, PERF 2026-09-29 (R6a static attribution), `186234944`.
- **Suggested Fix**: Re-run the `4c9a5b36` vs `99933f87b` control with the camera pinned to one pose on both sides. `scripts/fsr-bench-matrix.sh` would need a pose override or a pre-`b9e961eeb` spawn; that is not verified to exist. Then re-state R6a-regress-22 as whatever residual survives. Record the spawn pose in the bench TSV header so a future pose change is visible.

### RT-5: `entities_total` outside ±2 % on fnv (−5.8 %), oblivion (+25.4 %), fo4 (−11.0 %) — three intentional content changes landed without a TSV refresh
- **Severity**: MEDIUM
- **Dimension**: Telemetry diff (tolerance metric) / baseline integrity
- **Location**: `entities_total` rows in `fnv-FreesideAtomicWrangler.tsv` (7414), `oblivion-ICMarketDistrictTheGildedCarafe.tsv` (745), `fo4-InstituteBioScience.tsv` (18969)
- **Status**: NEW (as a baseline finding; the underlying commits close #4813/#4814/#4820 and similar)
- **Game / Cell / Baseline / Current**: fnv 7414 → 6985 · oblivion 745 → 934 · fo4 18969 → 16885
- **Description**: The bisect assigns each step:
  - FNV: `5570c221c` −92 (dismemberment caps, with −92 draws and −92 `skin_pool_live`), then about +195 in a window containing `a070baaad` (player body), then `f87490826` −532 (Initially Disabled refs withheld).
  - Oblivion: `a070baaad` +177 (the baseline 745 vs `cb44d99f6`'s 757 is the known in-band creep).
  - FO4: +187 in the `a070baaad` window, then `f87490826` −2301 (with `skin_pool_live` 229 → 132).

  Each is an intended behaviour change. None refreshed the TSV, against the README's same-commit rule.
- **Evidence**: bisect table above.
- **Impact**: The ±2 % entity gate is inert until regenerated. Every future capture will "fail" for these known reasons and bury a real move. The `f87490826` drop is also a behaviour change worth a visual spot check: are quest-gated actors now absent where the game also hides them? That was not verified here.
- **Related**: RT-3, RT-6, #4813, #4814, #4820, FNV-D6-01.
- **Suggested Fix**: Fold into RT-3's single regeneration, and name all three commits in the `# regenerated:` header.

### RT-6: `light_count_point` 685 → 680 on fo4 InstituteBioScience — 5 emitters reclassified Point → Spot by `b9e961eeb`; the schema has no spot row
- **Severity**: MEDIUM (exact metric moved; the schema cannot express the change)
- **Dimension**: Telemetry diff (exact metric) / harness schema
- **Location**: `fo4-InstituteBioScience.tsv` `light_count_point 685`; `byroredux/src/bench.rs` `REQUIRED_METRICS`; skill Phase 3 metric table
- **Status**: NEW
- **Game / Cell / Baseline / Current**: fo4 / InstituteBioScience / 685 Point + 0 Spot / 680 Point + 5 Spot (`LightSource emitters: 685`, `lights=685` unchanged)
- **Description**: `cb44d99f6` dumps 685 `kind=Point`. `b9e961eeb` dumps 680 `kind=Point` plus 5 `kind=Spot`. That commit adds `crates/plugin/examples/spot_light_census.rs` and changes `crates/core/src/ecs/components/light.rs` and `lighting.rs`. No light was lost. The metric table counts only Point and Directional, so a correct reclassification reads as a regression, and a wrong one would go unnoticed on the Spot side.
- **Evidence**: `/tmp/audit/runtime/bisect-fo4-InstituteBioScience-{cb44d99f6,b9e961eeb}/*.telem.txt` (kind tallies).
- **Impact**: A false exact-metric failure on FO4, and a blind spot for spot-light regressions on every game.
- **Related**: RT-3, RT-5.
- **Suggested Fix**: Add `light_count_spot` to `REQUIRED_METRICS`, the TSVs and the skill's Phase 3 table (exact direction). Regenerate FO4 with 680/5 once the 5 spot classifications have been spot-checked against their LIGH records.

### RT-7: `entities_total` +2.34 % on fo3 MegatonPlayerHouse
- **Severity**: LOW (tolerance metric drifted within ±5 %)
- **Dimension**: Telemetry diff
- **Location**: `fo3-MegatonPlayerHouse.tsv` `entities_total 3543`
- **Status**: NEW
- **Game / Cell / Baseline / Current**: fo3 / MegatonPlayerHouse / 3543 / 3626
- **Description**: Just outside the ±2 % band (budget 71, delta 83). Not bisected. The FO3 run shows the same camera and player-body signature as the others: COCMarkerHeading spawn, raster 123→1285, `skin_pool_live` 7→3. The most likely net is `a070baaad` (+) and `f87490826` / `5570c221c` (−).
- **Suggested Fix**: Include it in RT-3's regeneration. Bisect only if it does not settle there.

## Not run

- **Starfield** (`citycydoniamainlevel`): skipped before launch.
  - `free -g` across the session showed 29 GB total, 10–12 GB available and **28–29 of 31 GB swap in use**.
  - The skill requires ample free RAM for this cell's ~95k-BLAS cold load, and the last two attempts (2026-09-11) safety-aborted.
  - No baseline exists, so a run could only have produced BASELINE CREATED, which this audit may not write.

  It remains **unverified**. Use `--sf-smoke` for coverage.
- **Golden-frame device captures, playable-slice gates, milestone smokes, renderer-correctness gates, CI lanes**: not executed, because this run's constraints allow engine launches only through `capture.sh`. They are recorded as NOT RUN, never as SKIP or pass.
- No `--regen`: all baselines are untouched. RT-3/5/6/7 call for one reviewed regeneration.

## Cleanup

- Final `pgrep -x byroredux; pgrep -x byro-dbg` found no process. No engine or debug CLI launched by this audit is left running. Every capture tore down through `capture.sh`'s real-PID kill.
- The bisect worktree `/mnt/data/tmp/rt-bisect` was removed; its `target` symlink was unlinked first, so the main `target/` is intact.
- `target/release/byroredux` is rebuilt at HEAD `9fcfdc3fc`, and the working tree is clean apart from the untracked `docs/audits/*_2026-09-29.md` reports.
- `/tmp/audit/runtime/` is **kept**, not `rm -rf`'d as in skill Phase 6. It holds the per-game notes, the bisect logs and the validation log that this report cites, because the suite orchestrator reconciles against them.
- Other worktrees on the box (`/mnt/data/tmp/ctl-wt` at `b9e961eeb`, `/mnt/data/tmp/head-wt`, `target/bench-control-*`) predate this audit and were not touched.

Suggest `/audit-publish docs/audits/AUDIT_RUNTIME_2026-09-29.md`.
