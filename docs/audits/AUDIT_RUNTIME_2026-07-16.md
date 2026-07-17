# Runtime Telemetry Audit — 2026-07-16

`/audit-runtime --game all`, audit #21 of 21 in a `comprehensive` sweep against
HEAD `c3e09bb5`.

## Setup

| Check | Result |
|-------|--------|
| Headless X | PRESENT — `/usr/bin/xvfb-run`, `-screen 0 1280x720x24` |
| Game data dirs | PRESENT — Oblivion, Fallout 3, Fallout NV, Skyrim SE, Fallout 4, Fallout 76, Starfield all resolve under `/mnt/data/SteamLibrary/steamapps/common` |
| Binaries | Rebuilt clean: `cargo build --release -p byroredux -p byro-dbg` (10.1 s) |
| Baselines | 5 committed TSVs (fnv, fo3, oblivion, skyrim_se, fo4) under `.claude/audit-baselines/runtime/` |
| Fallout 76 | No `[profiles.fo76]` entry in `assets/debug_profiles.toml` — `--game all` has no key to expand for it, unchanged from every prior sweep |
| Starfield | SKIPPED — `[profiles.starfield]` ships empty `default_bsas`/`default_textures_bsas`/`default_materials_bsas` and `sample_cells = []`; no cell baseline exists (SF coverage is `--sf-smoke`, out of scope here) |
| Dedup source | `gh issue list --repo matiaszanolli/ByroRedux --limit 200` → 28 issues fetched to `/tmp/audit/issues.json` (targeted `gh issue view`/`gh issue list --search` follow-ups for specific numbers referenced by prior runtime reports) |

Five games have committed baselines and were diffed, run **serially** per the
SKILL's port-collision warning (RT-1 / #1619).

