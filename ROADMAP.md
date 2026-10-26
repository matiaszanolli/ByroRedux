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

**Last session close:** Session 104, 2026-10-24. Workspace
numbers (tests, LOC, files, issue dirs) live in [Project Stats](#project-stats)
only.

**Gates.** `rust-toolchain.toml` pins rustc 1.96.0 (#5308), which is also the
bin crate's MSRV since wasmtime 49. On it, the CI form `cargo clippy
--workspace -- -D warnings` was **clean** at the last run (2026-10-22, after the
Session 103 `head_look` lint was fixed; `--all-targets` is not, Known Issues). All 11 303 tests
pass (2026-10-24). The lock-order detector is clean across the workspace
(`BYRO_LOCK_ORDER_CHECK=1`, `--exclude byroredux-ui`, 2026-10-24; it caught
Session 104's Find and Flee, and a bare `cargo test` does not run it). The bench-of-record below (`d9a201c71`, 2026-10-22) was refreshed with a same-machine control and shows HEAD 1–9% slower than the outgoing record (R6a-regress-23). Its older open
regression, **R6a-regress-22**, was the bench camera moving with `b9e961eeb`'s
interior spawn ladder, not renderer code (#5128, 2026-09-30); a small residual
stays open — the top-of-frame all-slots fence wait that dominates GPU-bound
frames (#5365, the tracked throughput half of closed #4606/#5117).

**What works today.**
- **Content loading.** Interior cells load and render from unmodified game
  data for Oblivion, FO3, FNV, Skyrim SE and FO4, plus a walkable Starfield
  Cydonia interior, all through one `cell_loader`. Exterior grids load with
  LAND terrain and splatting, stream as the player moves (M40), and draw
  distant LOD: `.btr`/`.bto` with a per-game band ladder on Skyrim/FO4, Skyrim
  `.btt` tree LOD (#4913), `_far.nif` on Oblivion/FO3/FNV (M35). FO4 precombines decode from CSG (M49).
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
  Fourteen AI package procedures run by default (locomotion seven + Eat/Sleep +
  Dialogue + graph locomotion + Flee, UseWeapon, UseItemAt and Find; Accompany
  and Ambush ride Follow and Guard), with authored walk clips and
  KCC-backed steps (M42). Faction hostility starts combat on sight and
  disengages after a grace period (#4414, #4816). FNV-era ragdolls run on
  Rapier (M41.x).
- **Player.** Rapier kinematic character controller (M28.5), swimming and
  diving (WATAL W1), a visual body with a first/third-person toggle
  (`a070baaad`), and on the P2 fixture a melee → damage → death → loot loop.
  Native HUD vitals and objective text. Native F5/F9 quicksave/quickload
  through the deferred player-save queue, with quest state, cross-cell door
  saves, graceful quit and a validation-clean 30-minute soak all gated
  (P5, 2026-10-01; the soak's restore defect #5155 fixed 2026-10-22).
- **Scripting.** ECS-native event hooks and condition evaluator
  (M47.0/M47.1). Compiled `.pex` is decompiled and lowered through a
  recognizer chain (QUST fragments, aliases, SCEN/PACK scenes; MQ101 plays
  end-to-end), with a bounded Papyrus interpreter for the fragments,
  reference scripts and quest-alias scripts it declines (M47.2). An SCDA interpreter runs Oblivion/FO3/FNV quest
  scripts and the object scripts on placed references (M47.3). A first magic-runtime slice covers spell lists and
  the RACE `SPLO` racial pair (#4415).
  Dialogue: NPC activation selects a topic, a native response surface
  presents the INFO, and its `TIF_` fragments dispatch (P4).
  Story Manager (#5366 Phases 0–4, 2026-10-07 → 10-08): Skyrim+ quests autostart
  from the authored event tree — `SMBN`/`SMEN`/`SMQN` decode, `KILL` /
  `CLOC` producers, condition-gated dispatch through the M47.1
  evaluator into the canonical quest lifecycle, live-gated by
  [`sm1-story-manager.sh`](docs/smoke-tests/sm1-story-manager.sh).
- **Save/load.** Full-ECS snapshot with validation gates, atomic write and
  live load-apply (M45/M45.1).
- **Audio.** kira spatial audio: footsteps, ambient, music, per-cell reverb,
  water routing (M44).
- **UI.** Ruffle-hosted Scaleform menus with Skyrim (AVM1) and FO4 (AVM2) host
  profiles (R4). Native MenuXml HUDs for Oblivion/FO3/FNV, and the vanilla
  Skyrim/FO4 `hudmenu.swf` via `--hud` (M48.4–M48.7).

**What doesn't work yet.**
- Redux-native plugin loading: the manifest → `DataStore` →
  `DependencyResolver` layer (`crates/plugin`) has no production caller —
  the live path is `parse_esm_with_load_order` (#5609).
- AI beyond locomotion: 7 of ~17 package procedures need subsystems that do
  not exist (item use, package combat, magic). NAVM pathfinding
  covers the five pre-FO4 games including cross-tile joins (#3802); FO4's
  compressed `NVNM` body is still undecoded.
- FO76/Starfield actors have no walk-clip source (FO4's run through its Havok
  2014 behavior graphs).
- Dialogue: Skyrim+ `.fuz` voice (#5912). The greeting, line-lifetime, force-greet
  and FO3/FNV voice layers are live (#5367,
  [`dialogue-trees.md`](docs/engine/dialogue-trees.md)). Story Manager
  producer breadth is open (#5366 — `SCPT` awaits the `SendStoryEvent`
  native, `LEVL`/`SKIL` a leveling transition).
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
- Story Manager (#5366): **Phases 0–4 shipped**, the Skyrim-dialect runtime
  plus the FO4/SF decode (detail in the M43 row under Tier 7). Remaining:
  producer breadth as subsystems land and the open `DNAM`/`HNAM`/`MNAM`
  semantics ([`story-manager.md`](docs/engine/story-manager.md)).
- Dialogue trees (#5367): all layers landed and live-gated (`dt1`/`dt2`
  smokes); remaining is Skyrim+ `.fuz` voice (#5912).
- RT lighting and material recovery: R0–R3 are complete. The rest is tracked
  in [`rt-lighting-material-recovery.md`](docs/engine/rt-lighting-material-recovery.md).
- WATAL: W0 and W1 are closed on FNV and Skyrim (2026-10-26); distant water is per
  cell (#5243). Calm water has wind-driven swell that shoals and breaks at shores,
  river currents follow the bed (`WaterBedField`); underwater shafts are a sun-ray
  march with a screen-space occluder test ([`watal.md`](docs/engine/watal.md)).

### How to run a bench

**Repro-command CWD note:** bare `--bsa` / `--textures-bsa` / `--materials-ba2` names resolve against CWD, not the `--esm` folder. Run each bench with CWD set to that game's `Data/` directory. Run from elsewhere → archives silently fail → scene loads near-empty (Prospector: 36 entities / 3 meshes / spurious ~1792 FPS).

**CWD-immune alternative (#3346):** `--game <profile>` (`fnv` / `fo3` / `skyrim` / …) expands to *absolute* `--esm` / `--bsa` / `--textures-bsa` paths from `assets/debug_profiles.toml` via `expand_game_profile_args` (`byroredux/src/boot/cli.rs`), so it works from any CWD and cannot mistype an archive name. Prefer it for ad-hoc runs and audits:
`cargo run --release -- --game fnv --cell GSProspectorSaloonInterior --bench-frames 300 --bench-hold`.
The bench-of-record table below deliberately keeps the bare-name + `cd` form for apples-to-apples continuity with the historical record — do not restate those numbers under a different invocation shape.

**Standing methodology.** Every refresh includes a same-machine control:
rebuild the outgoing record's commit in a worktree and bench it in the same
session, on the same harness. That control is what proved
PERF-REGRESSION-6c56e311 was code. It also produced R6a-regress-22 — later
shown to be the bench camera's origin moving, not code (#5128) — so pin
`--camera-pos` / `--camera-forward` across any spawn-placement change.
`fsr-bench-matrix.sh` now records each run's
`camera_pos` / `camera_forward`, and the report prints them per scene. Also
isolate `BYROREDUX_SETTINGS_PATH` so persisted menu settings cannot leak into
runs (#4947). A genuinely idle-machine run has never been done. FSR Quality
is the engine default; the `TAA (native)` column stays the historical
reference and is reachable with `--upscaler taa`.

Superseded records (R6a-stale-15 at `8a668eff` through `4c9a5b36`) are in
[`roadmap-history.md` §2](docs/archive/roadmap-history.md#2-superseded-bench-records);
raw rows for every matrix are in `docs/audits/BENCH_*.tsv`.

### Bench-of-record (LIVE) — stepped-camera refresh (2026-10-22, HEAD `d9a201c71`)

Full matrix, 5 scenes × 5 configs × 3 runs of 300 frames, median with range,
1280×720 output, per-scene CWD set to that game's `Data/`. **75 runs, zero
rejections**, every fingerprint gate passing. Raw rows:
`docs/audits/BENCH_stepped-camera_d9a201c71.tsv`. Repro:
`scripts/fsr-bench-matrix.sh 3 300`, harness `994620123` (the previous harness
plus two appended timer columns the outgoing build does not emit).

**Same-machine control:** the outgoing record `a3a7a43b0` rebuilt in a
worktree and run scene by scene, alternating with HEAD, under HEAD's harness
copied in byte-identically (its TSV header still stamps `a4ede5fa0`):
`docs/audits/CONTROL_stepped-camera_a3a7a43b0_under-994620123.tsv`. Same camera
pose (Prospector within 1 BU) and same draw counts on all five scenes.

**Run conditions:** RTX 4070 Ti at 20–25% utilisation, 1-minute load 1.9–2.9 per
launch (7.7 at Whiterun HEAD), desktop resident, settings path fresh per run. Not idle.

| Scene | TAA (native) | FSR Quality | net recovery | FSR Performance |
|---|---:|---:|---:|---:|
| Cornell (37 ent, redistributable control) | **86.3 FPS / 11.59 ms** | 119.6 FPS / 8.36 ms | +3.23 ms (+28%) | 167.4 / 5.97 (+48%) |
| Prospector (2359 ent) | **83.8 FPS / 11.93 ms** | 101.9 FPS / 9.81 ms | +2.12 ms (+18%) | 137.8 / 7.25 (+39%) |
| Whiterun BanneredMare (5853 ent) | **83.7 FPS / 11.94 ms** | 135.1 FPS / 7.40 ms | +4.54 ms (+38%) | 192.1 / 5.21 (+56%) |
| MedTek Research 01 (24923 ent) | **35.4 FPS / 28.22 ms** | 59.7 FPS / 16.75 ms | +11.47 ms (+41%) | 71.4 / 14.00 (+50%) |
| FO4 Dugout Inn (9254 ent) | **66.1 FPS / 15.12 ms** | 93.2 FPS / 10.73 ms | +4.39 ms (+29%) | 116.8 / 8.56 (+43%) |

FSR Quality nets +18% to +41%, Performance +39% to +56%; `native-aa` loses 2% to 5%.

**Against the control (frame ms, `a3a7a43b0` → `d9a201c71`), TAA / Quality:**
Cornell 10.60 → 11.59 (+9.3%) / 8.09 → 8.36 (+3.3%); Prospector 11.73 → 11.93
(+1.7%) / 9.43 → 9.81 (+4.0%); Whiterun 11.17 → 11.94 (+6.9%) / 6.85 → 7.40
(+8.0%); MedTek 26.30 → 28.22 (+7.3%) / 15.48 → 16.75 (+8.2%); Dugout
14.78 → 15.12 (+2.3%) / 10.63 → 10.73 (+0.9%). Every scene is slower at HEAD,
with run ranges of at most 0.7 ms. Cornell is the content-identical control
(37 entities, 32 draws on both builds), so its +0.99 ms TAA is renderer cost
(R6a-regress-23); the game scenes' populations moved by under 3%. The outgoing
record's own figures differ from the control's by up to ±4% (Cornell TAA 10.23
vs 10.60 ms), hence the control. Prospector's TAA *gpu_main* is bimodal on both
builds (about 4–5 or 10 ms between runs) with steady wall time: use frame ms.
**No scene mounts `--scripts-bsa`, so this record cannot see the Papyrus
interpreter** (first Known Issue).

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
| Fallout 76        | BA2 v1        | **100%** (**102 980 / 102 980**; re-measured 2026-10-03 (#4628) over 5 of the 20 mesh-bearing archives (Meshes, StaticMeshes, both `GeneratedMeshes`, Startup) — #3461 (2026-09-02) closed the 3 056-NIF truncation tail the 2026-08-29 sweep found, so the old `GeneratedMeshes02` 0.00% / `GeneratedMeshes01` 95.03% split no longer exists) | Parse only. |
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
- #5155 (soak restores pinned `grounded=false`) fixed 2026-10-22: a blocked fall asks the walkable probe.
- Skyrim `p2-melee-core.sh` passes end to end since #5161 (`5ae7f8ad4`) and
  #5246 contained the ragdoll solver explosion. #5352 closed the
  recovery-substep gap in that clamp (`1efdfda4a`), and rapier 0.36's BVH
  broad phase removed the multi-SAP panic class outright (#5488).
- P1: Skyrim's W1 water leg runs since 2026-10-26 on the northern sea shore (Tamriel (-9,-16)); the Riverwood bank wedge that blocked it is still an open character-controller defect. P4's
  greeting / line-lifetime / force-greet layers landed with #5367
  (Skyrim force-greet and voice remain). Gamepad sources are not started.

**Second route: Fallout: New Vegas (2026-10-22).** P0–P5 run on the FNV fixture, with
FNV P3/P4 twins (`p4-fnv-quest-route.sh`, `p5-fnv-quest-persistence.sh`): VCG02 opened by
Sunny's force-greet and moved by ObScript result scripts. Found and fixed: stage/dialogue
result scripts never ran, a force-greet loop behind #6052, link-only topics refused, two VM
defects, tuple-keyed maps failing JSON saves, #5155. HUD gains the rad gauge. Detail:
[`playable-vertical-slice.md` §Second route](docs/engine/playable-vertical-slice.md#second-route--fallout-new-vegas-2026-10-22).

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
| M47.3 | ObScript VM (quests + object scripts) | **Shipped (2026-10-18), live-verified on FNV by `docs/smoke-tests/m47-3-obscript.sh`** (object-script attach, `OnLoad`, `OnTriggerEnter`, `OnDeath`, item `OnAdd`, `PlayGroup`; quest ticking earlier) **and measured against the three masters for the rest.** An SCDA bytecode interpreter (opcode ids recovered by aligning each script's `SCTX` source with its bytecode over the masters; `obscript_tools` measures it) runs on one host, `ObScriptCommandHost`, owned by a quest, a placed reference or an inventory item; it models **96 % / 93 % / 94 %** of the call-site lines of FNV / FO3 / Oblivion (`obscript_tools names <ESM> all`). **Quests (phase 1):** each running quest's `GameMode` ticks on the authored `QUST` delay (`SetQuestDelay` overrides, saved) and lowers onto `QuestStageState` / `Globals`; messages reach the native UI; FO3/FNV objective commands; getters through the M47.1 evaluator (id = `0x1000` + CTDA index). **Phase 2:** reference-bound actions (`Enable`/`Disable`/`MoveTo`/`EVP`/`Activate`/`AddScriptPackage`, `FORMAT_MAJOR` 110) and the object-script blocks (`GameMode`, `OnLoad`, `OnDeath`, `OnActivate` replacing default processing, the three trigger blocks), locals in the saved `ScriptVariables`. **Phase 3:** three expression-grammar gaps that had made whole classes of vanilla expressions wrong are fixed — a call on a reference (`' r<ref>' 'X'…`, 14 % of FNV's expressions), remote variables (`Quest.var`, read and written by ordinal) and `' Z'` reference literals — plus exact reference variables (`VmState::ref_locals`; saved as two 16-bit halves, no shape change) and an effect bridge that lowers ~60 commands onto the Papyrus `Effect` runtime (items, combat, factions, locks, perks, spells, ownership, `PlaceAtMe`, `MarkForDelete`, `Say`/`SayTo`, `StartConversation`, `Enable`/`DisablePlayerControls`, `ShowMessage`, …) and the GECK actor-value codes (`GetAV`/`SetAV`/`ModAV`/`ForceAV`/`DamageAV`/`RestoreAV`, table checked against every vanilla call by `obscript_tools avcodes`). More getters (`GetLinkedRef`, `GetParentRef`, `GetItemCount`, `IsInCombat`, `GetCurrentTime`, …; the `GameHour` global now follows the clock), and more blocks: `OnHit`/`OnHitWith`, `OnStartCombat`/`OnCombatEnd`, `OnPackageStart`/`Done`/`Change`, `SayToDone`, and the item blocks `OnAdd`/`OnDrop`/`OnEquip`/`OnUnequip` (`RemoveMe` ends the block). **Placement and animation:** `GetPos`/`SetPos`/`GetAngle`/`SetAngle`/`Rotate`/`GetScale`/`SetScale` (axis arguments decoded), the image-space-modifier trio, `AutoSave`, `GetSitting`, and `PlayGroup`/`IsAnimPlaying` on a placed object's own `NiControllerSequence`s (`NamedSequences`, captured at NIF import). The phase-1 note that MS23 self-advances to stage 90 live is withdrawn: it predates the call-on-a-reference fix, and MS23's gates are distance-gated. **Caster, world and bookkeeping (close-out):** a caster runtime (`magic_effects.rs`: `Cast`, `CastImmediateOnSelf`, `Dispel`, `HasMagicEffect`, the `ScriptEffectStart`/`Update`/`Finish` blocks, `OnMagicEffectHit`, scripted ingestibles through the menu's `Consume`), `PlayIdle` on the KF games and `PickIdle`, `Look`/`StopLook` head tracking, `PlayMagicShaderVisuals` (the EFSH fill glow) and `TriggerHitShader`, `ForceWeather`/`SetWeather`, `Resurrect`, `OnReset`, `OnDestructionStageChange`, `IsInInterior`/`GetInWorldspace`/`GetOpenState`, Pip-Boy notes, a kill ledger (`GetDeadCount`/`IsKiller`), `IgnoreCrime`/`SetAlert`/`GetDetected`/`SendAssaultAlarm`, `DropMe`, `RemoveAllTypedItems`, `GetGS`, `FORMAT_MAJOR` 111. The live gate `docs/smoke-tests/m47-3-obscript.sh` runs the object blocks, `PlayGroup`, a chem's script effect, `Resurrect` and the getters on FNV; the exterior weather switch and `PlayIdle` were checked live too. **What is left is a tail of single commands** (about 4-7 % of call-site lines), each blocked by state the engine does not keep — a crime system that raises and consumes the (stored) bounty/fame ledger (witnessed Oblivion theft, assault and murder raise it since 2026-10-23; guards do not read it), Oblivion path grids, a disposition value, the calendar's weekday epoch, a knockdown cycle, a fast-travel/challenge UI; the table in [`scripting.md`](docs/engine/scripting.md) names each. Detail and the full table: [`scripting.md`](docs/engine/scripting.md). | M47.2, M42.10 |

#### Tier 5 — Renderer polish (quality, not capability)

| #     | Milestone | State | Depends on |
|-------|-----------|-------|------------|
| M35   | Terrain LOD | **Shipped:** Skyrim/FO4 `.bto` objects and `.btr` terrain on a per-game `Ultra.ini` band ladder (`d96110eb`), with `_n`/`_msn` normal maps. Oblivion `_far.nif` placement LOD (#1726); FO3/FNV `blocks\` quad LOD (#3321). A derived far plane (`9e96a9f9`). The live `terrain.seams` gate in `m-exteriors.sh` (#2371). **Open:** VWD culling is blocked because baked `.bto` quads carry no per-object ids (#3307). `.btr` diffuse/normal sampling mode (#4912). Skyrim `.btt` distant trees are never drawn (#4913). The reversed-Z flip needs hardware validation (#3308). LAND value-plausibility guards ([`exterior-readiness-plan.md`](docs/engine/exterior-readiness-plan.md) Tranche C item 5). | M32 |
| M39   | Texture streaming | Mip-chain-aware loading: upload low mips immediately, stream high mips on demand. Memory budget with LRU eviction. | — |
| M29.3 | Pre-skinned raster path | Make `triangle.vert` read pre-skinned vertices from the per-entity `SkinSlot` output instead of doing inline bone blending, and re-add the `VERTEX_BUFFER` usage dropped in #681. This saves ~50 ALU ops per skinned vertex, but makes raster depend on the compute pass. The recorded precondition is that the compute + BLAS-refit chain has proved stable on visible animated content. | `1ae235b`, #681 |

#### Tier 6 — Engine infrastructure

| #     | Milestone | State | Depends on |
|-------|-----------|-------|------------|
| M24.2 | ESM Phase 2 — QUST stages + PERK entries | **QUST done:** [`parse_qust`](crates/plugin/src/esm/records/misc/quest.rs) covers startup/shutdown stages, conditional log entries, objectives, and version-aware targets with load-order remapping, and the scripting runtime consumes all of it. **PERK:** all three PRKE entry bodies decode. **Open:** per-`function_type` EPFD decoding and per-entry CTDA. DIAL conversation trees are M43 scope. | R2 |

#### Tier 7 — Deep gameplay systems

| #   | Milestone | State | Depends on |
|-----|-----------|-------|------------|
| M42 | AI packages | **Shipped:** PACK decode (PKDT/PSDT/PLDT/PTDT, #446). CTDA-gated package selection through the M47.1 evaluator, failing open on out-of-catalog functions (M42.2). Re-evaluation at game-minute boundaries and on Papyrus `EvaluatePackageRequest` (M42.9, #2652). Seven procedures — Sandbox seat with per-marker reservations, Wander, Travel, Follow, Escort, Guard, Patrol — run by default behind a `BYRO_NO_AI_LOCOMOTION=1` kill-switch, with authored per-game walk clips at stride-matched speed and KCC-backed steps (M42.10/M42.11, 2026-09-18). Plus ambient hostility (#4414), disengagement (#4816) and re-seating after a save load (#4815). Eat/Sleep (`00f580e09`, walk through `Transform` #5373) and Dialogue (force-greet, #5367 Phase F, modeled player-targeted Conversations only #5376) run too. FleeNotCombat (`flee_package_system`: flee from the `PTDT` target to its authored distance, or hold at a `PLDT` refuge) and UseWeapon (`use_weapon_package_system`: `PKW3`/`PTD2` decoded; raises the ObScript `UseWeaponRequest` for a specific weapon at a specific target: 60 of 82 FNV packages qualify structurally, those with unmodelled conditions are declined) run (2026-10-24). Accompany runs as a Follow (the target-destination lead is not modelled), and Ambush waits on Guard and gates ambient hostility to its ambush zone. UseItemAt installs Eat's walk-and-sit and is live-gated on Vault 3's Motor-Runner, which also fixed Eat/Sleep's seat step never firing for an actor standing above its marker. Find hunts, takes, attacks, greets or activates its target (`find_package_system`; items by Ammo/Alchemy/Food type or base, the Food Item flag decoded). **Open:** CastMagic (no FO3/FNV package uses it), Find for flora/furniture and the other object types, and UseWeapon's weapon-class packages (the object-type enum numbering contradicts the vanilla data), place to stand and reload/hold-fire flags. On FO3/FNV the sit fallback is the legacy sleep path (#5390), and creatures never take furniture (#5495). Also: calendar-aware scheduling, sit-enter beyond FNV/FO3, legacy sleep/lean marker disambiguation, and FO4+ walk sources. NearReference target resolution was deprioritized (~12% of targets resolve). Trace: [`npc-spawn-ai-packages.md`](docs/engine/npc-spawn-ai-packages.md). | M28.5, M41 |
| M43 | Quests & dialogue | **Shipped:** the quest core — version-aware stages, logs, objectives and targets; full lifecycle transitions; Papyrus quest effects; save-persistent progress; loaded-reference alias fill with conditions and reservations; faction/inventory injections. `quest.*` observability commands, with [`m43-quest-runtime.sh`](docs/smoke-tests/m43-quest-runtime.sh) driving the production path. **Shipped (2026-10-07):** the Story Manager slice (#5366 Phases 0–1) — `SMBN`/`SMEN`/`SMQN` decode into `EsmIndex.story_manager_nodes` (census floors pinned: 571 nodes / 24 events / 1191 of 1811 Skyrim quests SM-referenced), a dispatcher walking each event's node chain through the M47.1 evaluator into the canonical lifecycle, `KILL`/`CLOC` producers, live-gated by [`sm1-story-manager.sh`](docs/smoke-tests/sm1-story-manager.sh) (six quests on the boot-time CLOC — the pre-decode continue-always placeholder, superseded at Phase 3 by the authored two-start set). **Shipped (2026-10-07):** SM Phase 2 (#5366) — the event payload is the wire format's R1/R2/L1/L2 positional slots (corpus-decoded: `DA08KillFriendNode`'s tutorial killer conditions run on R2, `WIKill06`'s `Victim` alias fills from R1, floors in `parse_real_esm.rs`); `RunOn::EventData` node conditions resolve them, `FromEvent` (`ALFE`/`ALFD`) alias fills bind from the raising event through the P4 refresh, the CLOC producer keys on the cell's `XLCN` LCTN carrying old/new as L1/L2, `GetInFaction` (Skyrim fn 71) joined the M47.1 catalog, and the new `sm.event` console command raises the real marker for live gating (MGSuspension selectivity legs). **Shipped (2026-10-08):** SM Phase 3 (#5366) — the `DNAM` policy bits decoded (0x1 random / 0x10000 do-all-before-repeating / 0x20000 shares-event, anchored on the `*SHARES*`-named nodes and the radiant-cycle cluster) and `RNAM` per-quest reset windows; the walk honors them (random shuffled chains stopping at the first fire, round-robin pools, consume-on-processed for non-sharing nodes — the live boot set is now the authored two-start set with `CRHoldExpansion` as the pinned consume-negative), and node state (fired marks + reset timestamps over `StoryClock`) is save-persistent so a quickload cannot resurrect a fired radiant (round-trip test + the F5/F9 leg of `p5-quest-persistence.sh` extended to the SM-fired `WIGreeting`). **Shipped (2026-10-08):** SM Phase 4 (#5366) — the FO4/SF dialect census: same `DNAM` vocabulary (SF's 0x50001/0x70001 are combinations of the same bits), pools at scale (`RETravelQuests` 44; Minutemen measured 14 links with `HNAM` 72.0h and one 4320.0h `RNAM`), `RNAM` pairing identical to Skyrim, typed-raw `HNAM` (hours-shaped float, node-reset candidate — the runtime does not gate on it) + `MNAM` tail; the `AHEL` producer landed at the conversation-open surfaces (activation greeting + force-greet, `OnStoryHello`'s L1/R1/R2); every large FO4 pool is fn-576-gated (corpus-checked full-path reachability), so the pool-cycling gate runs on the fully-reachable `SuperMutantConversationQuests` — faction-crafted R1/R2 through the Phase-2 run-on, faction-negative refused, seeded cycling across the pool (`fo4_radiant_pool_cycles_on_real_content`), with the `story_manager_fo4_dialect_floor` census floor. Design: [`story-manager.md`](docs/engine/story-manager.md). **Shipped (2026-10-07):** the dialogue greeting / Random-SayOnce-Goodbye / force-greet layers (#5367 — generic-greeting fallback opens quest-less conversations, the line-lifetime flags select/persist/close on, `PKDD` force-greet walks and opens without activation; live-gated by [`dt1-dialogue-layers.sh`](docs/smoke-tests/dt1-dialogue-layers.sh); design: [`dialogue-trees.md`](docs/engine/dialogue-trees.md)). **Shipped (2026-10-08):** dialogue voice, #5367 Phase V — per-line voice playback on the FO3/FNV path: `sound\voice\<plugin>\<voice-type>\<quest>_<topic>_<formid>_<n>.ogg` resolved through the NPC's VTCK + the owning quest/topic EDIDs (INFO records carry none of their own), `Fallout - Voices1.bsa` opened through the FNV sounds profile (its first consumer), segments scheduled sequentially through kira with the voice duration driving the Phase-L Goodbye close; dt1's voice leg gates it (fresh-boot greeting = 1 segment / 5.0 s in `maleuniquedocmitchell`). Skyrim+ `.fuz` (FUZE + raw lip + RIFF xWMA, #5464) stays V2 scope (#5912). **Shipped (2026-10-08):** the Skyrim force-greet dialect — `PackRecord::force_greet` resolves both authored shapes (FO3/FNV procedure-15 `PKDD`; Skyrim's `ForceGreet` tree leaf with its `Topic` data input, census 339 packs (5 own-tree + 334 template instances), >=320 own-topic instances and >=17 generic, pinned by `force_greet_skyrim_tree_dialect_floor` in `parse_real_esm.rs`), `dialogue.forcegreet` installs either, and `dt2-skyrim-forcegreet.sh` gates the topic resolution arms and the activation-less DialogueGenericHello open. **Open:** SM producer breadth (#5366 — `SCPT` awaits the `SendStoryEvent` Papyrus native, `LEVL`/`SKIL` a leveling transition; open `DNAM` bits 0x2/0x40000 and `HNAM`/`MNAM`/`XNAM`/`QNAM` semantics), Skyrim+ `.fuz` voice (#5912), reference collections, location-run-on conditions (L-tagged CTDAs need a location runtime), created-object spawning, and the dialogue UI polish beyond the native page. These are subsystem boundaries, not missing QUST bytes. | M24.2, M41, M47.1 |
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
| Wave-4 feature tracks | The 2026-10 dependency refresh's candidate tracks, completed 2026-10-10 (details in [dependency-update-followups-plan.md](docs/engine/dependency-update-followups-plan.md) §Wave 4). **Surface-aware footsteps and impacts — implemented in full** (`886edcded`, `441d90c2f`, `1291f370a`): the Havok surface material flows from the bhk tree through `RigidBodyData::surface_material` + leaf-aligned `part_surface_materials` into each collider's user-data; footsteps select per-material (`select_footstep_sound`), and clutter impacts play from rapier's `ContactForceEvent::started` crossings (`impact_audio_system`, engine's first contact events) — both maps wait on IPDS/IPCT decode, so production stays a no-op until content data exists. **Zero-copy Scaleform — stood down**: #4717's pacing cut pass counts 4–6×, so the 5–8 ms per pass no longer clears the "still hurts" gate. **Cloth — blocked** on decoding `BSClothExtraData`'s packed Havok blob (the PHYSAL FO4+ problem), a research milestone of its own. **Collider debug view — assessed**: the real prerequisite is a line-list render pipeline (estimate M, verifiable on lavapipe); the color-by-material overlay is a consumer of the user-data this Wave put in place — schedule when a live physics-debugging need arrives. |

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
| Scripting                                    | ECS-native; bounded interpreter fallback under a per-frame statement budget | Eliminates Papyrus queue latency, stack serialization, orphaned stacks. R5's one-quest prototype (closed 2026-05-16) confirmed the call; M47.2 builds on it.                             |

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

**Tripwire today** (re-measured 2026-10-22 with `git ls-files '*.rs' | xargs wc -l`;
counts include inline test modules, not dedicated test files). Over the line:
`crates/physics/src/world/mod.rs` 5 617, `byroredux/src/inventory.rs` 4 442,
`byroredux/src/material_translate.rs` 4 415, `crates/renderer/src/vulkan/volumetrics.rs` 4 281,
`crates/physics/src/water.rs` 4 208, `crates/plugin/src/esm/records/misc/world.rs` 3 508.
`translate/effects.rs`, `fragment/vm.rs` and `condition.rs` are now hub directories (#5800–#5804).
Before splitting one, grep for `include_str!` source-shape tests that read it.

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

One-liners grouped by area. Full rows for milestones closed before 2026-09-29
are in [`roadmap-history.md` §5](docs/archive/roadmap-history.md#5-tier-17-milestone-tables-with-full-rows);
per-milestone scope is in `git log`; session context is [HISTORY.md](HISTORY.md).

**Graphics foundation** — M1 Vulkan init chain · M2 GPU geometry ·
M4 ECS-driven rendering · M7 depth buffer · M8 texturing · M13 directional lighting.

**ECS, plugins, coordinates** — M3 ECS foundation (World, Component, Storage,
Query, Scheduler, Resources, string interning) · M5 plugin system (stable Form
IDs, DAG resolver) · M6 legacy bridge (per-game parser stubs) · M17 coordinate
fix (CW rotation, SVD degenerate repair) · R7 scheduler access declarations
(`Access` builder, `sys.accesses`) · M27 parallel system dispatch (2026-05-23) ·
M46.0 multi-plugin CLI (repeatable `--master`, #561).

**NIF parser overhaul (N23 series)** — N23.1 trait hierarchy · N23.2 shader
completeness · N23.3 Oblivion block types · N23.4 FO3/FNV validation · N23.5
skinning · N23.6 Havok collision skip + compressed mesh · N23.7 Fallout 4 ·
N23.8 particles · N23.9 FO76/Starfield · N23.10 test infrastructure · R3
per-block-type parse histogram (`nif_stats --tsv`, seven-game baselines). The
block dispatcher's live arm count is in `crates/nif/src/blocks/mod.rs`.

**Asset pipeline** — M9 NIF parser · M10 NIF→ECS import · M11 BSA reader ·
M14 DDS texture loading · M16 ESM parser & cell loading · M18 Skyrim SE NIF ·
M19 full cell loading · M26 BA2 archive support (v1/v2/v3/v7/v8, zlib + LZ4) ·
NIFAL (2026-05-28) — the canonical parse-time translation boundary · M49 FO4
precombined geometry (CSG reader, 2026-06-02, #1351).

**ESM records (M24 Phase 1)** — Items (WEAP/ARMO/AMMO/MISC/KEYM/ALCH/INGR/
BOOK/NOTE), containers, leveled lists (LVLI/LVLN), NPC_, RACE, CLAS, FACT,
GLOB, GMST; SCPT pre-Papyrus bytecode parsed (#443); CREA + LVLC dispatched
(#442/#448); PACK / QUST / DIAL / MESG / PERK / SPEL / MGEF fully parsed
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
M47.2 full scripting runtime (2026-10-12) ·
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

- [ ] **R6a-regress-23 — HEAD is 1–9% slower than `a3a7a43b0` under a same-machine
  control** (2026-10-22, `d9a201c71`; table under §Bench-of-record). Cornell, the
  content-identical scene, is +0.99 ms TAA. The SVGF bracket is +0.22–0.35 ms on
  every scene, equal to the new `gpu_shadow_filter` bracket inside it; `gpu_main`
  is +0.5–1.2 ms on Cornell, Whiterun and MedTek, unattributed (742-commit
  window: ReSTIR `c2647d468` / `17e0b7464` / `6110b6078`, GI sampling `0b56f1133`,
  SVGF history `1092b5b9e` / `24eb7dba8`, cloud-shadow map `c231f5b5f`). Next:
  bisect Cornell `gpu_main`. The record is 80 commits old at the Session 104
  close (6 on renderer paths: ground-cover wind `1ca5032ae`, `initialize_layouts`
  `4cd4bacbd` / `d87246b57`, `byte_view` `dedf6c0d6`, `core::paths` `b42f802fa`,
  LOD pool `021e1b824`); none was re-benched, so re-run before citing it.
- [ ] **Papyrus interpreter: what the frame budget does not bound** (#5764).
  #5763 gave the four exclusive `Update` systems a shared 10 000-statement
  frame allowance (runaway frame 13.6 → 1.4 ms; a never-ending handler dies at
  100 000 statements over ~10 frames). Left: a burst pays ~0.8 µs a delivery
  to be deferred (5 000 references: 6.9 ms in the first frame); a value-called
  function or a frame holding an array/struct local cannot suspend;
  `reference_script_event_system` still walks the attached scripts (2.8 ns
  each). Numbers: `docs/audits/BENCH_vm-cost_after-5763.txt`.
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
- [ ] **Fire lighting is on by default and never had its visual check.**
  `2325c1de` replaced `render/fire_lights.rs` (and its reach canary) with
  `append_combustion_surface_lights` from the transported field, which flipped
  the default as a side effect. Owed: an A/B on a vanilla torch interior (FNV
  Prospector, Skyrim BanneredMare). Derived reach should land near the
  ~512-unit vanilla LIGH radius, and fires without a companion LIGH should not
  blow out the room.
- [ ] **CHARAL runtime-dead surface.** The 2026-08-15 audit verified 62
  constants with zero mismatch at that HEAD; #5238 later falsified the
  then-current NPC Health row (fixed, and the capture's Health rows are now
  split player/NPC — #5342), so treat that "zero mismatch" as dated, not
  standing. FO3, FNV, Skyrim and FO4 reach actors. Still open:
  - The Oblivion ruleset is unwired (see Status).
  - `regen` no-ops until an Oblivion-shaped `PoolRegenConfig` exists.
  - `affliction_tick_system` is never registered.
  - FO76's capture is locked with no builder; Starfield waits on pending data.
- [ ] **MQ101 residue.** The quest plays end-to-end (live-verified
  2026-09-13), but the race menu auto-accepts because no interactive UI
  exists. `Fragment_11`'s `AddRaceSpells()` now lowers onto the player's
  RACE `SPLO` set (#4415, `9813af435`); no live MQ101 replay has re-checked
  it.
- [ ] **A scripted Enable()/Disable() on a resident reference takes effect on
  the next cell load only (#5065)** — the live consumer needs per-placement
  GPU/physics teardown + a re-spawn path; ledger/gate/interaction agree today.
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
  #5197); `ObjectInfo.Parent` inheritance resolves for settings (`5c14ebdf8`) and texture sets (#5502; glass-coverage signal still undecided), blend
  state is captured (#5277), emissive is parked (#5503). Owed: definition of done + a live Cydonia render.

- [ ] **Parsed-but-unconsumed minimal records** (TD6-2026-10-25-01, #6387).
  Fifteen `EsmIndex` maps built from EDID+FULL-only record types have no reader
  outside tests/examples (`dispatch_misc_stub.rs` is the deliberate parking
  lot), and each costs a `HashMap<u32, MinimalEsmRecord>` plus String allocs on
  every FNV/FO3/Oblivion load. The ones that are owed a consumer:
  - `birthsigns` (BSGN) — Oblivion birthsign abilities never reach the player
    ruleset.
  - `hunger_stages` / `dehydration_stages` / `sleep_deprivation_stages`
    (HUNG/DEHY/SLPD) — FNV Hardcore needs; `charal-fnv` has no consumer.
  - `recipe_records` / `recipe_categories` (RCPE/RCCT) — crafting.
  - `camera_shots` / `camera_paths` (CAMS/CPTH) — the load-screen MOD2 camera
    paths in open #6194.
  - `default_objects` (DOBJ) — engine default forms.

  No owner yet, so candidates to drop (the group then falls to
  `skipped_unconsumed_groups`, which already reports it): `animation_objects`
  (ANIO), `menu_icons` (MICN), `ammo_effects` (AMEF), `load_screen_types`
  (LSCT), `placeable_waters` (PWAT), `sigil_stones` (SGST). Re-derive the list
  with `grep -w <field>` outside `tests/`, `examples/`, `index.rs`, `parse.rs`
  and `dispatch_misc_stub.rs`.

### Infrastructure and tooling

- [ ] **Clippy `--all-targets` carries 20 lints** (2026-10-19): test/example
  targets across `scripting`, `plugin`, `pex`, `hkx`, `sfmaterial` and the bin. The lane is a
  non-blocking CI step (#5621); the blocking `cargo clippy --workspace -- -D
  warnings` form is clean.
- [ ] **Offline texture-set upscale finalization.** `tools/texture-upscale`
  works end-to-end: set discovery, TOML manifests, an external ESRGAN-family
  pass, companion-map upsampling and provenance. Remaining: per-game/per-role
  DDS compression and mip generation, BC5/BC7 decode, and material-slot-aware
  discovery.

---

## Project Stats

Ground-truth as of 2026-10-24, HEAD `fcd1cce4a`: all figures re-counted at the Session 104 close.

| Metric                                  | Value                        |
|-----------------------------------------|------------------------------|
| Rust source lines (`src/` dirs)         | 856 033                       |
| Rust total lines (all `.rs`, excl. `target/`) | 912 672                  |
| Source files (`.rs`, excl. `target/`)   | 1552 total · 1451 outside `tests/` dirs (+31 / +31 since the Session 103 close) |
| Workspace members                       | 35 (count the `[workspace] members` block only; 30 crates — `terrain-height` joined in Session 103 — + `byroredux` binary + 4 tools — `tools/nifskope` is on disk but not a member) |
| Tests                                   | **11 303 passing, 0 failing** (`cargo test --workspace --no-fail-fast` on rustc 1.96.0, 2026-10-24; 366 ignored; the lock-order detector is off in this count). Always pass `--no-fail-fast` for the ground-truth count — without it, `cargo test --workspace` stops after the first binary with a failure and silently omits every crate queued behind it (Session 77 saw 1836 vs the true 6905) — and beware shell pipes: `cargo test … | tail` reports the *pipe's* exit code. |
| Open issue directories                  | 6298 (`.claude/issues/`)     |
| NIFs in per-game integration sweeps     | **562 069** across eight games (Skyrim SE re-measured 2026-09-29 at 33 468 over 8 archives, #3712 added Animations; FO76's tail closed by #3461 and the 2026-10-03 re-measure (#4628) swept 102 980 over 5 of its 20 mesh-bearing archives; Skyrim LE 22 466 gated since fb8173fe0; earlier widenings #3369/#3466, Oblivion DLC #3925, Starfield #4440). Oblivion 9 612 · FO3 17 172 · FNV 20 746 · Skyrim SE 33 468 · Skyrim LE 22 466 · FO4 235 082 · FO76 102 980 · Starfield 120 543. |
| Per-game NIF clean-parse rate           | See the [compatibility matrix](#compatibility-matrix) — it is the single home for per-game parse rates, sweep dates and residual truncation tails. Summary only: 100% clean on all eight titles (Starfield re-measured 2026-09-24 #4440; FO76's `GeneratedMeshes` truncation tail closed by #3461 and re-measured 2026-10-03, #4628). |
| Supported archive formats               | BSA v103/v104/v105, BA2 v1/v2/v3/v7/v8 |

### Repro commands for every bench claim

> **CWD matters** — see the repro-command CWD note under §How to run a bench
> (and the `--game` CWD-immune alternative there). **Retired rows** (the
> single-scene R6a-stale-15 set, the sweetrow single-mesh figure) are in
> [`roadmap-history.md` §8](docs/archive/roadmap-history.md#8-retired-repro-table-rows).

| Claim                                                                     | Command                                                                                                                                                                                        |
|---------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Bench-of-record (LIVE)** — the command behind every FPS/ms figure in the live bench table above (§Bench-of-record, HEAD `d9a201c71`, 2026-10-22). Per-scene CWD applies; scene subset via `FSR_BENCH_SCENES`, output via `FSR_BENCH_OUT`. The default set is four interiors plus Cornell — no ground cover (R6a-groundcover-1). | `scripts/fsr-bench-matrix.sh 3 300` |
| **Papyrus interpreter cost census** — every µs/ms figure in the interpreter Known Issue (idle scan, `OnCellLoad` burst, polling, runaway cap) | `cargo test -p byroredux-scripting --release --lib -- --ignored --nocapture vm_cost` |
| **Scripted-scene A/B** — the +0.16 ms / +1.0% figure: BleakFallsBarrow01 from Skyrim `Data/`, 3 alternating runs with and without `--scripts-bsa` (the ~70 µs per-system figures come from the log patch recorded in the audit file) | `byroredux --esm Skyrim.esm --cell BleakFallsBarrow01 --bsa "Skyrim - Meshes0.bsa" --bsa "Skyrim - Meshes1.bsa" --textures-bsa "Skyrim - Textures0.bsa" … --upscaler taa --bench-frames 600 --bench-mode renderer-stepped --bench-camera orbit [--scripts-bsa "Skyrim - Misc.bsa"]` |
| **Same-machine control** (the method that makes a refresh interpretable — see **Standing methodology** under How to run a bench; used to prove PERF-REGRESSION-6c56e311 was code, and to prove R6a-stale-17's apparent regressions were not). Rebuild the prior record's commit in a worktree and bench it in the same session under the same load; the harness must be byte-identical at both commits (`diff` both scripts before trusting the comparison). | `git worktree add <dir> <prior-commit> && cd <dir> && cargo build --release && FSR_BENCH_SCENES="cornell prospector medtek" FSR_BENCH_OUT=<dir>/out <dir>/scripts/fsr-bench-matrix.sh 3 300` |
| **Upscaler SSIM quality matrix** — the SSIM / outlier-percentage figures quoted for every preset (Cornell Quality 0.9554 SSIM / 1.68% outliers, Performance 0.9199 / 5.39%; FO4 Dugout higher SSIM, worse outliers). Scores each preset against the native TAA render over five deterministic `--bench-camera` paths. Append `game` to score real game content instead of the redistributable Cornell scene. | `cargo test --release -p byroredux --test upscaler_quality -- --ignored --nocapture` |
| Megaton interior parse-side 929 REFRs (2026-04-19; test lives in the plugin **lib**, so a `--test parse_real_esm` filter runs 0 tests — #5070) | `cargo test -p byroredux-plugin --release --lib -- --ignored --exact esm::cell::tests::integration::parse_real_fo3_megaton_cell_baseline`                                                      |
| Per-game full mesh sweep (clean rates above; recoverable 100% gate)       | `cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`                                                                                                          |
| FO3 ESM parse floors, #3756 (index sum ≥ 44 000, measured 44 718 — an index sum that double-counts by design, not a record count; the file holds 718 952 records; placed refs ≥ 573 000; exterior cells ≥ 41 900) | `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored --exact parse_rate_fo3_esm`                                                                                       |
| FNV ESM parse floors (index sum ≥ 76 000, measured 78 575; same index-sum caveat as the FO3 row) | `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored --exact parse_rate_fnv_esm`                                                                                       |
| Dialogue greeting + force-greet census floors, #5367 (FNV: GREETING/HELLO topics exist, ≥438 greeting DIALs [parser-measured 438], ≥330 Dialogue-procedure PACKs with ≥260 PKDD topics [332/263]) — the source of the dialogue counts in M43's row | `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored dialogue_greeting` |
| Story Manager census floors, #5366 (Skyrim: 571 nodes / 24 event mnemonics / 448 SMQNs / 1191 SM-referenced quests / 441-of-443 sibling integrity, exact; FO4: 311 / 17 count floors) — the source of every SM count claim in M43's row and the live smoke's target chain | `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored story_manager` |

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
| `byroredux-audio`             | 3D spatial audio via kira 0.12 (spatial sub-tracks, reverb send, streaming music — M44)                          |
| `byroredux-boot-request`      | The launcher → engine handoff contract (`BootRequest`: what to play, not which flags express it)               |
| `byroredux-game-detect`       | Install detection and pre-launch validation                                                                    |
| `byroredux-settings-io`       | Persistent user settings — `(id, value)` pairs overlaid on the universal registry                              |
| `byroredux-save`              | M45 save/load — full-ECS-snapshot format, type-erased registry, atomic write + ring                            |
| `byroredux-hkx`               | Minimal safe Havok packfile reader (Skyrim LE/SE and FO4 skeletons, clips, and FO4 behavior graphs)             |
| `byroredux-fsr3-sys`          | Narrow Rust ABI for AMD FidelityFX FSR 3.1's Vulkan upscaler (no frame generation)                             |
| `byroredux-mod-runtime`       | Sandboxed executable-mod runtime — WebAssembly Components behind semantic host services                        |
| `byroredux-menuxml`           | Oblivion / FO3 / FNV legacy MenuXml UI — FOLD evaluator, layout, CPU raster (M48.4/M48.5)                      |
| `byroredux-pex`               | Compiled Papyrus `.pex` reader + decompiler (Skyrim / FO4 / FO76 / Starfield)                                  |
| `byroredux-sdk`               | Public tooling APIs — Studio document state, typed edit commands, actor-value snapshots                        |
| `byroredux` (binary)          | Game loop, cell loader, fly camera, animation system, render data collection, NIFAL translation boundary         |
| `tools/byro-dbg`              | Standalone debug CLI (TCP client, REPL)                                                                          |
| `tools/byro-detect`           | Headless install discovery + validation (the launcher's detection path with no window, GPU or engine)          |
| `tools/byro-launcher`         | Launcher UI (eframe/glow) — finds installed games, checks them, starts the engine                              |
| `tools/texture-upscale`       | Offline BSA/BA2 texture-set discovery and reference-guided semantic-map upscaling                                |
