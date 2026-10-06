# ByroRedux — Roadmap

A Rust + Vulkan rebuild of the Gamebryo/Creation engine lineage, targeting
the full Oblivion → Starfield range. This document is the live source of
truth for **what works, what's next, and why**. Session narratives live in
[HISTORY.md](HISTORY.md); per-commit archaeology lives in `git log`; sections
retired from this file (superseded bench records, closed known issues, full
milestone implementation logs) live verbatim in
[`docs/archive/roadmap-history.md`](docs/archive/roadmap-history.md).

**Keeping this document honest.** Run `/session-close` at the end of each
working session. It diffs stated facts against ground truth (test count,
LOC, open issues, bench freshness, completeness of repro commands) and
proposes a single synchronised edit across ROADMAP / HISTORY / README.
Ritual-driven, not hook-driven — one checkpoint per session, not N per
commit. Live state only: when something closes, it becomes a one-liner under
[Completed Milestones](#completed-milestones) or leaves the file.

---

## Status

**Last session close:** Session 95, 2026-10-06, HEAD `eb479269f`. Workspace
numbers (tests, LOC, files, issue dirs) live in [Project Stats](#project-stats)
only.

**Gates.** `rust-toolchain.toml` pins rustc 1.96.0 (#5308), and on it
`cargo clippy --workspace --all-targets` is clean. Five lints this session's
code added were fixed at the Session 95 close. CI gates only the
non-`--all-targets` form; hosted CI status is under
[Known Issues](#known-issues). The bench-of-record below was taken at
`a37fcba3c`, 390 commits before this close (R6a-stale-25). Its one open
regression, **R6a-regress-22**, turned out to be
the bench camera moving with `b9e961eeb`'s interior spawn ladder, not
renderer code (#5128, 2026-09-30); a small residual stays open — the
top-of-frame all-slots fence wait that dominates GPU-bound frames
(#5365, the tracked throughput half of closed #4606/#5117).

**What works today.**
- **Content loading.** Interior cells load and render from unmodified game
  data for Oblivion, FO3, FNV, Skyrim SE and FO4, plus a walkable Starfield
  Cydonia interior, all through one `cell_loader`. Exterior grids load with
  LAND terrain and splatting, stream as the player moves (M40), and draw
  distant LOD: `.btr`/`.bto` with a per-game band ladder on Skyrim/FO4,
  `_far.nif` on Oblivion/FO3/FNV (M35). FO4 precombines decode from CSG (M49).
- **Parsers.** NIF parses across seven games, 100% recoverable everywhere; the
  [compatibility matrix](#compatibility-matrix) is the single home for rates.
  Archives: BSA v103/v104/v105, BA2 v1/v2/v3/v7/v8. ESM record totals come
  from the floor-based `parse_rate_fnv_esm` test — prefer its output to any
  pinned count.
- **Renderer.** RT-first: ray-query shadows, ReSTIR-DI, RT reflections,
  bounded material-aware path-traced GI, SVGF, TAA and FSR 3.1 (Quality is the
  default). Also: clustered lights, volumetric froxel fog with a transported
  combustion solver, bloom, RT water, EXAL ground cover, and interior
  godrays / sky apertures (a vertical slice,
  [`interior-godrays-status.md`](docs/engine/interior-godrays-status.md)).
  Exposure is auto-metered by default (`a070baaad`), with ACES or AgX. BLAS
  compaction and LRU eviction, TLAS refit, GPU skinning.
- **Actors.** NPCs spawn with skeleton, body, FaceGen head and outfit (M41).
  Seven AI package procedures run by default, with authored walk clips and
  KCC-backed steps (M42). Faction hostility starts combat on sight and
  disengages after a grace period (#4414, #4816). FNV-era ragdolls run on
  Rapier (M41.x).
- **Player.** Rapier kinematic character controller (M28.5), swimming and
  diving (WATAL W1), a visual body with a first/third-person toggle
  (`a070baaad`), and on the P2 fixture a melee → damage → death → loot loop.
  Native HUD vitals and objective text. Native F5/F9 quicksave/quickload
  through the deferred player-save queue, with quest state, cross-cell door
  saves, graceful quit and a validation-clean 30-minute soak all gated
  (P5, 2026-10-01; soak's intermittent restore defect tracked in #5155).
- **Scripting.** ECS-native event hooks and condition evaluator
  (M47.0/M47.1). Compiled `.pex` is decompiled and lowered through a
  recognizer chain, with QUST fragments and aliases, and SCEN/PACK scenes
  (MQ101 plays end-to-end). An SCDA interpreter runs Oblivion quest scripts
  (M47.3). A first magic-runtime slice covers spell lists and the RACE
  `SPLO` racial pair (#4415).
  Dialogue: NPC activation selects a topic, a native response surface
  presents the INFO, and its `TIF_` fragments dispatch (P4).
- **Save/load.** Full-ECS snapshot with validation gates, atomic write and
  live load-apply (M45/M45.1).
- **Audio.** kira spatial audio: footsteps, ambient, music, per-cell reverb,
  water routing (M44).
- **UI.** Ruffle-hosted Scaleform menus with Skyrim (AVM1) and FO4 (AVM2) host
  profiles (R4). Native MenuXml HUDs for Oblivion/FO3/FNV, and the vanilla
  Skyrim/FO4 `hudmenu.swf` via `--hud` (M48.4–M48.7).

**What doesn't work yet.**
- AI beyond locomotion: 10 of ~17 package procedures need subsystems that do
  not exist (item use, package combat, magic, dialogue), and there is no
  cross-tile NAVM pathfinding.
- FO4/FO76/Starfield actors have no walk-clip source; their ragdolls wait on
  the `BhkSystemBinary` blob decoder.
- Full dialogue trees (the force-greet blocking branch is unmodeled), Story
  Manager events, perk entry-point composition (M43, M47.2).
- Starfield CDB materials: `.mat` texture slots merge from the streaming CDB
  index (`224a19372`, `18fce7e43`); #3398 stays open until its definition of
  done is checked.
- Texture streaming (M39). Reversed-Z is implemented but ships `Conventional`
  until validated on hardware (#3308).
- Oblivion's CHARAL ruleset is built but unwired: `Oblivion.esm` has no AVIF
  records, so a legacy actor-value resolver comes first
  ([`charal.md`](docs/engine/charal.md)).

**Active focus.**
- The [playable vertical slice](#playable-vertical-slice) is **complete**
  (P0–P5, closed 2026-10-01). Its follow-ons are listed there; none of them
  blocks the slice.
- RT lighting and material recovery: R0–R3 are complete. The rest is tracked
  in [`rt-lighting-material-recovery.md`](docs/engine/rt-lighting-material-recovery.md).
- WATAL: W0 and W1 are closed. W2's first defect is fixed: distant water is
  drawn per cell at each cell's XCLW height (#5243). The next step is the next
  W2/W3 shoreline/LOD defect from the captures
  ([`watal.md`](docs/engine/watal.md)).

### How to run a bench

**Repro-command CWD note:** bare `--bsa` / `--textures-bsa` / `--materials-ba2` names resolve against CWD, not the `--esm` folder. Run each bench with CWD set to that game's `Data/` directory. Run from elsewhere → archives silently fail → scene loads near-empty (Prospector: 36 entities / 3 meshes / spurious ~1792 FPS).

**CWD-immune alternative (#3346):** `--game <profile>` (`fnv` / `fo3` / `skyrim` / …) expands to *absolute* `--esm` / `--bsa` / `--textures-bsa` paths from `assets/debug_profiles.toml` via `expand_game_profile_args` (`byroredux/src/boot.rs`), so it works from any CWD and cannot mistype an archive name. Prefer it for ad-hoc runs and audits:
`cargo run --release -- --game fnv --cell GSProspectorSaloonInterior --bench-frames 300 --bench-hold`.
The bench-of-record table below deliberately keeps the bare-name + `cd` form for apples-to-apples continuity with the historical record — do not restate those numbers under a different invocation shape.

**Standing methodology.** Every refresh includes a same-machine control:
rebuild the outgoing record's commit in a worktree and bench it in the same
session, on the same harness. That control is what proved
PERF-REGRESSION-6c56e311 was code. It also produced R6a-regress-22, which a
pinned-pose control later showed was the bench camera's origin moving, not
code (#5128): a control compares like with like only when both builds start
from the same pose, so pin it with `--camera-pos` / `--camera-forward` across
any spawn-placement change. `fsr-bench-matrix.sh` now records each run's
`camera_pos` / `camera_forward`, and the report prints them per scene. Also
isolate `BYROREDUX_SETTINGS_PATH` so persisted menu settings cannot leak into
runs (#4947). A genuinely idle-machine run has never been done. FSR Quality
is the engine default; the `TAA (native)` column stays the historical
reference and is reachable with `--upscaler taa`.

Superseded records (R6a-stale-15 at `8a668eff` through `4c9a5b36`) are in
[`roadmap-history.md` §2](docs/archive/roadmap-history.md#2-superseded-bench-records);
raw rows for every matrix are in `docs/audits/BENCH_*.tsv`.

### Bench-of-record (LIVE) — stepped-camera refresh (2026-09-28, HEAD `a37fcba3c`)

Full matrix, 5 scenes × 5 configs × 3 runs of 300 frames, median with range,
1280×720 output, per-scene CWD set to that game's `Data/`. **75 runs, zero
rejections**, every scene-state fingerprint gate passing. Raw rows:
`docs/audits/BENCH_stepped-camera_a37fcba3c.tsv`.
Repro: `scripts/fsr-bench-matrix.sh 3 300`, harness `88c23887b`.

**Run conditions:** RTX 4070 Ti, 1380 / 12282 MiB VRAM in use and 1%
utilisation at start; desktop apps (browser, mail, chat, editor) resident but
idle. First record run with `BYROREDUX_SETTINGS_PATH` pointed at a fresh file,
so no persisted menu setting reached any run (#4947). Every config passes
`--upscaler` explicitly, so the persisted upscaler could not have leaked into
earlier records either; the other persisted settings could have.

| Scene | TAA (native) | FSR Quality | net recovery | FSR Performance |
|---|---:|---:|---:|---:|
| Cornell (37 ent, redistributable control) | **93.3 FPS / 10.72 ms** | 122.8 FPS / 8.15 ms | +2.57 ms (+24%) | 173.9 / 5.75 (+46%) |
| Prospector (3114 ent) | **80.1 FPS / 12.48 ms** | 97.2 FPS / 10.29 ms | +2.19 ms (+18%) | 128.0 / 7.82 (+37%) |
| Whiterun BanneredMare (5777 ent) | **83.8 FPS / 11.93 ms** | 134.2 FPS / 7.45 ms | +4.48 ms (+38%) | 190.9 / 5.24 (+56%) |
| MedTek Research 01 (39560 ent) | **19.8 FPS / 50.50 ms** | 37.1 FPS / 26.98 ms | +23.52 ms (+47%) | 51.5 / 19.42 (+62%) |
| FO4 Dugout Inn (11592 ent) | **41.2 FPS / 24.29 ms** | 68.4 FPS / 14.61 ms | +9.68 ms (+40%) | 89.5 / 11.18 (+54%) |

FSR Quality remains a net win on all five scenes (+18% to +47%), Performance
reaches +37% to +62%, and `native-aa` still loses 2% to 8%. MedTek
`fsr-balanced` is not usable from this capture: one run took 50.1 ms against
23.1 ms for another while its main pass stayed at ~20 ms in all three, i.e. a
stall outside the GPU in one run.

**Against the previous record (`4c9a5b36`) this is a large regression on FO4
content**: Dugout TAA 11.12 → 24.29 ms (FPS −54%), MedTek TAA 37.64 → 50.50 ms
(−26%), Cornell TAA 9.20 → 10.72 ms (−14%). Prospector TAA is 5% faster and
Whiterun Quality 24% faster. **Content was NOT unchanged**, despite what this
paragraph originally claimed (FNV-2026-09-29-D6-01): Prospector lost 35
entities and 35 TLAS instances (3149 → 3114, 928 → 893) in this refresh's
window — attributed by the 2026-10-04 same-machine probe pair
(renderer-stepped, orbit): `5570c221c^` reproduces 3149/928 and `5570c221c`
reproduces 3114/904/893 bit-for-bit, so the dismemberment-cap
reclassification (`is_dismemberment_cap` from body-part data; the same commit
the 2026-09-29 runtime audit measured at −92 on AtomicWrangler) withheld ~35
Prospector instances. The Prospector TAA "5% faster" is therefore measured
on −35-instance content, not like-for-like. Whiterun (+12) and MedTek (+23)
entity bumps landed between `4c9a5b36` and the *uncontrolled* middle record
`cb44d99f6`, not in this window; Dugout's −11 entities/TLAS instances in the
window remain unattributed (small; same probe procedure applies). Since this
record Prospector moved again, attributed 2026-10-04 with two further probe
pairs: +195 entities with draws/TLAS/lights stable (non-rendering accounting;
the `a070baaad` player-body window — the same commit the #5125 fo4 regen
measured at +187 — unprobed on FNV), then `f87490826` (Initially Disabled /
Starts Dead withholding, #4813/#4814/#4820) took it to today's 2359
entities / 814 draws / 803 TLAS, matching HEAD bit-for-bit
(`f87490826^` reads 3309/904/893). Compare wall and fence only:
`gpu_main` changed meaning in this range (#4808 starts it at COMPUTE, see
R6a-stale-22). The fence wait carries the regression (Dugout TAA 4.13 →
18.95 ms, MedTek TAA 16.12 → 35.48 ms), and draw batching roughly halved its
merge rate at flat draw counts (Dugout `1935/326b/12c` → `2367/659b/43c`,
Prospector `928/54b/6c` → `904/257b/68c`, MedTek 29 → 82 GPU calls).

The 90-run same-session control behind this record (`4c9a5b36` vs
`99933f87b` vs `a37fcba3c`), superseded by R6a-regress-22's pinned-pose
reading, is in [`roadmap-history.md` §2](docs/archive/roadmap-history.md#2-superseded-bench-records).

### Compatibility matrix

NIF rates come from
`cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`,
which sweeps every archive in `Game::mesh_archives()` plus the present-only
`optional_mesh_archives()` tier (#3369 / #3712). Clean = no `NiUnknown`
placeholder and no truncation. Recoverable = the file parses end-to-end.
Each row carries its own measurement date. This table is the single home for
parse rates; other sections link here instead of restating them. Frame-time
figures for the scenes named below are in the bench-of-record, not here.

| Game              | Archive       | NIF parse rate (clean / recoverable) | Verified content |
|-------------------|---------------|--------------------------------------|------------------|
| Oblivion          | BSA v103      | **100%** (9 612 / 9 612) · recover 100% — includes the eight vanilla DLC archives (2026-09-07, #3925) | Interior (Anvil Heinrich Oaken Halls). Exterior Tamriel `(0,0)` r1 passes the image-health and environment-value gates ([`exterior-readiness-plan.md`](docs/engine/exterior-readiness-plan.md)). |
| Fallout 3         | BSA v104      | 100% (17 172 across 6 archives, base + 5 DLC; 2026-08-28) | Interior (Megaton, 929 REFRs). Exterior wired. |
| Fallout New Vegas | BSA v104      | 100% (20 746 across all 11 mesh-bearing archives; 2026-08-28) | Interior (Prospector Saloon, a bench-of-record scene). Exterior 7×7; Lake Mead water traversal (W1). |
| Skyrim SE         | BSA v105 LZ4  | 100% (**33 468** across 8 archives — the two base + `_ResourcePack` + five CC/AE + `Animations`; #3712 added the 44-NIF Animations archive, re-measured 2026-09-29) | Interior (WhiterunBanneredMare, a bench-of-record scene, with equipped named NPCs). Exterior Tamriel with `.btr`/`.bto` LOD. MQ101 plays end-to-end. |
| Skyrim LE         | BSA v104 zlib | 100% (**22 466 / 22 466**, `Skyrim - Meshes.bsa`; gated since fb8173fe0) | Parse only. |
| Fallout 4         | BA2 v1/v7/v8  | **100.00%** (**235 082 / 235 082**) · recover 100% — all 8 mesh-bearing archives, including 9 073 `.bto`/`.btr` LOD meshes (2026-08-29, #3466) | Interior (MedTekResearch01 and Dugout Inn, bench-of-record scenes). CSG precombines (M49). Commonwealth exterior streaming. |
| Fallout 76        | BA2 v1        | **100%** (**102 968 / 102 968**; re-measured 2026-09-29 over 4 of the 20 mesh-bearing archives, incl. both `GeneratedMeshes` — #3461 (2026-09-02) closed the 3 056-NIF truncation tail the 2026-08-29 sweep found, so the old `GeneratedMeshes02` 0.00% / `GeneratedMeshes01` 95.03% split no longer exists) | Parse only. |
| Starfield         | BA2 v2/v3 LZ4 | **100.00%** (**120 543 / 120 543**) · recover 100% — all 13 mesh-bearing archives (2026-09-24, #4440) | Walkable Cydonia interior. `BSWeakReferenceNode` still leaves an undecoded remainder in `starfield_tail`; the 100% is recovery, not decode. |

---

## Active Roadmap

Priority: **shortest path to a playable cell**, not shortest path to a
shinier frame. The renderer is mature and the content pipeline parses
recoverably across every target; the bottlenecks are *consumers* — systems
that make parsed data do something on screen, at the speakers, or in play.

**Two axes.** Milestones (`M…`) ship user-visible capability.
Risk-reducers (`R…`) are structural fixes flagged in the 2026-04-22
architectural review — not features, but prevention work that gates a
specific milestone.

### Playable vertical slice

**Closed 2026-10-01.** All six phases (P0 input → P5 persistence and soak)
were closed by live gates on one console-free Skyrim route. The phase-by-phase
evidence is in
[`docs/engine/playable-vertical-slice.md`](docs/engine/playable-vertical-slice.md),
and the one-liner is under [Completed Milestones](#completed-milestones).
Open follow-ons, none of them a slice blocker:
- #5155: the soak intermittently restores `grounded=false` after its 10th
  F5 → door → F9 cycle.
- Skyrim `p2-melee-core.sh` passes end to end since #5161 (`5ae7f8ad4`) and
  #5246 contained the ragdoll solver explosion before the broad phase;
  #5352 (HIGH) is a recovery-substep gap left in that clamp.
- P1: Skyrim's W1 water leg is skipped (land-side KCC wedge). P4: the
  force-greet blocking branch is unmodeled. Gamepad sources and a second
  game route are not started.

### Milestone tiers

Tiers 1 (playable exterior), 2 (actors visible and animated) and 4 (save/load)
are closed. Their milestones are listed under
[Completed Milestones](#completed-milestones), and their full rows are in
[`roadmap-history.md` §5](docs/archive/roadmap-history.md#5-tier-17-milestone-tables-with-full-rows).
The rows below are the open ones: a shipped summary, the open scope, and the
design doc.

#### Tier 3 — Scripting runtime

| #     | Milestone | State | Depends on |
|-------|-----------|-------|------------|
| M47.2 | Full scripting runtime | **Shipped slices:** a Champollion-port `.pex` decompiler (99.996% of the corpus) lowered through the recognizer chain, with VMAD attach from `--scripts-bsa`; quest-advance and trigger volumes (Session 51); QUST stage fragments (Session 55); quest aliases and object-targeting effects (Session 59); the MQ101 SCEN/PACK scene runtime backed by `crates/hkx` (Session 62); the quest-area completion pass (2026-08-07); and an SKSE-family script-extender provider slice (~23.9k LOC, tested in-crate but never audited against real mods). **Open:** recognizer catalog breadth (OnEquip/OnHit emit sites), ESM-native 136-event dispatch, perk entry-point composition, general NPC playback from HKX, and the Story Manager / LCTN / created-object / reference-collection alias operations. Design: [`m47-2-design.md`](docs/engine/m47-2-design.md), [`sdk-v0.1-development-plan.md`](docs/engine/sdk-v0.1-development-plan.md). | R5, M30.2, M43 |
| M47.3 | ObScript quest VM (Oblivion) | **Phase 1 shipped 2026-09-18:** an SCDA bytecode interpreter runs every running quest's `Begin GameMode` block on the vanilla 5 s cadence and lowers onto `QuestStageState`/`Globals`. Its opcode table was recovered by aligning source against bytecode over vanilla `Oblivion.esm` (2 349 of 2 393 scripts decode clean), and it was verified live (MS23 self-advances to stage 90). **Phase 2:** non-GameMode blocks (OnActivate/OnTrigger on object scripts), Message/MessageBox UI, per-quest `fquestdelaytime`, actor-state functions (GetDeadCount/GetItemCount/GetDistance), and the remaining ~60-command corpus. | M47.2, M42.10 |

#### Tier 5 — Renderer polish (quality, not capability)

| #     | Milestone | State | Depends on |
|-------|-----------|-------|------------|
| M35   | Terrain LOD | **Shipped:** Skyrim/FO4 `.bto` objects and `.btr` terrain on a per-game `Ultra.ini` band ladder (`d96110eb`), with `_n`/`_msn` normal maps. Oblivion/FO3/FNV `_far.nif` object LOD (#1726). A derived far plane (`9e96a9f9`). The live `terrain.seams` gate in `m-exteriors.sh` (#2371). **Open:** VWD culling is blocked because baked `.bto` quads carry no per-object ids (#3307). `.btr` diffuse/normal sampling mode (#4912). Skyrim `.btt` distant trees are never drawn (#4913). The reversed-Z flip needs hardware validation (#3308). LAND value-plausibility guards ([`exterior-readiness-plan.md`](docs/engine/exterior-readiness-plan.md) Tranche C item 5). | M32 |
| M39   | Texture streaming | Mip-chain-aware loading: upload low mips immediately, stream high mips on demand. Memory budget with LRU eviction. | — |
| M29.3 | Pre-skinned raster path | Make `triangle.vert` read pre-skinned vertices from the per-entity `SkinSlot` output instead of doing inline bone blending, and re-add the `VERTEX_BUFFER` usage dropped in #681. This saves ~50 ALU ops per skinned vertex, but makes raster depend on the compute pass. The recorded precondition is that the compute + BLAS-refit chain has proved stable on visible animated content. | `1ae235b`, #681 |

#### Tier 6 — Engine infrastructure

| #     | Milestone | State | Depends on |
|-------|-----------|-------|------------|
| M24.2 | ESM Phase 2 — QUST stages + PERK entries | **QUST done:** [`parse_qust`](crates/plugin/src/esm/records/misc/quest.rs) covers startup/shutdown stages, conditional log entries, objectives, and version-aware targets with load-order remapping, and the scripting runtime consumes all of it. **PERK:** all three PRKE entry bodies decode. **Open:** per-`function_type` EPFD decoding and per-entry CTDA. DIAL conversation trees are M43 scope. | R2 |

#### Tier 7 — Deep gameplay systems

| #   | Milestone | State | Depends on |
|-----|-----------|-------|------------|
| M42 | AI packages | **Shipped:** PACK decode (PKDT/PSDT/PLDT/PTDT, #446). CTDA-gated package selection through the M47.1 evaluator, failing open on out-of-catalog functions (M42.2). Re-evaluation at game-minute boundaries and on Papyrus `EvaluatePackageRequest` (M42.9, #2652). Seven procedures — Sandbox seat with per-marker reservations, Wander, Travel, Follow, Escort, Guard, Patrol — run by default behind a `BYRO_NO_AI_LOCOMOTION=1` kill-switch, with authored per-game walk clips at stride-matched speed and KCC-backed steps (M42.10/M42.11, 2026-09-18). Plus ambient hostility (#4414), disengagement (#4816) and re-seating after a save load (#4815). **Open:** the 10 non-locomotion procedures (Find/Eat/Sleep/Accompany/UseItemAt/Ambush/FleeNotCombat/CastMagic/Dialogue/UseWeapon), each blocked on a missing subsystem. Also: `PTD2`, calendar-aware scheduling, sit-enter beyond FNV/FO3, legacy sleep/lean marker disambiguation, and FO4+ walk sources. NearReference target resolution was deprioritized (~12% of targets resolve). Trace: [`npc-spawn-ai-packages.md`](docs/engine/npc-spawn-ai-packages.md). | M28.5, M41 |
| M43 | Quests & dialogue | **Shipped:** the quest core — version-aware stages, logs, objectives and targets; full lifecycle transitions; Papyrus quest effects; save-persistent progress; loaded-reference alias fill with conditions and reservations; faction/inventory injections. `quest.*` observability commands, with [`m43-quest-runtime.sh`](docs/smoke-tests/m43-quest-runtime.sh) driving the production path. **Open:** Story Manager event payloads and search, reference collections, true LCTN/unloaded-world resolution, created-object spawning, broader condition/event coverage, and the dialogue tree with its UI. These are subsystem boundaries, not missing QUST bytes. | M24.2, M41, M47.1 |
| M46 | Full plugin loading | Discover, sort, merge, and resolve conflicts across the full load order. Builds on M46.0 (CLI wiring), the `plugin/resolver.rs` DAG, and the parallel per-plugin walk (#3813). | M24.2, M46.0 |
| M48 | UI integration | **Shipped:** the Scaleform host bridge — Skyrim/SkyUI 142-method catalog, the FO4 `BGSCodeObj` 269-method catalog with a generated AVM2 adapter, and an archive-backed navigator (`--menu … --menu-archive …`). The MenuXml crate (FOLD evaluator, layout, CPU raster) drives the Oblivion HUD (M48.4) and FO3/FNV through a game-agnostic profile (M48.5). `--hud` runs the vanilla Skyrim (M48.6) and FO4 (M48.7) `hudmenu.swf` transparently over the world, with a smoke per route. Starfield's `hudmenu.swf` now parses through a PlaceObject3 dialect shim (#4470); its profile and catalog are not built. **Open:** method behaviour and `_global.gfx` stubs, font fidelity, menu-stack policy, and Papyrus/ECS ↔ UI callbacks. Vanilla Skyrim/FO4 HUD meters stay empty: the game feeds them by GFx object-path invocation, which Ruffle cannot reach, so they need AVM1 injection or SkyUI-class menus. Design: [`ui.md`](docs/engine/ui.md). | R4, M48.4, M48.6 |

#### Tier 8 — Visual fidelity stretch (post-Tier-4 horizon)

Make existing content look as good as it can: beauty only, with
pure-performance work in Tier 11. Each entry builds on the RT investment
rather than adding a parallel pipeline — volumetric and hair shadows are RT,
SSS can be RT-traced, and decals take part in GI. Sequencing is impact-first:
M55 (volumetrics) and M59 (decals and material layering) change every
existing cell. M56/M57 (SSS, hair) render onto the NPCs M41 now spawns. M51
(PT reference) and M-LIGHT top off the pipeline, and M54 waits until there is
enough scene data to train on. M55's fog slice and M58's bloom slice have
shipped; the rest of the tier has no active work.

| #        | Milestone                          | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | Depends on              |
|----------|------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------|
| M55      | Volumetric lighting                | God rays / light shafts via single-scattering volumetric integration in a frustum-aligned 3D froxel texture, RT-shadowed (no shadow-map cascade hack — we already have RT visibility). Exponential height fog with per-cell density driven by REGN region records and weather state. The cinematic moment Bethesda interiors most lack — sunlight through Megaton's church windows, dust motes in Doc Mitchell's hallway, fog rolling off WastelandNV grass at dusk. Reference: Hillaire (Frostbite, SIGGRAPH 2015) Frostbite volumetrics. **Fog slice shipped 2026-07-26→08-01 (Session 62):** procedural froxel-grid fog with temporal reprojection, clustered local fog volumes, authored CELL/WTHR extinction/chromaticity/coverage. **Emissive media became a transported combustion solver (Session 69, 2026-08-17→18, `2325c1de`..`348f4cd0`):** fire and smoke are one soot material advected around solid geometry, with first-order fuel/oxidizer reaction chemistry, fuel-rich soot yield and hot-lean oxidation, self-shadowing, spectral scattering, transported overpressure and restored vorticity. Rates, yields and temperatures are canonical constants in `crates/core/src/combustion.rs`, mirrored into GLSL through the generated shader-constant path; `FogProfile` acts as a solver emitter preset with no game or authoring-provenance branch. Surface illumination is reduced from the transported field (`append_combustion_surface_lights`), replacing the deleted analytic-primitive `render/fire_lights.rs` — see Known Issues for the visual check this still owes. Spec: [docs/engine/procedural-volumetric-fog.md](docs/engine/procedural-volumetric-fog.md). Interior godrays and sky apertures landed as a vertical slice (`0572bfd5a`, [`interior-godrays-status.md`](docs/engine/interior-godrays-status.md)); REGN-driven per-cell height fog and exterior single-scattering light shafts remain open, as does the follow-up boundary that doc enumerates (boundary-normal slip/pressure response, aerial-perspective LUT, majorant-grid path-traced media). | M34, M44 (REGN parsing) |
| M59      | Material & decal layering          | Decal projection (blood splatters, bullet holes, footprints in dust, water puddles, scorch marks), micro-detail normal maps (concrete, fabric, metal grain, leather pores), anisotropic specular (brushed metal, hair, satin, vinyl). **POM slice shipped 2026-07-29:** authored-tangent primary POM, adaptive layer budgets, and height-displaced material UVs at reflection / GI / refraction / water / material-aware shadow hits; BVH silhouettes remain undisplaced. RT decals participate in GI — Bethesda decals never have. Material slot extensions to `GpuMaterial` only; the R1 promise holds (no DrawCommand or shader-lockstep growth). | R1                      |
| M61      | Wet-surface & storm-accumulation system | A "wet" state on actors/clothing/static geometry driven by water contact (rain volumes, submersion, splashes, footprints exiting a WATAL water plane) that darkens albedo and boosts specular/fresnel toward a thin-film reflective/refractive look, with a drying-over-time falloff. **Extension (2026-08-03):** procedural snow buildup during storms — an accumulation state (ground, roofs, static geometry) that grows while the active WTHR is snow-classified and settles/melts otherwise, sharing the same per-surface weather-contact state machine as wet (rain → wet, snow → accumulating coverage) rather than a parallel system. Triggered off WTHR's `classification` byte (Clear/Cloudy/Rain/Snow, DATA byte 11 — parsed since #538/#543 but with no downstream consumer today). **Extension (2026-08-03):** Fallout radstorms accumulate a third, darker grime/ash coating by the same mechanism. Open research question before scoping: `classification` is only a 4-value Clear/Cloudy/Rain/Snow bitmask (`WTHR_PLEASANT/CLOUDY/RAINY/SNOW`) with no dedicated radstorm bit, so the real per-record signal that distinguishes a radstorm WTHR from ordinary rain (EDID convention? an associated ash/particulate FormID? a script-side trigger outside WTHR entirely?) hasn't been identified yet — needs the same kind of source verification `resolve_water_material`'s EDID-substring `WaterKind` heuristic went through, not a guess. No scoping done yet — surfaced while wiring #2240 (WATR wave params); needs its own design doc before work starts, sized like M59's material-slot approach (`GpuMaterial` extension, no `DrawCommand`/shader-lockstep growth) rather than a new render pass. Depends on WATAL's water-plane contact geometry existing to detect "touched by water" in the first place. | M59, WATAL              |
| M58      | Reference-quality post-process     | Kawase-blur bloom (5-pass dual filter, ~2 ms total), scatter-as-gather DOF for cinematic mode, per-object motion blur reusing existing motion vectors, color grading via 3D LUT (per-cell-type mood — interior warm, exterior cool, irradiated green), AgX or Tony McMapface tone mapping selectable alongside ACES, optional vignette / film grain. Single compute dispatch chain layered onto the existing composite pass; no extra render-pass churn. **Bloom slice shipped Session 33 (`33f48b5`):** separable box-filter downsample/upsample pyramid (`bloom_downsample.comp` / `bloom_upsample.comp`) applied to scene HDR after composite (`bloom_apply.comp`); the Kawase/Jimenez dual-filter upgrade and the DOF, motion-blur, LUT-grading and tone-map-selection scope all remain open. | —                       |
| M56      | Subsurface scattering              | Burley normalized SSS (preferred) or screen-space SSS for skin / wax / fruit / soft organic materials. M41's NPCs become visibly human only once skin gets SSS — flat Lambert reads as plastic. Eyes get cornea refraction + caustic (existing M22 caustic compute path is reused, not duplicated). Optional RT-traced SSS variant for closeup actors in cinematic mode.                                                                                                                                       | M41, R1                 |
| M57      | Hair / fur shading                 | Marschner three-lobe BRDF (R / TT / TRT) for hair; Disney's hair shading model as the simpler default. RT hair shadows with stochastic transparency. Bethesda-vintage hair plate meshes look like clay under standard PBR; correct hair shading is the difference between "T-pose mannequin" and "T-pose person." Pairs naturally with M56 — face closeups need both.                                                                                                                                          | M41                     |
| M-LIGHT  | Reference-quality lighting         | Soft shadow penumbras filtered in screen space using RT visibility samples (no PCF, no cascade tricks), contact shadows on dielectrics (close-range RT for groundedness), IBL from per-cell HDR sky probe captured once at cell load, multi-bounce GI (≥ 2 bounces in PT mode, currently 1 in raster mode). Closes the gap between "lit correctly" (today) and "lit as well as the data allows."                                                                                                              | M51                     |
| M51      | Path tracing reference mode        | Full PT (no rasterized fallback), ReSTIR-PT spatiotemporal reservoirs, SHARC radiance cache for diffuse, optional NRC neural radiance cache. Reference mode for screenshots / cinematics; demonstrates "RT-first" wasn't a positioning claim. References: Bitterli et al. (ReSTIR PT, 2022), Pharr et al. (SHARC, 2024). Reuses existing reservoir + denoiser plumbing from M31.5 / M37.                                                                                                                       | M37, M37.3              |
| M54      | Neural denoiser                    | Small NN (~1–2 MB weights) replaces SVGF spatial filter for indirect lighting once enough scene data exists to train. Targets visual parity + 30% runtime. References: NRD-NN, Intel Open Image Denoise GPU. Implementable as a Vulkan compute pass; no proprietary SDK dependency. At PT scales (M51) it stops being a perf optimisation and becomes a quality multiplier.                                                                                                                                  | M37, M51                |

#### Tier 9 — Better-than-Bethesda capability stretch (post-Tier-4 horizon)

Plays to the ECS-native architecture and clean-room rebuild. Each
entry is something Bethesda demonstrably cannot ship on top of the
Papyrus stack-VM + non-deterministic save system. **No active work**
until M40 (streaming) and M45 (save/load) prove the underlying
state model holds up.

| #     | Milestone                     | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Depends on                              |
|-------|-------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------|
| M50   | In-engine world editor        | ECS-native Creation Kit replacement: edit cells, place REFRs, paint terrain, edit lighting, with hot-reload to a running engine via the `byro-dbg` protocol. Linux-native, no Wine; undo/redo is free off the ECS snapshot machinery; saves diff against the parsed ESM into a content-addressed plugin. The largest single user-visible win the project can ship — and the existing debug-protocol crate already does ~60% of the wire shape this needs.        | M45, debug protocol expansion           |
| M60   | P2P co-op (≤ 4 players)       | Deterministic ECS state replication, host authority with host migration. Was always a community-mod hack on every Bethesda title — practical here because we don't have Papyrus stack-VM semantics fighting determinism. Engine ships P2P only; **no central server, no matchmaking, no telemetry.** R5's quest prototype must include a determinism analysis before this can ship.                                                                              | M45, R5                                 |
| M62   | LLM dialogue plugin (opt-in)  | Optional plugin wiring DIAL / INFO + custom Papyrus events to a local LLM (Llama 3.x via candle / mistral.rs). Off by default, opt-in per quest or per NPC; lives entirely in plugin space. Demonstrates what "ECS event hooks" enables that stack-VM Papyrus cannot. No network calls; LLM weights ship with the mod.                                                                                                                                            | M47.0, M43                              |
| M63   | OpenXR / VR                   | Full VR via openxrs. RT-first renderer is genuinely useful here — VR is the genre most starved of well-running RT content, and our forward-pass cost is already low. Stereo rendering through the existing pipeline; controller input through the existing input layer; M50 expanded for VR-aware authoring.                                                                                                                                                     | M27, M50                                |
| M64   | Procedural exterior cells     | Generate exterior cells from heightmap + biome rules + noise. Effectively unlimited worldspace; complements rather than replaces vanilla Bethesda exteriors. Starfield's procedural-planet model is the obvious comparison; ours can do better because cells are first-class ECS state, not save-file blobs.                                                                                                                                                       | M40, M50                                |

#### Tier 10 — Ecosystem unlock (post-Tier-4 horizon)

Realizes the architectural promise of content-addressed Form IDs and
clean-room data-only legacy compat. **No active work** until the
engine ships something playable for the existing community to mod.

| #     | Milestone                     | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Depends on              |
|-------|-------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------|
| M70   | Cross-platform builds         | Windows native (vanilla Vulkan, no portability layer) and macOS via MoltenVK. The project is Linux-first by primary maintainer setup, **not Linux-only by design.** CI matrix expands to all three platforms; Steam Deck falls out of the Linux build for free.                                                                                                                                                                                                   | —                       |
| M72   | Decentralized mod hosting     | IPFS-style content-addressed mod distribution: mod-id = content hash, dependency graph auto-resolves through the existing `plugin/resolver.rs` DAG. Realizes the full payoff of "no LOOT, no slot limits" — mods are addressable globally, no central marketplace, no Bethesda.net-style platform tax.                                                                                                                                                            | M46, M50                |
| M80   | glTF / USD export             | Round-trip Bethesda assets to industry-standard formats. Open NIF / NifSkope replacement: load any vanilla mesh, export to glTF + materials, edit in Blender / Houdini, re-import as a content-addressed plugin. Massive creative-pipeline unlock for modders who don't want to fight 3ds Max 2010 and a 15-year-old NIF plugin.                                                                                                                                  | NIF parser stable       |
| M81   | Visual scripting (BT-style)   | Behavior-tree node graph layered on M47.0 event hooks, for modders who don't write code. Same surface as Papyrus would expose, but with no language to learn. Complements rather than replaces M47.0 / M47.2.                                                                                                                                                                                                                                                       | M47.0, M50              |
| M82   | Asset preprocessing pipeline  | One-shot bake: BSA / BA2 → ByroRedux native asset format with optimal layouts (texture-streaming-aware mip ordering, cluster-aware mesh layout for M53). Ships once at install / mod-publish; runtime loads are 10× faster. Replaces the current "open the BSA, decode on demand" hot path with a memory-mappable bundle.                                                                                                                                          | M39, M53                |

#### Tier 11 — Performance ceiling (post-Tier-4 horizon)

Pure ceiling raisers: no new visuals or gameplay surface. They start only
when a real-content bench names them as the bottleneck. The live
bench-of-record shows the FO4 scenes fence-bound from their post-`b9e961eeb`
spawn views (see R6a-regress-22), and
`build_render_data` still walks every entity per frame, scaling linearly with
cell complexity. M52 is the lever for that regime.

| #     | Milestone                          | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                              | Depends on |
|-------|------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------|
| M52   | GPU-driven rendering               | Mesh shaders + task shaders, GPU frustum + cluster cone culling, indirect-from-GPU draw command generation. Eliminates CPU draw-submission overhead at 7 000+ entities (FO4 MedTek bench's actual ceiling). Falls back to current path on hardware without `VK_EXT_mesh_shader`. Closes the loop on R7 / M27: parallel ECS dispatch on the CPU side + GPU-driven submission on the GPU side.                                                       | M27        |
| M53   | Virtual geometry (Nanite-class LOD) | Cluster mesh format with deterministic simplification chain, GPU cluster selection per pixel. Makes M35 (.btr terrain LOD) obsolete in the good direction: load full-resolution geometry, GPU picks the right level. Same data structure works for static and skinned meshes (with care around bone-influenced clusters).                                                                                                                       | M52        |

### Parking lot (nice-to-have, no active work)

| #       | Notes                                                                                                                                                                            |
|---------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| M37.6   | DLSS2. Proprietary, 4070 Ti target. Post-M37 TAA is already solid; DLSS is a later polish pass if it ever becomes relevant.                                                     |
| M25     | Vulkan Compute — partially realised (clustered lighting / SSAO / SVGF temporal are compute-backed). Remaining work folds into M29 (skinning) and M37 (spatial filter).          |
| Full cosave save/load | M45 v1 ships a simple snapshot. Byte-compatible cosave format (load original-engine save into Redux, or vice versa) is speculative and not a priority.                         |
| Morrowind (TES3)      | NIF v3.x / v4.x is fundamentally different from the v10+ era ByroRedux supports — separate parser, separate ESM dialect, no BSA. Gamebryo 2.3 source we reference predates Morrowind's release; OpenMW is the canonical clean-room re-implementation. **Out of scope** unless explicit demand surfaces — supporting it would double the parser surface for one extra game. |

### What we are NOT doing (anti-scope)

Documented to keep "wouldn't it be cool if" suggestions from
silently growing the cone:

- **Per-pixel parity with Bethesda's interior look.** Sessions 25–28
  showed the trap: chasing "chrome cushion" / "yellow fog" took 3
  sessions to find a missing texture and a Frostbite-curve adoption.
  ByroRedux is RT-first; matching a 2008–2015 forward-renderer's
  specific look is *not* the goal. Render correctly, ship.
- **Original-engine save-format compatibility.** M45 ships a
  structured ECS snapshot. Loading vanilla saves is a 6-month
  reverse-engineering project for an audience of approximately zero.
- **Mod-load-order tooling (LOOT-equivalent).** Content-addressed
  Form IDs are explicitly the architectural alternative. We do not
  ship a sorter.
- **Console releases / non-Linux primary support.** Linux-first.
  Windows + macOS are downstream if they happen.
- **Hosted online services.** No telemetry, no updater, no crash
  reporter posting upstream, no central server, no skin /
  monetization surface, no Bethesda.net-style mod marketplace.
  P2P co-op (M60, Tier 9) is **in scope** — engine ships the
  replication layer, never a server. Decentralized mod hosting
  (M72, Tier 10) is in scope — content-addressed, no central
  registry.
- **Cloning Papyrus VM semantics.** R5 (closed 2026-05-16) chose
  ECS-native: compiled `.pex` is decompiled and lowered onto ECS
  effects, and M47.3's SCDA interpreter lowers onto the same quest
  state. We are not implementing OpcodeFetch / OpcodeDispatch /
  StackFrame / StackUnwind in their original shapes. Better, not
  same.

---

## Architecture Decisions

### The keep list — what *not* to change despite temptation

These were re-examined in the 2026-04-22 architectural review and
deliberately kept. Document them here so they survive hype-driven
rewrite pressure.

| Decision                                     | Choice                                                         | Why kept                                                                                                                                                                                                              |
|----------------------------------------------|----------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| ECS storage model                            | Per-component `Component::Storage` (SparseSet or Packed)       | No archetype store. Cheap to maintain, easy to reason about, fine at Bethesda entity counts (~1 200 dense interior, a few thousand streamed exterior). Archetypes would be seductive and wrong at these query shapes. |
| Lock model                                   | Per-storage `RwLock`, TypeId-sorted acquisition, lock_tracker   | Query methods take `&self`; multi-component queries acquire in `TypeId` order; `#313` cross-thread ABBA graph + debug tracker catches deadlock pre-parallel. Mature. Keep.                                          |
| NIF block dispatch                           | `Box<dyn NiObject>` over ~250 dispatch arms (live count in source) | Enum dispatch would cost more in maintenance than it gains in perf at these branch counts. Keep.                                                                                                                       |
| NIF versioning                               | Raw `bsver()` checks inline, not trait dispatch via `NifVariant` | Per `#437` / `#160` / `#323`: byte-level versioning is genuinely per-game-per-version. A trait would lie. The semantic `NifVariant` flags are used where useful; raw bsver is used where versioning is byte-level. Keep. |
| Plugin identity                              | Content-addressed Form IDs                                      | Eliminates load-order dependency + slot limits. Best single architectural call in the project. Keep.                                                                                                                  |
| Coordinate system                            | Z-up→Y-up with CW angle negation                                | Documented in `docs/engine/coordinate-system.md`. Keep.                                                                                                                                                                |
| Rendering                                    | RT-first, rasterized fallback                                   | Scoped to RTX 4070 Ti target. Correct for this hardware. Keep.                                                                                                                                                         |
| Legacy compat                                | Parse data, don't emulate engine                                | Better results, clean room, no copyright issues. Keep.                                                                                                                                                                 |
| Scripting                                    | ECS-native (no VM)                                              | Eliminates Papyrus queue latency, stack serialization, orphaned stacks. R5's one-quest prototype (closed 2026-05-16) confirmed the call; M47.2 builds on it.                             |

### Risk-reducers (R1–R7, 2026-04-22)

Structural fixes to keep known growth patterns from calcifying. Only R2 is
still open; the others are listed under Completed Milestones.

- **R2** — ESM typed sub-record decoder. Phases A and B closed 2026-05-24
  (cursor primitive and the 169-site migration). Phase C, the typed
  `read_sub::<T>` schema layer, is adopted only in `records/misc/magic.rs` so
  far. It does not block M24.2.
- **R6a** — the bench-of-record discipline. It is a standing rule rather than
  a milestone: a record more than 30 commits old is stale. The live record and
  its open regression are under Status and Known Issues.

### Growth discipline

The project's single biggest risk is **scope growth without compression**.
Tier ordering gives top-level backpressure; apply it inside crates too. If a
single file crosses 3 500 lines, a struct crosses 50 fields, or a context
struct crosses 60 fields, treat it as a signal to investigate before adding,
not a stat to report.

**Tripwire today** (measured 2026-09-29; the counts include inline test
modules). Over the line: `byroredux/src/env_translate.rs` 4 594,
`crates/renderer/src/vulkan/volumetrics.rs` 4 396,
`byroredux/src/material_translate.rs` 4 141,
`crates/renderer/src/vulkan/context/draw.rs` 4 107,
`crates/scripting/src/translate/effects.rs` 3 899,
`byroredux/src/inventory.rs` 3 786 and `crates/physics/src/world.rs` 3 785.
Before splitting one, grep for `include_str!` source-shape tests that read
it.

### Pacing discipline

Audit cadence is load-bearing: without backpressure, audits become the work
product instead of the work.

- **Which audits run** is decided by the audit skills, not by session habit.
  Runs are delta-scoped: `/audit-suite --changed` routes the diff through
  `_audit-owners.md`, and each skill scopes to the dimensions whose paths
  changed since its last report.
- **LOW-severity findings** are bundled per audit into one commit per theme,
  not one commit per finding.
- **Stale-bench discipline**: any change that touches a numbered claim here
  must refresh the claim, remove it, or move it to
  [`roadmap-history.md`](docs/archive/roadmap-history.md). No silent drift.

---

## Completed Milestones

One-liners grouped by area. Full rows for milestones closed before
2026-09-29 are in
[`roadmap-history.md` §5](docs/archive/roadmap-history.md#5-tier-17-milestone-tables-with-full-rows);
per-milestone scope is in `git log`; session context is in
[HISTORY.md](HISTORY.md).

**Graphics foundation**
M1 Vulkan init chain · M2 GPU geometry · M4 ECS-driven rendering ·
M7 depth buffer · M8 texturing · M13 directional lighting.

**ECS, plugins, coordinates**
M3 ECS foundation (World, Component, Storage, Query, Scheduler,
Resources, string interning) · M5 plugin system (stable Form IDs,
DAG resolver) · M6 legacy bridge (per-game parser stubs) ·
M17 coordinate system fix (CW rotation, SVD degenerate repair) ·
R7 scheduler access declarations (`Access` builder, `sys.accesses`) ·
M27 parallel system dispatch (2026-05-23) ·
M46.0 multi-plugin CLI (repeatable `--master`, #561).

**NIF parser overhaul (N23 series)**
N23.1 trait hierarchy · N23.2 shader completeness ·
N23.3 Oblivion block types · N23.4 FO3/FNV validation ·
N23.5 skinning · N23.6 Havok collision skip + compressed mesh ·
N23.7 Fallout 4 · N23.8 particles · N23.9 FO76/Starfield ·
N23.10 test infrastructure · R3 per-block-type parse histogram
(`nif_stats --tsv`, checked-in seven-game baselines). The block dispatcher's
live arm count is in `crates/nif/src/blocks/mod.rs` — not frozen here.

**Asset pipeline**
M9 NIF parser · M10 NIF→ECS import · M11 BSA reader ·
M14 DDS texture loading · M16 ESM parser & cell loading ·
M18 Skyrim SE NIF · M19 full cell loading · M26 BA2 archive
support (v1/v2/v3/v7/v8, zlib + LZ4) · NIFAL (NIF Abstraction Layer,
2026-05-28) — the canonical parse-time translation boundary ·
M49 FO4 precombined geometry (CSG reader, 2026-06-02, #1351).

**ESM records (M24 Phase 1)**
Items (WEAP/ARMO/AMMO/MISC/KEYM/ALCH/INGR/BOOK/NOTE), containers,
leveled lists (LVLI/LVLN), NPC_, RACE, CLAS, FACT, GLOB, GMST.
SCPT pre-Papyrus bytecode records parsed (#443). CREA + LVLC dispatched
(#442/#448). PACK / QUST / DIAL / MESG / PERK / SPEL / MGEF fully parsed
(#446/#447; decode detail under M24.2 / M43).

**Animation and actors**
M21 animation playback (.kf, linear/Hermite/TBC, 8 controller types,
blending stack) · KFM binary parser (Gamebryo 1.2.0.0 → 2.2.0.0) ·
BSAnimNote / BSAnimNotes with IK hints · skeletal skinning end-to-end
(#178) · M29 skinning chain verification · M29.5 GPU bone-palette compute
(Session 40) · M41.0 FaceGen heads (2026-05-05) · M41 NPC spawning —
T-pose humanoids at REFR positions (2026-05-07), outfits via OTFT + LVLI
equip (2026-05-11).

**RT renderer**
M22 RT-first multi-light (SSBO lights, ray-query shadows, RT
reflections, bounded material-aware path-traced GI, SVGF temporal,
composite + ACES) ·
M31 RT performance at scale (batched BLAS, TLAS culling,
importance-sorted shadow budget, distance-based ray fallback, BLAS
LRU eviction, deferred SSBO rebuild) ·
M31.5 streaming RIS direct lighting ·
M32 landscape terrain (LAND + LTEX/TXST splatting) ·
M33 sky & atmosphere · M33.1 cloud layers 2/3 + weather fade transitions ·
M34 exterior lighting (sun arc, TOD ambient/fog/directional, interior fill
split) · M32.5 per-game cell loader parity (Skyrim SE, FO4) ·
PERF-1 wall-clock bench and CPU hot-path profile (GPU-bound finding) ·
M36 BLAS compaction · M37.5 TAA (Halton jitter, motion-vector reprojection,
YCoCg clamp, mesh-id disocclusion) ·
R1 MaterialTable refactor (`GpuInstance` 400 → 112 B, 2026-05-01) ·
M-NORMALS per-vertex tangents and LIGHT-N2 display-space fog (2026-05-02) ·
M38 water (2026-05-11) · EFFECT-LIT effect-shader intensity (2026-06-03) ·
M37 SVGF spatial filter and M37.3 ReSTIR-DI (2026-06-18) ·
R6 scratch-buffer telemetry (`ctx.scratch`).

**World and streaming**
M40 world streaming (async pre-parse, LRU BLAS eviction, interior↔exterior
swap; 2026-05-24).

**Scripting, physics, UI, audio, persistence**
M12 ECS-native scripting foundation (events + timers) ·
M28 Phase 1 physics (Rapier3D bridge) · M28.5 character controller
(2026-05-22) ·
M30 Phase 1 Papyrus parser (logos lexer + Pratt expression parser,
full AST) · M30.2 Papyrus Phases 2–4 (2026-05-23) ·
R5 Papyrus quest prototype — verdict "go ECS-native" (2026-05-16) ·
M47.0 event hooks runtime and M47.1 condition evaluator (2026-05-23) ·
M20 Scaleform/SWF UI via Ruffle · R4 SWF/GFx decision — pinned Ruffle with
ByroRedux-owned host profiles (2026-07-25) ·
M44 spatial audio (2026-05-06) ·
M45 save/load and M45.1 live load-apply (2026-06-21).

**Playable vertical slice**
P0 input and interaction (2026-08-10) · P1 traversal · P2 melee → loot
loop · P3 HUD, inventory and player body · P4 authored objective with
dialogue (MS01, 2026-09-30) · P5 F5/F9 persistence and 30-minute soak
(2026-10-01).

**Debug & diagnostics**
M15 debug logging & diagnostics · debug CLI (`byro-dbg`) with
TCP protocol and Papyrus-expression query language ·
live ECS inspection (`find`, `entities(Component)`, screenshot).

---

## Known Issues

Open items only. Each closed item is removed at the session close that closes
it; the full list as it stood on 2026-09-29 (open and closed) is in
[`roadmap-history.md` §7](docs/archive/roadmap-history.md#7-known-issues-full-open-and-closed).

### Performance and measurement

- [ ] **R6a-stale-25 — bench-of-record `a37fcba3c` is 390 commits stale**
  (2026-10-06 close). Frame-path changes since: #5154, #5018/#5191, #5062,
  #5064, #4902/#4909/#4915, #5055/#4784, #5158's exposure retune
  (`7d99ba7f0`), #5243, and this session's #5192 transmission-lobe split,
  #5204 light-identity sort, #5245 current-driven water transport and the
  sun-disc / sunset-palette changes (`c60405083`, `56786698b`). Re-run
  `scripts/fsr-bench-matrix.sh 3 300` with a
  same-machine control; no HEAD frame-time claim is current until then.
- [ ] **R6a-regress-22 — residual after the camera move** (filed 2026-09-28,
  re-stated 2026-09-30 by #5128). The original reading ("FO4 frame time
  doubled between `4c9a5b36` and `99933f87b`, content unchanged") was a
  bench-camera move: `b9e961eeb` put the spawn, and with it the stepped
  camera's origin, inside the room. Pinned-pose control on Dugout TAA
  (orbit, 300 frames, 3 runs, isolated settings;
  `docs/audits/BENCH_R6a-regress-22_pose_control_4c9a5b36_vs_99933f87b.tsv`):

  | Camera origin | `4c9a5b36` | `99933f87b` |
  |---|---:|---:|
  | Each build's own spawn (the R6a comparison) | 10.20 ms (fence 2.65) | 25.56 ms (fence 19.82) |
  | Old spawn, pinned (identical final pose on both) | 10.24 ms (fence 2.71), 330 b | 10.92 ms (fence 5.33), 168 b |
  | New spawn, pinned | 27.14 ms (fence 18.96), 1396 b | 25.57 ms (fence 19.83), 659 b |

  So the old build is as slow as the new one from the new view, and batching
  merges MORE at `99933f87b`, not less. What survives is a small, view-
  dependent delta: +0.7 ms / fence +2.6 ms from the old pose (the clean read,
  same path end to end), −1.6 ms from the new origin (the orbit ends ~190 BU
  apart there on the two builds, so that read is not strictly like for like).
  Candidates for the old-pose fence growth: `186234944` (early-fragment-test
  opaque pipeline + 29.49 MB per-frame reservoir clear), `5eb07a4f3`,
  `0572bfd5a`. MedTek TAA (34.22 → 50.38 ms) was not re-measured; presumed
  the same mechanism until pinned. Any HEAD-vs-record comparison still needs
  `--no-auto-exposure` (the record ran without it).
- [ ] **FO4 Dugout Inn TAA regressed ~10% inside `e6282349..4c9a5b36`**
  (filed 2026-09-09): 10.09 → 11.12 ms at identical fingerprint and entity
  count, while Cornell and Dugout FSR Quality improved in the same runs.
  Raw rows are in `docs/audits/BENCH_control_e6282349_vs_4c9a5b36.tsv`, so a
  bisect needs no new baseline.
- [ ] **R6a-groundcover-1 — ground cover has never been costed.** The default
  bench scenes are four interiors plus Cornell, so the ground-cover path never
  runs. The fix: run `FSR_BENCH_SCENES="gridcross" scripts/fsr-bench-matrix.sh 3 300`
  once, set `scene_entity_floor` from the observed count, and add `gridcross`
  to the default set. Until then, accept no ground-cover perf claim in either
  direction.
- [ ] **Teardown SIGSEGV** (filed 2026-09-09): all 75 `4c9a5b36` runs
  segfaulted on exit ("outstanding references"), 0 of 30 at `e6282349`. #4187
  (`4777908b`) fixed the staging-pool path and the demo scene exits cleanly;
  check the game-cell matrix's exit status on the next refresh.
- [ ] **`gpu_main` can read longer than the wall frame** (13.27 ms inside an
  11.12 ms frame), so per-pass attribution and negative "render recovery"
  cells are untrustworthy; wall and fence are unaffected. #4808 moved the
  bracket start to COMPUTE, cure unmeasured.

**Decided, do not re-file:** PERF-REGRESSION-6c56e311 (#2161). The ~2.2×
main-pass cost of glass-transmitting shadows plus the second diffuse GI bounce
was accepted on 2026-07-27 as a quality decision. Do not re-file it as a
performance finding. The measured knob table is in
[`roadmap-history.md` §7](docs/archive/roadmap-history.md#7-known-issues-full-open-and-closed).

### Correctness and content

- [ ] **BC2 world-texture mip-chain staging copy overruns by 8 bytes on long
  streaming runs** (2026-09-18): `vkCmdCopyBufferToImage … exceeds VkBuffer
  total size`, ~20 times in multi-minute `--bench-hold` sessions and 0 in
  short ones. Suspected cause: a `dds::mip_size` rounding gap. Needs a
  targeted repro plus an allocation assert.
- [ ] **FO76 `GeneratedMeshes` truncation tail** (2026-08-29): `02` 0.00%
  clean (all 2 049 truncate), `01` 95.03%, all recoverable. The suspected
  gap (#3461) has closed but the sweep was not re-run; then raise the `0.0`
  floor and regenerate baselines.
- [ ] **Fire lighting is on by default and never had its visual check.**
  `2325c1de` replaced `render/fire_lights.rs` (and its reach canary) with
  `append_combustion_surface_lights` from the transported field, which flipped
  the default as a side effect. Owed: an A/B on a vanilla torch interior (FNV
  Prospector, Skyrim BanneredMare). Derived reach should land near the
  ~512-unit vanilla LIGH radius, and fires without a companion LIGH should not
  blow out the room.
- [ ] **CHARAL runtime-dead surface.** The formulas are right: 62 constants
  were verified with zero mismatch in
  [`AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md).
  FO3, FNV, Skyrim and FO4 reach actors. Still open:
  - The Oblivion ruleset is unwired (see Status).
  - `regen` no-ops until an Oblivion-shaped `PoolRegenConfig` exists.
  - `affliction_tick_system` is never registered.
  - FO76's capture is locked with no builder; Starfield waits on pending data.
- [ ] **MQ101 residue.** The quest plays end-to-end (live-verified
  2026-09-13), but the race menu auto-accepts because no interactive UI
  exists. `Fragment_11`'s `AddRaceSpells()` now lowers onto the player's
  RACE `SPLO` set (#4415, `9813af435`); no live MQ101 replay has re-checked
  it.
- [ ] **FO3/FNV SCPT records are parsed but not executed.** The SCDA
  interpreter (M47.3) runs Oblivion quest scripts only.
- [ ] **`NiStencilProperty` is parsed but never applied** (stencil test off,
  stencil-less `D32_SFLOAT`). Needs per-material stencil variants plus a
  stencil format, so it waits for a consumer (#4213).
- [ ] **One Starfield NIF (`meshes\marker_radius.nif`) asks for a 318 MB
  single allocation**, above `MAX_SINGLE_ALLOC_BYTES` (256 MB); raising the
  cap weakens the hostile-`u32` defence. One file in the corpus.
- [ ] **Starfield CDB Phase 2: texture slots landed, closure unverified**
  ([#3398](https://github.com/matiaszanolli/ByroRedux/issues/3398)). A
  streaming `MaterialIndex` (`224a19372`) feeds the `.mat` arm of
  `merge_external_material` (`18fce7e43`): a hit forwards texture slots and
  flat-colour replacements as `Merged`, a miss keeps the Phase-1 PBR flip;
  per-slot replacements, scalars and flags translate honestly (#5190, #5196,
  #5197). Owed before closing: the issue's definition of done and a live Cydonia render.

### Infrastructure and tooling

- [ ] **Hosted CI was red on three of ten jobs** at `7bf742054` (2026-10-06);
  two were fixed at the Session 95 close and await the next push.
  - *Test + Check + Clippy*: #5308 pinned the toolchain, then this session's
    code added two pinned-1.96 lints (plugin `manual_is_multiple_of`,
    `needless_lifetimes`) and a ui `manual_div_ceil`. Fixed at the close.
  - *ECS Miri*: red since #5308 — `rust-toolchain.toml` overrode the job's
    nightly, and 1.96.0 ships no miri. Fixed at the close with
    `cargo +nightly miri` (passes locally, 83/0).
  - *Vulkan validation*: the lane reaches lavapipe now (#4987, `6d5d8fa5f`).
    But the info-level renderer log that its device gate needs also trips the
    bare `grep -F '[Vulkan]'` error gate, on loader INFO and performance-WARN
    lines; the run has no ERROR-severity validation message.
- [ ] **Offline texture-set upscale finalization.** `tools/texture-upscale`
  works end-to-end: set discovery, TOML manifests, an external ESRGAN-family
  pass, companion-map upsampling and provenance. Remaining: per-game/per-role
  DDS compression and mip generation, BC5/BC7 decode, and material-slot-aware
  discovery.

---

## Project Stats

Ground-truth as of 2026-10-06 (session close, HEAD `eb479269f`). Every
figure in this table was measured at that HEAD, not carried forward.

| Metric                                  | Value                        |
|-----------------------------------------|------------------------------|
| Rust source lines (`src/` dirs)         | ~693 967                      |
| Rust total lines (all `.rs`, excl. `target/`) | ~744 088                 |
| Source files (`.rs`, excl. `target/`)   | 1226 total · 1135 outside `tests/` dirs (+19 / +18 since the 2026-10-03 close) |
| Workspace members                       | 34 (count the `[workspace] members` block only — an unscoped `grep -c '^\s*"' Cargo.toml` returns 39, picking up quoted lines elsewhere in the file; 29 crates (incl. `menuxml`, added in Session 88) + `byroredux` binary + 4 tools: `byro-detect`, `byro-launcher`, `byro-dbg`, `texture-upscale`; `tools/nifskope` exists on disk but is not a workspace member) |
| Tests                                   | **9293 passing, 0 failing** (`cargo test --workspace --no-fail-fast` on rustc 1.96.0, 2026-10-06; 265 ignored). The full run was 9292 / 1: a parallel-runner race in which two Starfield `.mat` fixtures shared one process-global CDB-index cache key. It was fixed at this close and the bin suite re-run (2669 / 0). Always pass `--no-fail-fast` for the ground-truth count — without it, `cargo test --workspace` stops after the first binary with a failure and silently omits every crate queued behind it (Session 77 saw this first-hand: 1836 vs the true 6905) — and beware shell pipes: `cargo test … | tail` reports the *pipe's* exit code, which masked a toolchain-version failure here before the 2026-09-18 session caught it. |
| Open issue directories                  | 5299 (`.claude/issues/`)     |
| NIFs in per-game integration sweeps     | **562 057** across eight games (Skyrim SE re-measured 2026-09-29 at 33 468 over 8 archives, #3712 added Animations; FO76's tail closed by #3461 and the 2026-09-29 re-measure swept 102 968 over 4 of its 20 mesh-bearing archives; Skyrim LE 22 466 gated since fb8173fe0; earlier widenings #3369/#3466, Oblivion DLC #3925, Starfield #4440). Oblivion 9 612 · FO3 17 172 · FNV 20 746 · Skyrim SE 33 468 · Skyrim LE 22 466 · FO4 235 082 · FO76 102 968 · Starfield 120 543. |
| Per-game NIF clean-parse rate           | See the [compatibility matrix](#compatibility-matrix) — it is the single home for per-game parse rates, sweep dates and residual truncation tails. Summary only: 100% clean on all eight titles (Starfield re-measured 2026-09-24 #4440; FO76's `GeneratedMeshes` truncation tail closed by #3461 and re-measured 2026-09-29). |
| Supported archive formats               | BSA v103/v104/v105, BA2 v1/v2/v3/v7/v8 |

### Repro commands for every bench claim

> **CWD matters.** Bare `--bsa` / `--textures-bsa` / `--materials-ba2` names
> resolve against the current working directory, not the `--esm` folder. Run
> each command with CWD set to that game's `Data/` directory (e.g.
> `cd "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data"`) — and
> drop the `Game/Data/` prefix from `--esm` accordingly. Run from elsewhere and
> the archives silently fail to open; the scene loads near-empty (Prospector
> falls to 36 entities / 3 meshes and reports a spurious ~1792 FPS).
>
> **Retired rows.** The single-scene R6a-stale-15 rows (Prospector, Whiterun,
> MedTek at `8a668eff`) and the sweetroll single-mesh figure left this table on
> 2026-09-29; they are in
> [`roadmap-history.md` §8](docs/archive/roadmap-history.md#8-retired-repro-table-rows).

| Claim                                                                     | Command                                                                                                                                                                                        |
|---------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Bench-of-record (LIVE)** — full FSR matrix, all five scenes × TAA-native vs four FSR presets (stepped-camera refresh, HEAD `a37fcba3c`, 2026-09-28, 75 runs = 5×5×3 of 300 frames, median with range, 1280×720 output). Source of every FPS/ms figure in the live bench table above, including the FO4 Dugout Inn and Cornell rows. Per-scene CWD still applies. Scene subset via `FSR_BENCH_SCENES`, output dir via `FSR_BENCH_OUT`. **Refreshed 2026-09-28 at HEAD `a37fcba3c` (harness `88c23887b`); all 75 state-gated runs accepted and R6a-stale-22 resolved. Its same-machine control is the source of R6a-regress-22.** Note the default scene set is four interiors plus Cornell — it does not exercise ground cover (R6a-groundcover-1). The uncontrolled `cb44d99f6` matrix quoted under R6a-stale-22 (2026-09-22, `docs/audits/BENCH_stepped-camera_cb44d99f6.tsv`) came from this same command and is **not** the record. | `scripts/fsr-bench-matrix.sh 3 300` |
| **Same-machine control** (the method that makes a refresh interpretable — see **Standing methodology** under How to run a bench; used to prove PERF-REGRESSION-6c56e311 was code, and to prove R6a-stale-17's apparent regressions were not). Rebuild the prior record's commit in a worktree and bench it in the same session under the same load; the harness must be byte-identical at both commits (`diff` both scripts before trusting the comparison). | `git worktree add <dir> <prior-commit> && cd <dir> && cargo build --release && FSR_BENCH_SCENES="cornell prospector medtek" FSR_BENCH_OUT=<dir>/out <dir>/scripts/fsr-bench-matrix.sh 3 300` |
| **Upscaler SSIM quality matrix** — the SSIM / outlier-percentage figures quoted for every preset (Cornell Quality 0.9554 SSIM / 1.68% outliers, Performance 0.9199 / 5.39%; FO4 Dugout higher SSIM, worse outliers). Scores each preset against the native TAA render over five deterministic `--bench-camera` paths. Append `game` to score real game content instead of the redistributable Cornell scene. | `cargo test --release -p byroredux --test upscaler_quality -- --ignored --nocapture` |
| Megaton interior parse-side 929 REFRs (2026-04-19; test lives in the plugin **lib**, so a `--test parse_real_esm` filter runs 0 tests — #5070) | `cargo test -p byroredux-plugin --release --lib -- --ignored --exact esm::cell::tests::integration::parse_real_fo3_megaton_cell_baseline`                                                      |
| Per-game full mesh sweep (clean rates above; recoverable 100% gate)       | `cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`                                                                                                          |
| FO3 ESM parse floors, #3756 (index sum ≥ 44 000, measured 44 718 — an index sum that double-counts by design, not a record count; the file holds 718 952 records; placed refs ≥ 573 000; exterior cells ≥ 41 900) | `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored --exact parse_rate_fo3_esm`                                                                                       |
| FNV ESM parse floors (index sum ≥ 76 000, measured 78 575; same index-sum caveat as the FO3 row) | `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored --exact parse_rate_fnv_esm`                                                                                       |

**Rule**: every "FPS / ms / count" claim in this document must have a
repro command in this table. `/session-close` refuses edits that add
a new claim without one.

---

## Reference Materials

| Resource                   | Location                                               | Purpose                                              |
|----------------------------|--------------------------------------------------------|------------------------------------------------------|
| nif.xml (niftools)         | `docs/legacy/nif.xml` (authoritative at `/mnt/data/src/reference/nifxml/nif.xml`) | NIF format spec (8 563 lines)                        |
| Gamebryo 2.3 source        | External drive                                         | Byte-exact serialization reference                   |
| FNV / FO3 / SkyrimSE data  | Steam library (env var overrides, see README.md)       | Primary test content                                 |
| Creation Kit wiki          | uesp.net                                               | Record type documentation                            |
| Coordinate system docs     | `docs/engine/coordinate-system.md`                     | Transform pipeline, CW convention, winding chain     |

---

## Crate Map

| Crate                         | Focus                                                                                                           |
|-------------------------------|-----------------------------------------------------------------------------------------------------------------|
| `byroredux-core`              | ECS, math, animation engine, string interning, Form IDs                                                         |
| `byroredux-renderer`          | Vulkan + RT (ash, gpu-allocator, acceleration manager, pipelines, SVGF, TAA, composite, caustic, SSAO)          |
| `byroredux-platform`          | winit, raw handles                                                                                              |
| `byroredux-plugin`            | Plugin manifests, DAG resolver, ESM/ESP/ESL parser, cell loader helpers                                         |
| `byroredux-nif`               | NIF binary parser (~250 dispatch arms — live count in `crates/nif/src/blocks/mod.rs`), import-to-ECS, animation import |
| `byroredux-bsa`               | BSA (v103/v104/v105) + BA2 (v1/v2/v3/v7/v8, GNRL + DX10) readers                                                 |
| `byroredux-bgsm`              | FO4 / Skyrim SE / FO76 external material files (BGSM / BGEM v1–v22)                                              |
| `byroredux-sfmaterial`        | Starfield `materialsbeta.cdb` component-database reader (`.mat` JSON descriptors resolve through it)            |
| `byroredux-spt`               | SpeedTree `.spt` binary parser (Oblivion 4.x / FO3+FNV 5.x), placeholder-billboard fallback                     |
| `byroredux-facegen`           | FaceGen sidecar parsers — `.egm` geometry morphs, `.egt` texture morphs, `.tri` animated morph targets          |
| `byroredux-physics`           | Rapier3D bridge (M28 Phase 1, kinematic character controller M28.5)                                             |
| `byroredux-scripting`         | ECS-native events + timers + condition evaluator (M47.1) + `papyrus_demo` hand-translations                     |
| `byroredux-papyrus`           | Papyrus `.psc` parser (lexer + Pratt expression parser + statement/script parsers + full AST, M30.2)            |
| `byroredux-ui`                | Scaleform/SWF via Ruffle                                                                                         |
| `byroredux-debug-ui`          | Embedded egui debug overlay (egui-ash-renderer Vulkan pipeline, F-key toggle)                                   |
| `byroredux-debug-protocol`    | Wire types + component registry for debug CLI                                                                    |
| `byroredux-debug-server`      | TCP debug server (Late-stage exclusive system)                                                                   |
| `byroredux-cxx-bridge`        | C++ interop via cxx                                                                                              |
| `byroredux-audio`             | 3D spatial audio via kira 0.10 (spatial sub-tracks, reverb send, streaming music — M44)                          |
| `byroredux` (binary)          | Game loop, cell loader, fly camera, animation system, render data collection, NIFAL translation boundary         |
| `tools/byro-dbg`              | Standalone debug CLI (TCP client, REPL)                                                                          |
| `tools/texture-upscale`       | Offline BSA/BA2 texture-set discovery and reference-guided semantic-map upscaling                                |