**Methodology note (self-caught mid-run):** the first FO3 capture hit exactly
the RT-1/#1619 collision the SKILL warns about — the FNV engine process
launched under `xvfb-run` outlived the `kill -INT <wrapper-pid>` teardown
(the wrapper PID isn't always the actual engine PID), so `byro-dbg`'s FO3
capture silently read stale FNV telemetry (`Entities: 9352`, FNV's own
`grey.bmp` missing-texture / `dlcpitt` mesh-cache-failure list) while FO3's
own `bench:` line correctly showed `entities=3311`. Caught by cross-checking
the `byro-dbg` entity count against the `bench:` line before trusting either;
all stray `byroredux`/`Xvfb` processes were killed, the driver script was
fixed to resolve and kill the actual engine PID via `pgrep -f "byroredux
--game $GAME --cell $CELL"` (not just the `xvfb-run` wrapper), and FO3 (plus
every subsequent game) was captured clean against a verified-free port 9876.

**Determinism note:** captured with `BYROREDUX_FIXED_DT=0.016666` (60 Hz
fixed step), not `BYROREDUX_FIXED_DT=0`. The SKILL's `=0` suggestion freezes
`TotalTime` entirely, which starves `crosses_one_second_boundary` (used by
both the once-per-wall-second `engine::stats` log line and the `skin=L/M+S`
telemetry it carries) — with `=0` the `skin=` line never fires at all. `60`
Hz keeps determinism (fixed dt every frame) while letting `TotalTime`
actually advance, so `skin_pool_live/max/overflow` are still capturable.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| fnv | FreesideAtomicWrangler | PASS | entities 9250→9352 (+1.1%, in ±2% band, same magnitude as the RT-3/#1705 precedent); tex 1→1; mesh 11→11; skin 686/1364+0 (exact); draws 2722→2921 cmds (+7.3%, ≤×1.1); `wall_fps` 141.4→79.0 (advisory) |
| fo3 | MegatonPlayerHouse | PASS | entities 3311→3311 (exact); tex 0→0; mesh 3→3; skin 0/1364+0 (exact); draws 1839→1839 cmds (exact), 96→98 batches (≤×1.1), 9→8 gpu_calls; `wall_fps` 93.3→143.0 (advisory) |
| oblivion | ICMarketDistrictTheGildedCarafe | PASS (structural) / 1 HIGH (physics, off-metric) | entities 701→701 (exact); tex 0→0; mesh 0→0; skin 3/1364+0 (exact); draws 324→324/27b/4c (exact); `wall_fps` 323.4→293.2 (advisory) — see RT-1: player rig never grounds at spawn |
| skyrim_se | WhiterunDragonsreach | PASS (structural) / 1 HIGH (physics, off-metric) | entities 6044→6049 (+0.08%, in band, same magnitude as the RT-3/#1705 precedent); tex 0→0; mesh 11→9 (improved — see note below); skin 0/1364+0 (exact); draws 2614→2442 cmds (decrease, fine), 3→2 batches, 5→2 gpu_calls; `wall_fps` 321.1→270.0 (advisory) — see RT-1: player rig never grounds at spawn |
| fo4 | InstituteBioScience | PASS | entities 11279→11289 (+0.09%, in band, same magnitude as the RT-3/#1705 precedent); tex 1→1 (`textures\temp_v1_d.dds`); mesh 0→0; skin 100/1364+0 (exact); draws 3800→3800/279→272b(+2.6%,≤×1.1)/40c (exact cmds+gpu_calls); `wall_fps` 50.0→123.7 (advisory) |
| starfield | — | SKIPPED | no cell baseline / empty profile archives (unchanged from every prior sweep) |
| fo76 | — | SKIPPED | no `[profiles.fo76]` key exists for `--game` to expand (unchanged) |

Every diffable structural metric (`entities_total` within its documented ±2%
tolerance band, `tex_missing_unique_paths`, `mesh_cache_failed_count`,
`light_count_directional`, the full `skin_pool_*` triple, and
`bench_draws_*` within its ×1.1 gate) passed clean on all five games — three
of five (fo3, fo4, oblivion) reproduced their baseline's exact-match metrics
to the integer on every non-tolerance field. `bench_fps_*` deltas are
reported per the SKILL's advisory-only rule (RT-2/#1701) and never gate.

One HIGH finding was raised **outside** the SKILL's declared metric table: a
player-rig physics symptom (infinite freefall at cell-load spawn, TES-family
only) visible in the `M28.5` character-controller log lines that this
capture already collects as a byproduct of driving the engine. It reproduces
identically on both TES-family cells and was explicitly left unfiled by the
investigation that closed the adjacent issue, so it is reported here rather
than silently dropped.

## Findings

### RT-1: TES-family (Oblivion, Skyrim) player rig never grounds at cell-load spawn — infinite freefall
- **Severity**: HIGH
- **Dimension**: runtime / physics (character controller)
- **Games**: oblivion (`ICMarketDistrictTheGildedCarafe`), skyrim_se (`WhiterunDragonsreach`)
- **Location**: `byroredux/src/systems/character.rs` (M28.5 grounding, logged as `M28.5 frame N: body Y a→b … grounded=false`); ground-probe/KCC result from `crates/physics/src/world.rs` (`grounded` field on the move-shape result) written into `crates/physics/src/components.rs::CharacterController.is_grounded`
- **Status**: NEW — closest prior work is **Existing: #1832** ("RT-2: TES-family character rig never grounds -> infinite freefall (Oblivion, Skyrim)"), but that issue is **CLOSED** (`stateReason: COMPLETED`, closed 2026-07-05). Its own closing comment states a *partial* fix landed (`ae083d69`, reclassifying zero-mass `Dynamic`-per-enum Havok bodies as `Static` — confirmed fixed, not re-litigated here per prior session guidance) and explicitly flags this exact door-threshold spawn symptom as still reproducing and "not yet filed as a separate issue." No other open or closed issue title matches (`gh issue list --search "freefall OR grounding OR door-threshold OR spawn freefall"` returns only #1832 and unrelated hits). Reported as NEW because the specific symptom was never given its own tracking issue.
- **Description**: On both TES-family cells, the player character controller free-falls from spawn and never sets `is_grounded=true` for the entire 240-frame bench window (and continues in `--bench-hold`). Contrast with the three Fallout-family cells, which all ground within 0–9 frames of spawn. This is the exact symptom #1832 described pre-fix, and its own closing comment already anticipated it would keep reproducing: "Even after the fix, the character still free-falls completely at the door-based spawn point in both Skyrim cells tested. This looks like a **separate** issue... Next step is a fresh investigation into the door-threshold spawn gap specifically... not yet filed as a separate issue."
- **Evidence**:
  - FNV (control): `M28.5 frame 0: body Y 13962.0→13962.0 … grounded=true` (grounds immediately).
  - FO4 (control): `M28.5 frame 0: body Y 294.2→312.2 … grounded=true` (grounds immediately).
  - FO3 (control): `grounded=false` frames 1–4 during the initial fall from spawn height, then `M28.5 frame 9: body Y 7494.0→7490.3 … grounded=true, rapier_bodies=845 [TRANSITION]` (settles, as expected).
  - Oblivion: falls from `Y=414.8` to `Y≈324.0` by frame ~60, then **sticks** at `Y≈323.9–324.0` for the rest of the run (frames 120→900+, `Δ≈0.000`) while `v` stays pinned at the `-2000.0` terminal-velocity cap and `grounded` **never** flips to `true` — the KCC appears to be resting against *something* (Y stops changing) but the grounded flag is never set, a distinct sub-symptom from Skyrim's case below.
  - Skyrim: falls continuously and never contacts anything — `Y` descends monotonically from `-232.3` (frame 0) through `-28824.8` (frame 900) at a steady `Δ≈-33.332`/tick once terminal velocity is reached, `grounded=false` throughout. True infinite fall into the void, matching #1832's original evidence table almost exactly in magnitude/shape.
  - The RT-1/#1698 perf-collapse half of the original #1832 report (Skyrim 321→8.7/30 fps from the falling body sweeping 1575 rapier bodies every substep) does **not** reproduce here — this run's Skyrim `wall_fps` is 270.0, a normal ~16% advisory delta from baseline, not a collapse — so the `ae083d69` mass=0→Static reclassification fix is holding for the performance half even though the grounding half is still broken.
- **Impact**: The player character is not usable at spawn in either TES-family test cell — basic movement/standing is broken for the two games that exercise the `bhkRigidBody`/Havok-derived collision path (Oblivion, Skyrim), while all three Fallout-family games (which share the same character-controller code but different collision-authoring conventions) are unaffected. This blocks any manual playtesting or automated interaction testing that assumes the player starts grounded in a TES-family interior loaded via `--cell`. Does not corrupt telemetry, crash the renderer, or affect any of this audit's gated structural metrics (entities/textures/mesh-cache/skin-pool/draws all matched baseline exactly on both cells) — purely a physics-layer correctness gap.
- **Related**: #1832 (closed — the partial mass=0 fix is confirmed still intact; the door-threshold-spawn continuation was explicitly deferred by that issue's own closing comment). #1698 (closed — the associated Skyrim perf collapse does not reproduce this run, consistent with the `ae083d69` fix holding).
- **Suggested Fix**: Per #1832's own next-step note, investigate the door-threshold spawn specifically rather than the collision-classification angle again (already fixed and confirmed holding). Two candidate leads named in #1832: (1) the Bannered Mare/first-`DoorTeleport` spawn point in Skyrim leads to an *exterior* worldspace not loaded under an interior-only `--cell` invocation, so the landing spot may have no floor geometry on our side of the loaded content at all; (2) a pre-existing code comment in `crates/physics/src/world.rs` about floor-plank vertex gaps (~1-2 BU) at collision-triangle seams — a KCC tunneling issue independent of body classification. Oblivion's "sticks at Y≈324 but never grounds" sub-symptom suggests the KCC probe may also have a grounded-flag threshold/normal-facing bug distinct from the tunneling theory — worth checking whether the resting contact's surface normal is being computed post the Z-up→Y-up conversion (`crates/nif/src/import/coord.rs`) correctly for TES-derived collision meshes specifically.

## Positive confirmations (not findings)

- **fo3, fo4, oblivion reproduce their baselines essentially exactly**: every non-tolerance structural metric (`tex_missing`, `mesh_cache_failed`, `light_count_directional`, all three `skin_pool_*` fields, `bench_draws_cmds`, and on oblivion+fo3+fo4 even `bench_draws_gpu_calls`) matched the committed baseline to the integer. `bench_draws_cmds` matched exactly on fo3 (1839), oblivion (324), and fo4 (3800) — the tightest possible confirmation of the render-load contract.
- **`entities_total` drift on fnv (+102), skyrim (+5), and fo4 (+10) is not new** — these are the *identical* magnitudes already logged as benign non-rendering creep by the RT-3/#1705 precedent audit, confirming the ±2% tolerance band continues to hold and no new entity-spawn regression landed since.
- **Skyrim `mesh_cache_failed_count` improved 11→9** (same direction/magnitude previously logged in `AUDIT_RUNTIME_2026-07-03.md`'s "positive confirmations" section) — the two corrupted control-character mesh paths called out in the original 2026-06-14 baseline note remain fixed (#1620), not regressed.
- **Skyrim `bench_draws_cmds`/`batches`/`gpu_calls` all dropped** (2614→2442, 3→2, 5→2) relative to baseline. All three moves are decreases, which pass cleanly under the SKILL's `≤ baseline` / `≤ baseline×1.1` gates (fewer draws is never a regression), but the magnitude (~7% fewer cmds, one fewer batch/GPU-call bucket) is large enough to be worth a forward pointer for whoever next regenerates this baseline — likely an upstream batching/culling change, not measurement noise, given it's reproducible against a fixed 60 Hz sim step.
- **The oblivion `bench_draws_gpu_calls` staleness previously tracked at #1863** (baseline said `3`, four independent runs read `4`) is resolved — the committed baseline was regenerated 2026-07-03 to `4`, and this run's capture reads `4` again, an exact match.
- **`skin_pool_max=1364` is uniform and exact across all five games** and matches every baseline row — the FNV `skin_pool_max` staleness previously tracked at #1833 (baseline said `1365`) is likewise resolved; the committed FNV baseline already reads `1364`.
- **No `skin_pool_overflow_attempts` on any game** — `SkinSlotPool` cap-sizing (#1284) continues to hold with zero spill across all five captures.

## Cleanup

- All five engine instances launched serially, `byro-dbg`-polled for `pong`
  (up to 90 s), captured, and torn down with `SIGINT` → `SIGKILL` against the
  actual engine PID (resolved via `pgrep -f`, not the `xvfb-run` wrapper PID
  — see the methodology note above for why that distinction mattered this
  run).
- Final check: `pgrep -af 'byroredux --game'` / `pgrep -af 'byroredux|Xvfb'`
  returned no engine/Xvfb processes before this report was written.
- `/tmp/audit/runtime` removed; committed baselines under
  `.claude/audit-baselines/runtime/` untouched (no `--regen` requested this
  run).

## References

- SKILL: `.claude/commands/audit-runtime/SKILL.md`
- Shared protocol: `.claude/commands/_audit-common.md`, `.claude/commands/_audit-severity.md`
- Prior runs: `docs/audits/AUDIT_RUNTIME_2026-07-03.md`,
  `AUDIT_RUNTIME_2026-07-02.md`, `AUDIT_RUNTIME_2026-06-26.md`,
  `AUDIT_RUNTIME_2026-06-23.md`, `AUDIT_RUNTIME_2026-06-14.md`,
  `AUDIT_RUNTIME_2026-06-01.md`
- Tracking issues referenced: #1284, #1619, #1620, #1698 (closed), #1701,
  #1705, #1832 (closed), #1833, #1863
