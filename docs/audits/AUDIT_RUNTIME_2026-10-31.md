# Runtime Telemetry Audit — 2026-10-31

**HEAD**: `670f81917` (clean tree apart from the other audits' untracked reports; nothing was built from a modified tree). · **Baseline**: [AUDIT_RUNTIME_2026-10-27.md](AUDIT_RUNTIME_2026-10-27.md) (HEAD `fa6be4a6b`). · **Audited**: telemetry diff, harness/baseline integrity, and the playable-slice, milestone and M43 smoke matrix. · **Unchanged / skimmed**: none (the 10-27 report did not run most of the matrix, so there is no earlier measurement to skim against).

The audit ran alone, serially, with no sub-agents. Telemetry captures went through `capture.sh` (release build, `renderer-static`, `xvfb-run`). Before every capture `pgrep -x byroredux` was empty and port 9876 was free. Every capture matched `mode=renderer-static`, the exact pose rows, `upscaler=fsr3/quality` (render 853x480, output 1280x720 — the engine default), and an entities cross-check (`stats` vs `bench:`) with delta 0.

**Environment note.** The playable-slice and milestone gates were launched with their default display handling, so they opened windows on the user's Wayland session (`README` §Headless runs documents this). That is how `p1-character-traversal` is meant to run (it must run on a display). Every red non-P1 gate below was re-run under `env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE xvfb-run -a` and failed identically, except where stated.

## Headline

1. **All five telemetry baselines PASS.** Draw rows, pose rows, lights, skin, tex/mesh rows are byte-identical to the 2026-10-28 regen (#6498/#6499 landed and hold). `entities_total` is 0.2 to 0.85 % below baseline on four games and +0.1 % on Oblivion, all inside the ±2 % band.
2. **A GPU device loss on a core playable path (RT-01, HIGH).** On a display-attached GPU, Skyrim's exterior to interior door transition (Whiterun to Bannered Mare, second visit) kills the Vulkan device about 5 s after the transition starts. The kernel logs `NVRM: Xid 109 CTX SWITCH TIMEOUT`. It reproduced in 3 of 4 `p1-character-traversal` runs. The 10-27 audit could not see it because it ran P1 headless.
3. **Eight declared gates are red for reasons that are not telemetry.** Three are stale harnesses (HUD smokes blocked by message boxes, `p6-loading-model`'s log filter, `m47-2-bleedout`'s hard-coded 30). Two are live behavioural gates (`m47-3-obscript` NukaCola, `m-exteriors cycle`). Findings below.
4. **Already known, not re-filed**: #6506 (every engine exit logs the allocator-leak ERROR; the debug-build `interior-godrays` panics on it), #6493 (per-frame moved-surface WARN spam, 4,155 lines in one 60 s Skyrim run), #6500 (`m-exteriors fo3 boundary`, identical numbers to 10-27), REG-2026-10-31-04 (nine M43 smoke scripts in no contract loop; `check-playable-smoke-contracts.sh` still FAILs on `m43-armor.sh`), FO3/FNV mine and EXPL findings (not reached by any run here).
5. **Fixed since 10-27, verified live**: #6501 (`p5-f5-f9-quicksave fo3` now PASS), #6502 (the lock-order call-site now names `condition/eval/global.rs:81`), #6503 (no orphan Xvfb from `capture.sh`; the survivor sweep stayed silent), #6498/#6499 (baselines refreshed).

## Per-game telemetry table

| Game | Cell | Status | Δ vs baseline (gating first; advisory last) |
|------|------|--------|---------------------------------------------|
| fnv | FreesideAtomicWrangler | PASS | entities 6985→6926 (−0.84 %); tex_missing_base_color 1→1 (`textures/grey.bmp`); mesh 0→0; lights 30 pt / 0 spot / 0 dir exact; skin max/overflow 1364/0; draws 2069/503b/118c raster 1284 exact (raster < 3000); advisory: skin_live 106→106, fps 141.5→140.7, frame p50/p95/max 7.18/9.06/63.11 ms, all_slots 1→1 |
| fo3 | MegatonPlayerHouse | PASS | entities 3626→3602 (−0.66 %); tex/mesh 0/0; lights 11/0/0 exact; skin 1364/0; draws 1515/516b/62c raster 1289 exact; advisory: skin_live 3→3, fps 84.3→86.5, frame 11.53/12.55/57.87 ms, all_slots 0→0 |
| oblivion | ICMarketDistrictTheGildedCarafe | PASS | entities 969→970 (+0.1 %); tex/mesh 0/0; lights 8/0/0 exact; skin 1364/0; draws 336/18b/2c raster 20 exact; advisory: skin_live 10→10, fps 171.3→174.4, frame 5.98/7.00/57.09 ms |
| skyrim_se | WhiterunDragonsreach | PASS | entities 9809→9775 (−0.35 %); tex_missing_base_color 0→0; mesh 0/0; lights 28/0/0 exact; skin 1364/0; draws 2495/643b/34c raster 1786 exact; advisory: skin_live 123→123, fps 160.2→162.9, frame 5.13/17.30/65.95 ms, all_slots 2→2 (unique-path count; the control-char `\bnor` path plus `ore_ebony_e.dds`) |
| fo4 | InstituteBioScience | PASS | entities 14061→14028 (−0.23 %); tex_missing_base_color 1→1; mesh 0/0; lights 680/5/0 exact; skin 1364/0; draws 2471/710b/110c raster 2247 exact (below the 3000 `DRAW_SORT_PARALLEL_THRESHOLD`, as recorded by the 10-28 regen); advisory: skin_live 258→258, fps 62.3→63.1, frame 14.56/15.73/168.03 ms, all_slots 2→2 |
| starfield | citycydoniamainlevel | NOT RUN | no baseline; the last two attempts were safety-aborted under memory pressure and ~13 GB RAM was available. Unverified, neither known-good nor known-stalled |

No metric improved, so there is nothing to tighten via `--regen`. The four-game entity drop (−59 / −24 / −33 / −34) is uniform and benign. It stays unattributed because it is inside the band, so it is not a finding.

## Gate matrix

`P` = pass, `F` = fail (finding), `SKIP` = exit 77, `cfg` = exit 2 (the fixture declares no route for the gate; not a pass), `—` = not applicable.

| Gate | fnv | fo3 | oblivion | skyrim_se | fo4 |
|------|-----|-----|----------|-----------|-----|
| Telemetry diff | P | P | P | P | P |
| `m-exteriors` static | P | P | P | P | P |
| `m-exteriors` boundary | P | **F** (#6500, identical to 10-27) | P | P | P (documented #3306 crack only) |
| `m-exteriors` cycle | **F** (RT-05) | **F** (RT-05) | P | **F** (RT-05) | P |
| `m-exteriors` water | P | P | P | P | P |
| `p0-door-interaction` | P | P | P | P | P |
| `p1-character-traversal` | P | — | — | **F** (RT-01) | — |
| `p2-melee-core` | P | P | — | P | F ('starts clean' wait; documented in the skill as pre-existing, not a regression) |
| `p3-hud` | P | cfg | cfg | cfg (RT-07) | cfg |
| `p3-player-body` | P | cfg | cfg | cfg (RT-07) | cfg |
| `p4-quest-route` / `p5-quest-persistence` (MS01) | — | — | — | P / P | — |
| `p4-fnv-quest-route` / `p5-fnv-quest-persistence` | P / P | — | — | — | — |
| `p5-save-restart` | P | P | — | cfg (not declared) | — |
| `p5-f5-f9-quicksave` | P | P (was FAIL on 10-27; #6501) | — | — | — |
| `p5-door-transition` | P | — | — | — | — |
| `p5-soak` | P (3-minute run via `BYROREDUX_SOAK_MINUTES=3`; the 30-minute default was not run) | — | — | — | — |
| `p6-loading-model` | — | — | — | **F** (RT-06) | cfg (RT-07) |
| `w1-water-traversal` | P | — | — | P | — |
| `w2-dynamic-water` | P | — | — | — | — |

| Other gate | Result |
|------------|--------|
| `capture.sh --self-test` | PASS |
| Baseline schema tests (`cargo test -p byroredux --bin byroredux bench::`) | PASS, 10 of 10 |
| Golden frames (`golden_frames -- --ignored`) | PASS ×2 on re-run (cube, combustion lab). The first run's combustion lab hung (RT-10) |
| `scripts/check-playable-smoke-contracts.sh` | **FAIL** (`m43-armor.sh` in no loop / `UNCHECKED_SMOKE_GATES`): dup of REG-2026-10-31-04 |
| `scripts/check-byro-dbg-harness-contracts.sh` | PASS (78 scripts) |
| Real-data pins (`scripts/run-real-data-pins.sh`, one process per pin, RAM-guarded) | 62 pass, 0 fail, 4 NOT RUN (the Starfield pins, skipped for memory: `parse_real_starfield_dialogues_and_scenes_populated`, `installed_starfield_small_masters_do_not_collide`, `installed_starfield_npcs_and_races_decode_acbs_and_skeletons`, `starfield_cydonia_pack_ins_instance_their_template_cells`) |
| `m41-equip` / `m41-ragdoll` / `m43-quest-runtime` / `m47-triggers` / `m-trees` | P (data present, so the exit-0-SKIP caveat did not apply) |
| `m42-eat-sleep` / `m42-package-procedures` / `vats-targeting` / `sm1` / `dt1` / `dt2` / `fo4-ragdoll` / `tsd1-height-census` | P |
| `m43-*` live gates (adia, item-drop, crafting, tempering, armor, skyrim-actions, mines, attraction, attraction-reaction, node-window, fo4-actions) | P |
| `m43-hello` | P headless (29 s). On the Wayland display it ran 601 s without a hello. Environment (window frame pacing), not engine; see RT-10's note |
| `m47-2-bleedout` | **F** (RT-04) |
| `m47-3-obscript` | **F** (RT-03) |
| `m48-menu-load` / `m48-5-fo3-hud` / `m48-7-fo4-hud` | P |
| `m48-4-oblivion-hud` / `m48-5-fnv-hud` / `m48-6-skyrim-hud` | **F** (RT-02) |
| `m34-day-night` | **F** (RT-09) |
| `interior-godrays` | **F**: the debug build panics at `teardown.rs:496` (existing #6506); the open and sealed captures themselves were written |
| Cornell RT oracle, `check-bench-determinism.sh`, `material-provider-matrix.sh`, `fsr-bench-matrix.sh` | NOT RUN (out of the time budget; not folded into a pass) |

## Findings

### RT-2026-10-31-01: Skyrim exterior to interior door transition loses the Vulkan device (NVRM Xid 109 CTX SWITCH TIMEOUT)
- **Severity**: HIGH (the engine exits, with the playable path's second Bannered Mare entry as the trigger)
- **Dimension**: Gate matrix / renderer runtime
- **Location**: gate `docs/smoke-tests/p1-character-traversal.sh` (skyrim_se); path `byroredux/src/cell_loader/transition.rs` → first draw of the destination cell (`app_frame` "Draw failed")
- **Status**: NEW
- **Game / Cell**: skyrim_se / Whiterun exterior `(6,-2)` to `WhiterunBanneredMare` (the route's second door; the first visit, interior to exterior, is fine and is what `p0` covers)
- **Baseline**: none (the 10-27 audit ran P1 headless and reported it NOT RUN)
- **Current**: 3 of 4 display runs end with the same lines. The 4th run, with `BYRO_VALIDATION=1`, timed out earlier at "door prompt recovered after walking", so it never reached the door.
  ```
  loading.screen: begin LSCR=0001DD5D owner=Door
  Transition: source REFR 00000000 → interior 'WhiterunBanneredMare' …
  (5 s later) ERROR app_frame: Draw failed: wait_for_fences: The logical device has been lost
  ```
  Kernel log: `NVRM: Xid (PCI:0000:01:00): 109 … name=byroredux, channel 0x00000039, errorString CTX SWITCH TIMEOUT` at 17:26:55, 18:24:22 and 18:41:45 (this audit's three runs), with an identical `Info` word on two of them. The same Xid 109 fired three times in 90 s at 00:31 to 00:33 the same day, so it predates this audit. `Xid 56` display-engine errors also appear.
- **Description**: The second transition logs `Transition: source REFR … → interior …` but never reaches `Transition: unloading prior exterior` or `Cell transition applied`. The main thread stalls about 5 s, then the first `wait_for_fences` returns `DEVICE_LOST`. A GPU job that runs past the driver's context-switch timeout is the proximate cause. Which pass it is, and whether it is driver-side or a malformed dispatch, is not established. I did not bisect, since each release build takes minutes and the gate is display-only. Skyrim has no other exterior to interior round trip: `p0` is one-way, `p5-door-transition` is not declared for Skyrim, and `m-exteriors` has no door leg. FNV's `p5-door-transition` (both directions) passes.
- **Evidence**: `/tmp/audit/runtime/gates/p1-skyrim_se.log`, `r-p1-skyrim_se-b.log`, `/tmp/audit/runtime/p1_c.log` (engine.stderr kept by `BYROREDUX_KEEP_ARTIFACTS=1`), `journalctl -k | grep Xid`. `nvidia-smi`: RTX 4070 Ti, driver 580.178.04.
- **Impact**: The engine dies on a normal Skyrim door transition; save-guard, streaming and loading-screen work all sit behind it. It cannot be found headless.
- **Related**: #6506 (the shutdown leak ERROR that follows any device loss is the same teardown path). `/audit-renderer` and `/audit-concurrency` (queue/AS sync) should look at the exterior-unload to interior-load GPU work (BLAS/TLAS eviction and rebuild, SVGF history reset).
- **Suggested Fix**: Capture one run under `BYRO_VALIDATION=1` with the slower pacing kept out of the route (drive the second door through a `byro-dbg` command instead of frame-counted input), then bisect with `p1` on a display. Add a Skyrim `p5-door-transition` route so the round trip has a headless-capable gate.

### RT-2026-10-31-02: Three of five HUD smokes are red: an authored MessageBox opens over the HUD and the harness never answers it
- **Severity**: MEDIUM (declared gates red; they cannot catch a HUD regression)
- **Dimension**: Gate matrix / harness integrity
- **Location**: `docs/smoke-tests/m48-4-oblivion-hud.sh`, `m48-5-fnv-hud.sh`, `m48-6-skyrim-hud.sh` (pixel gates after the `pin_and_shoot` captures)
- **Status**: NEW (not in the 10-27 report, which did not run them)
- **Game / Cell**: oblivion / AbandonedMine, fnv (HUD smoke cell), skyrim
- **Baseline**: PASS expected
- **Current**: `m48-4`: "full-bars health fill missing (0 px run)". `m48-5-fnv`: "full HP tick band missing (0 white cols)". `m48-6`: "world too animated between captures (4093 vs 3459 changed px)". `m48-5-fo3` and `m48-7-fo4` PASS.
- **Description**: The captured `full.png` shows a modal dialog over the scene: Oblivion's "You dream of long days spent basking in the sunlight of your native lands…" (Done) and FNV's "Happy Trails Expedition" (Ok). The modal's backdrop dims the HUD strip, so the bar/tick colour runs the gates measure are not there. These boxes come from the ObScript quest runtime (M47.3, 10-18 and later), which now runs vanilla start-up message boxes. Every other live gate that meets them calls `smoke_answer_message_boxes` (`ui.answer`); the three HUD scripts do not, and they run their own launch lines instead of `lib/fixture.sh`. The Skyrim failure is the same family (world animated between captures); I did not open its image. Reproduced headless, with the screenshots inspected for Oblivion and FNV.
- **Evidence**: `/tmp/tmp.m2uevM9RDc/full.png` (Oblivion), `/tmp/tmp.UpOPtREaPT/full.png` (FNV; both deleted with this run's scratch cleanup, rerun with `BYRO_SMOKE_KEEP=1` to regenerate).
- **Impact**: Three of the five per-game HUD gates cannot discriminate. The CI contract loop only proves SKIP semantics, so nothing in CI notices.
- **Related**: *feedback_headless_capture_harness* (Oblivion message box needs a click), the P3/P4 gates that already answer boxes.
- **Suggested Fix**: Answer or suppress the start-up box before the first capture (`ui.answer` as `p3-hud` does), or pin the HUD captures with the modal up and sample outside its backdrop.

### RT-2026-10-31-03: `m47-3-obscript` red: `inv.use` of NukaCola is refused, so its ScriptEffectStart never runs
- **Severity**: MEDIUM (a live behavioural gate for the M47.3 chem path is red; not attributed)
- **Dimension**: Gate matrix / gameplay runtime
- **Location**: gate `docs/smoke-tests/m47-3-obscript.sh:169-177`; consumer `byroredux/src/inventory/actions.rs:88` (`consume_item`), warning at `byroredux/src/inventory/mod.rs:183`
- **Status**: NEW
- **Game / Cell**: fnv / GSProspectorSaloonInterior (`--player`)
- **Baseline**: PASS when the gate landed (`cd8744718`, 2026-10-18)
- **Current**: `inv.add 0x0001519E 1` succeeds, `inv.use 0x0001519E` reports "queued use of row N", then the engine logs `WARN byroredux::inventory: queued inventory action was unavailable for the current player/item`. No `Used …` notification, no `ObScript effects`. Every earlier leg of the gate (OnLoad, OnTriggerEnter, OnDeath, Resurrect, getters, item OnAdd) PASSes. Reproduced by hand on a clean engine.
- **Description**: `consume_item` returns `Unavailable` on a long list of preconditions: `Dead`, equipped row, no `InventoryCatalog` entry or no effects, `ActorVitals`/`ActorValues` missing, a non-finite health, an effect actor value the player's `ActorValues` lacks, a stale `ItemInstancePool` slot. I did not isolate which one fails for NukaCola. The consume path changed in M43's crafting / improved-items work and in `258c6d9b2` (#6356 split), after the gate was green.
- **Evidence**: `/tmp/byro-m47-3.tGgDpB/{use.log,effects.log,session.stderr}` and `/tmp/audit/runtime/nuka/err.log`.
- **Impact**: A player using a chem or food with a script effect gets no feedback and no effect, and the item stays in the inventory (the refusal happens before the count drops).
- **Suggested Fix**: Log the refusing precondition in `consume_item` (the current warning does not name it), then fix the one that fires.

### RT-2026-10-31-04: `m47-2-bleedout` red: an essential actor recovers at 57 health, the gate pins 30
- **Severity**: MEDIUM (declared gate red; either the gate or the Skyrim NPC health composition is stale)
- **Dimension**: Gate matrix
- **Location**: `docs/smoke-tests/m47-2-bleedout.sh:114`; `crates/scripting/src/bleedout.rs:244-285` (`target = health_regain * undamaged`)
- **Status**: NEW
- **Game / Cell**: skyrim_se / RiverwoodSleepingGiantInn (Delphine)
- **Baseline**: PASS when the gate landed (`dd843f9e6`)
- **Current**: "Delphine recovered at 57 health, not 30" (every other leg passes: she enters bleedout, does not die, gets back up).
- **Description**: The engine recovers at `fEssentialHealthPercentReGain` (0.3) of the undamaged Health. 57 = 0.3 × 190, so Delphine's composed Health is now 190 where the gate assumes 100. The Skyrim pool now carries a class/level term (#5914/#5915, documented by `27f022dfd`). The gate was never re-derived. Which side is right (190 is plausible for the composed value) is not established here.
- **Evidence**: `/tmp/audit/runtime/gates/m47-2-bleedout.log`, `h-m47-2-bleedout.log` (same under Xvfb).
- **Suggested Fix**: Read the expected value from the running engine (`GetActorValue` undamaged × the GMST) instead of the literal 30.

### RT-2026-10-31-05: `m-exteriors cycle` red on fnv, fo3 and skyrim: the `composite_term` sun-cycle pixel gate cannot see noon vs night
- **Severity**: MEDIUM (declared gate red on three of five games)
- **Dimension**: Gate matrix / exterior rendering
- **Location**: `docs/smoke-tests/m-exteriors.sh:657-669` (the #4491 pixel invariant), fixtures' cycle poses
- **Status**: NEW (the pixel invariant is the #4491 addition; the 10-27 report did not run `cycle`)
- **Baseline**: n/a
- **Current**: fnv noon 0.2770 / night 0.2523 / sd 0.0388 (delta 0.0247 passes, but sd < 0.039); fo3 noon 0.2557 / night 0.2529 / sd 0.0293; skyrim noon 0.2643 / night 0.2530 / sd 0.0280. Oblivion and FO4 PASS. Identical under Xvfb.
- **Description**: The `fo3` `noon.png` frame is a close-up of a near-black wall with a door prompt; `night.png` is black. The pose faces a wall, so `composite_term`'s mean cannot reflect the sky or the sun. Either the fixture pose is wrong for this invariant, or the sun/sky is not reaching `composite_term` on these games. The CPU-side sun values (`sun: intensity=…`) do change; the day phase is selectable (`m34-day-night` passes its first six legs).
- **Evidence**: `/tmp/byro-exterior-smoke.61gqnD/fo3/{noon,night}.png` and `*-composite-term.png`; `summary.tsv`.
- **Related**: AUDIT_EXTERIOR_2026-09-19 EXT-D7-04 (the gate that was meant to add this check).
- **Suggested Fix**: Look at the three `noon-composite-term.png` captures. If they show a wall, re-aim the cycle pose upward; if they show sky and the term is flat, route it to `/audit-exterior` as a sky-assembly defect.

### RT-2026-10-31-06: `p6-loading-model skyrim_se` can never pass: its `RUST_LOG` omits `byroredux::app_step`, and its "artifacts" path is deleted on exit
- **Severity**: MEDIUM (a declared Skyrim gate is red by construction)
- **Dimension**: Harness integrity
- **Location**: `docs/smoke-tests/p6-loading-model.sh:93` (`RUST_LOG`), `:177` (the wait), `:38-47` and `:51-52` (`fail()` / `cleanup`)
- **Status**: NEW
- **Game / Cell**: skyrim_se
- **Baseline**: PASS expected
- **Current**: Gates 1 to 6 PASS (census, cover pick, art resolves, cover began from a real LSCR, presented before teardown, cover frame 89,345 B). Then `FAIL -- skyrim_se: timeout waiting for the authored transition` after 300 s. A kept re-run shows the transition did apply (`Transition: … → exterior 'whiterunworld' (6,-2)`, `loading.screen: dismissed after destination frame` 5 s later).
- **Description**: The wait greps the engine log for `Cell transition applied:` containing `P0_APPLIED_LOG`. That line is `log::info!` in `byroredux/src/app_step.rs:1352`, target `byroredux::app_step`, but the gate sets `RUST_LOG="warn,byroredux::loading_screen=info,byroredux::cell_loader::transition=info,byroredux::interaction=info"`, so it is filtered out (p0 and p1 include `app_step=info`). Separately, `fail()` prints `artifacts: $LOG_DIR` and the `EXIT` trap then `rm -rf`s it, so the path in the failure message never exists; two runs left nothing to inspect.
- **Evidence**: `/tmp/audit/runtime/p6_keep.log` and the kept engine log.
- **Suggested Fix**: Add `byroredux::app_step=info` to the filter; honour `BYROREDUX_KEEP_ARTIFACTS` in the trap as `p3-player-body` does.

### RT-2026-10-31-07: Skyrim's P3 gates are unreachable (exit 2) although the fixture and every doc call them Skyrim's default
- **Severity**: LOW
- **Dimension**: Harness integrity / doc-rot
- **Location**: `docs/smoke-tests/fixtures/skyrim_se.env` (`FIXTURE_GATES`), `fixtures/fo4.env`, `docs/smoke-tests/README.md:37,50`, the skill's gate table
- **Status**: NEW (regression from #5674, `19b6fa93d`, which introduced the explicit list)
- **Description**: `skyrim_se.env` still carries a full `P3_*` block (cell, quest 0x18B4B, vital colours) but `FIXTURE_GATES=(p0-door-interaction p1-character-traversal p2-melee-core p6-loading-model w1-water-traversal)` omits `p3-hud` and `p3-player-body`. `p3-player-body.sh` with no argument (documented as "Skyrim SE: player body attach + view toggle") exits 2 "no measured route for this gate". The skill says "the two P3 gates are Skyrim-only today" and that `p6-loading-model` covers "Skyrim + FO4"; the FO4 fixture declares only `p0 p2`, so `p6-loading-model.sh fo4` exits 2.
- **Evidence**: `/tmp/audit/runtime/gates/p3-player-body-skyrim_se.log`, `p6-loading-fo4.log`.
- **Suggested Fix**: Either add `p3-hud p3-player-body` (and, if measured, the `p5-*` gates) to the Skyrim list, or delete the dead `P3_*` block and correct the README and skill.

### RT-2026-10-31-08: A recursive `QuestStageState` read is live on FNV and FO4 cell load, and now locatable
- **Severity**: MEDIUM (the hazard is HIGH-class: the std `RwLock` is writer-preferring on Linux, so a recursive read deadlocks behind a parked writer)
- **Dimension**: ECS runtime hazards (owner `/audit-ecs`, `/audit-scripting`)
- **Location**: second read at `crates/scripting/src/condition/eval/global.rs:81` (`get_stage_done`, `world.try_resource::<QuestStageState>()`); the first guard's holder is not named by the tracker
- **Status**: Existing hazard, open (10-27 RT-6 filed only the locator defect, now fixed by #6502; the hazard itself has no open issue). Related closed #6008.
- **Game / Cell**: fnv / GSProspectorSaloonInterior (seen in the `m47-3` gate and by hand), fo4 / InstituteBioScience (seen in the telemetry capture log at 20:11:39Z)
- **Description**: `lock_tracker` reports "a second `QuestStageState` read guard is live on this thread, acquired at `condition/eval/global.rs:81:29`". The call site is correct now, but the outer holder is still unidentified: it is some caller that keeps a `QuestStageState` read guard while evaluating a CTDA / ObScript condition (the 10-27 report listed SCEN phase transitions on FO4 and the first game-loop frame on FNV). The tracker warns once per type per thread, so only the first instance is ever visible.
- **Evidence**: `/tmp/audit/runtime/fo4-InstituteBioScience.engine.log`, `/tmp/audit/runtime/nuka/err.log`.
- **Suggested Fix**: Find the enclosing guard with `RUST_BACKTRACE`-style capture in the tracker's warn path (print `Backtrace::capture()` for the first instance), then hand the evaluator a snapshot or drop the outer guard before evaluating.

### RT-2026-10-31-09: `m34-day-night`'s multi-day assertion is wall-clock sensitive and fails at today's frame cost
- **Severity**: LOW
- **Dimension**: Harness integrity
- **Location**: `docs/smoke-tests/m34-day-night.sh:89-92,129`
- **Status**: NEW
- **Current**: expects `time.advance: day=2 hour=0.000 clock=00:00 phase=night`, observes `hour=0.001`. The script resumes the clock at 120× for the gap between `time.resume` and the next `time.pause` (about 30 ms, which at 120× is 3.6 game-seconds, so 23.001 h), then `time.advance 25` lands at 0.001. Every other leg (sunrise, noon, night, pause/resume) passes.
- **Suggested Fix**: Compare `hour` within 0.01, or pause first and `time.set 23:00` again before `time.advance`.

### RT-2026-10-31-10: First golden-frame run: the combustion-lab engine hung 7+ minutes at 100 % CPU after the teardown ERROR (not reproduced)
- **Severity**: LOW (unreproduced)
- **Dimension**: Gate matrix
- **Location**: `byroredux/tests/golden_frames.rs` (`combustion_lab_golden_frame`), teardown `crates/renderer/src/vulkan/context/teardown.rs:~470`
- **Status**: NEW observation
- **Description**: In the first `golden_frames -- --ignored` run the combustion-lab `byroredux --bench-frames 60 …` process wrote its screenshot and the `bench:` line, logged `GPU allocator has 3 outstanding references — leaking allocator + device …` (#6506), then stayed alive at 100 % CPU (one thread running, the others parked on futexes) until the 590 s `timeout` killed `cargo`. Two later runs of the same test finished in 19 to 22 s. The cube case in the same first run exited normally. The process leaves the device alive by design after the #6506 path, so a render or present thread spinning against it is plausible.
- **Suggested Fix**: Fix #6506; if a hang remains, add a join timeout and a thread dump on shutdown.

### RT-2026-10-31-11: `help` (and any read-reentrant command) trips the recursive-read WARN even though the contract permits it
- **Severity**: LOW
- **Dimension**: Diagnostics noise
- **Location**: `byroredux/src/commands/world_info.rs:22` (`HelpCommand::execute` re-reads `CommandRegistry`, documented as permitted by CONC-D3-04 / #1786), `crates/core/src/ecs/lock_tracker.rs`
- **Status**: NEW
- **Description**: Running `help` over `byro-dbg` logs `ECS recursive-read hazard: … CommandRegistry … acquired at world_info.rs:22:30`. The source comment says read-read re-entry is permitted by the tracker. Because the tracker warns once per type per thread, a benign instance can mask the first genuine one on the same thread (RT-2026-10-31-08 is that kind of case).
- **Suggested Fix**: Allow-list `CommandRegistry` in the tracker, or pass the registry down into `execute`.

## Existing issues and prior findings observed live (not re-filed)

- **#6506** (HIGH, `SceneBuffers::destroy` misses `moved_surface_buffers`): every engine exit in this audit logged `GPU allocator has N outstanding references — leaking allocator + device …` plus two `GpuBuffer dropped without destroy()`. The debug-build `interior-godrays.sh` panics on it (`teardown.rs:496`). This is the only reason that gate reads red.
- **#6493** (`moved-surface set over capacity … (#6250)` warns every frame): 4,155 lines in one 60 s Skyrim exterior run (Whiterun, 78 to 106 movers vs the cap of 64).
- **#6500** (`m-exteriors fo3 boundary`): `pairs_checked=6 pairs_dirty=5 height_mismatch_vertices=160` on MegatonWorld, the same numbers as 10-27. FO4's `pairs_dirty=1 height_mismatch_vertices=3` still passes as the documented vanilla #3306 crack.
- **REG-2026-10-31-04** (AUDIT_REGRESSION_2026-10-31): `check-playable-smoke-contracts.sh` fails on `m43-armor.sh` (and eight sibling M43 gates). All nine of these gates PASS live here.
- **FNV-2026-10-31-04**: `m41-ragdoll`, `p5-door-transition`, `p5-f5-f9-quicksave` and `p5-soak` sit in `UNCHECKED_SMOKE_GATES`. All four PASS live on FNV.
- **p2-melee-core fo4**: the 'starts clean' wait failed as documented in the skill (NPCs fighting at load); not a regression.
- **Mines / EXPL** (FNV-2026-10-31-01, FO3-2026-10-31-02, ESM-2026-10-31-01/02), **city worldspaces** (SKY-2026-10-31-D4-01) and **VFX colliders** (OBL-D3-2026-10-31-01): not reachable by any gate that ran, so there is no live confirmation. `m43-mines` runs on FO4 only.

## Harness and baseline integrity

- `git log --since=2026-10-27` over the harness paths shows #6498/#6499 (baseline refresh), #6503 (`capture.sh` Xvfb sweep), #6501 (`p5-f5-f9` pose), #6461 (contract scan, since extended by nine M43 gates that were not added to it), #6458 (FO4 seam exemption), and the M43 gate commits `834e83822`, `93428cba1`, `7c9414845`, `4dd6197dc`.
- The newest `# regenerated:` block of every TSV says 2026-10-28 (#6498), one run, one engine. Nothing moved without a reason. `bench_mode`, camera pose and the draw-split invariant (`batches <= raster <= cmds`, `gpu_calls <= 2 × batches`) hold in all five.
- `REQUIRED_METRICS`, the TSV keys and the skill's Phase 3 table agree; the four baseline schema tests pass.
- Gate-orphan observation (not filed): the three HUD smokes kill the `xvfb-run` wrapper (`kill $ENGINE`), which leaves the engine and Xvfb alive. Two such engines (and their Xvfb servers) survived from my last HUD runs and were killed by PID. Same class as the #3560 teardown trap `capture.sh` closes.
- Skill doc-rot: the gate table's "the two P3 gates are Skyrim-only today" and "`p6-loading-model` (Skyrim + FO4)" are not true of the fixtures (RT-2026-10-31-07).

## Process notes

- Telemetry captures and `--self-test`: 0 survivors (`pgrep -x byroredux; pgrep -x Xvfb; pgrep -x byro-dbg`) after each game.
- The final sweep found no engine, Xvfb or `byro-dbg` process of mine left running.
- Build: only the repo `target/` with `CARGO_INCREMENTAL=0` (release and debug for the HUD smokes; no private target dir, no probe crate).
- Suggested next step: `/audit-publish docs/audits/AUDIT_RUNTIME_2026-10-31.md` (RT-2026-10-31-01 first).
