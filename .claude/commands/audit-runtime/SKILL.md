---
description: "Runtime telemetry regression audit — drives headless engine on per-game cells, diffs against checked-in baselines"
argument-hint: "--game <key|all> [--regen] [--cell <EDID>]"
---

# Runtime Telemetry Audit

Drive the engine headless against a per-game representative cell, harvest the
visible-symptom telemetry (`stats`, `tex.missing`, `mesh.cache failed`,
`light.dump` plus the `bench:` summary line), and diff it against a checked-in
baseline TSV under `.claude/audit-baselines/runtime/`. Counts that move the
wrong way become findings.

This is the **runtime arm** of the audit suite: the per-game `audit-*` skills
inspect *code*; this one inspects what actually renders. It is the structural
answer to the recurring complaint that static audits never see the screen.

See `.claude/commands/_audit-common.md` for project layout, game-data paths,
deduplication, severity, and the base finding format. This skill only adds the
drive→capture→diff loop.

## Invocation surface (verified against `byroredux/src/boot/` + `byroredux/src/main.rs`)

The engine resolves a whole game install from one `--game <key>` flag via the
profile registry in `assets/debug_profiles.toml` (`expand_game_profile_args`,
`byroredux/src/boot/cli.rs` — CLI arg parsing/expansion split out of `main.rs`
under #1858).
That replaces the old hand-written `--esm`/`--bsa`/`--textures-bsa` table — you
no longer spell out archives per game.

- `--game <key>` expands to the profile's `--esm`, `--bsa`,
  `--textures-bsa`, and (FO4+) `--materials-ba2` args, joined under
  `<--games-root | $BYROREDUX_GAMES_ROOT | /mnt/data/SteamLibrary/steamapps/common>/<subdir>`.
- `--cell <EDID>` loads an interior cell (omit to fall through to the profile's
  `[defaults].cell`, if any).
- `--bench-frames N` runs N frames then prints the single `bench:` summary line.
- `--bench-mode renderer-static` fixes `dt = 0` and holds the authored camera
  still. `capture.sh` always passes it (#4417) — see *The bench mode is part
  of the baseline* below.
- `--bench-hold` keeps the engine alive after the bench window so `byro-dbg`
  can attach on port 9876 (prints a `bench-hold:` notice to stderr).

**Profile keys** (the literal `[profiles.<key>]` blocks in
`assets/debug_profiles.toml`): `fnv`, `fo3`, `oblivion`, `skyrim_se`, `fo4`,
`starfield`. There is no `fo76` profile. Use these exact keys for `--game`.

## Checked-in baselines (verified — `ls .claude/audit-baselines/runtime/`)

Five runtime baselines are committed today (the original fnv/fo4 pair plus the
fo3/oblivion/skyrim_se trio created in the 2026-06-14 `--game all` sweep).
This table deliberately carries **no metric values**. Every copy it used to
hold went stale within weeks (#4419). Read the TSV: its `# regenerated:`
headers record each value and why it moved.

| Baseline TSV | Cell | Notes |
|--------------|------|-------|
| `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` | FNV `FreesideAtomicWrangler` | Primary FNV guard; densest NPC interior (the #1284 `SkinSlotPool` cap case). |
| `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` | FO4 `InstituteBioScience` | BGSM-heavy + precombine CSG (M49). Profile `sample_cells` lists this EDID. |
| `.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv` | FO3 `MegatonPlayerHouse` | Exterior-style architecture in an interior shell. |
| `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv` | Oblivion `ICMarketDistrictTheGildedCarafe` | The smallest, cleanest cell, and the only one with directional emitters. |
| `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` | Skyrim SE `WhiterunDragonsreach` | Per-entity hot-path stress; carries the 2 corrupted control-char texture paths (AUDIT_RUNTIME_2026-06-14 RT-3). |

> The `.claude/audit-baselines/sf-esm/` dir holds Starfield **ESM resolve-rate**
> baselines for the `--sf-smoke` harness (`byroredux/src/sf_smoke.rs`), NOT this
> skill. Don't diff them here.

Any other `(game, cell)` row below is a *candidate* — running it with no
baseline present establishes one (Phase 4 emits "BASELINE CREATED") rather than
producing a diffable regression guard.

## Candidate cells

Interior-only by default (interiors load fast, are artifact-dense, and skip
worldspace streaming). Override with `--cell <EDID>`. Where a profile ships a
probe-verified `sample_cells` entry, prefer it.

| Game (`--game`) | Cell EDID | Baseline | Rationale |
|-----------------|-----------|----------|-----------|
| `fnv` | `FreesideAtomicWrangler` | ✓ | Committed primary guard. |
| `fnv` | `GSDocMitchellHouse` | — | Profile sample; well-characterised fallback-texture distribution (`docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md` F2). |
| `oblivion` | `ICMarketDistrictTheGildedCarafe` | ✓ | Committed guard (2026-06-14); cleanest path — zero fallback textures / parse fails. Catches regressions on a known-good cell. |
| `fo3` | `MegatonPlayerHouse` | ✓ | Committed guard (2026-06-14); exterior-style architecture in an interior shell. |
| `skyrim_se` | `WhiterunDragonsreach` | ✓ | Committed guard (2026-06-14); per-entity hot-path stress. |
| `fo4` | `InstituteBioScience` | ✓ | Committed guard; BGSM-heavy + precombine CSG (M49). |
| `starfield` | `citycydoniamainlevel` | — | Profile now ships real `default_bsas`/`default_materials_bsas` + this `sample_cells` entry (`6236b130`, 2026-09-03) — no more hand-supplying archives. Still not a stable guard: the 2026-08-30 sweep hit a CRITICAL hard stall on this exact cell (frame 0 never advances, single-core-pinned, RSS oscillating 12→20.6 GB over a 10-minute run — RT-1, `docs/audits/AUDIT_RUNTIME_2026-08-30.md`), unresolved as of this sync. Use `--sf-smoke` for SF coverage until that stall is fixed and a cell baseline lands. |

`--game all` runs every game whose profile data dir resolves (existence-checked
per `expand_game_profile_args`); games whose install is absent are skipped.

## Parameters (from $ARGUMENTS)

- `--game <key|all>`: Required. One of the profile keys above, or `all`.
- `--cell <EDID>`: Override the per-game default cell — e.g. to re-run a
  user-reported symptom against the exact scene that triggered it.
- `--regen`: After running, OVERWRITE the baseline TSV with the current values.
  Use only after an intentional change you've eyeballed — same intent as
  `BYROREDUX_REGEN_GOLDEN=1` for `golden_frames.rs`.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/runtime`.
3. Fetch dedup baseline:
   `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `cargo build --release -p byroredux -p byro-dbg` succeeds.

## Phase 2: Per-game headless launch

For each selected `(game, cell)`, run the capture harness — **do not
hand-roll the launch/teardown**:

```bash
.claude/commands/audit-runtime/capture.sh \
  --game <KEY> --cell "<CELL_EDID>" --out /tmp/audit/runtime [--frames 240]
```

Skip the game if its profile data dir doesn't resolve (the engine logs
`--game <key>: resolved data dir does not exist`). As of the 2026-09-03
`default_bsas` / `default_materials_bsas` / `sample_cells` addition,
`--game starfield` expands to real archives including a `--materials-ba2` —
you no longer need to pass them explicitly. See the Starfield row below
before spending a run on it, though.

It writes the same two files this skill's Phase 3 parses
(`<out>/<game>-<cell>.engine.log` and `.telem.txt`), and does everything the
old inline recipe did: `xvfb-run -a --server-args="-screen 0 1280x720x24"`,
`--bench-frames N --bench-mode renderer-static --bench-hold`, a 90 s
`byro-dbg` ping poll, then
`stats` / `tex.missing` / `mesh.cache failed` / `light.dump` / `quit`, and it
appends the run's `bench_frame_max_ms` to the telemetry file.

**The bench mode is part of the baseline (#4417).** `renderer-static` and
`system-live` place the camera differently: an authored pose versus wherever
the live systems leave it. So they cull a different frustum, and the whole
draw split changes with the mode, not with the code. Same build, Oblivion
`ICMarketDistrictTheGildedCarafe`:

| Mode | `camera_pos` | Draws | `bench_draws_raster_cmds` |
|------|--------------|-------|---------------------------|
| `system-live` | `209.8,546.2,-275.7` | `330/20b/2c` | 22 |
| `renderer-static` | `271.3,464.8,-58.0` | `330/78b/5c` | 132 |

`bench.rs` names `renderer-static` the regression-gate mode, so the harness
pins it (`BENCH_MODE`), refuses to start with `BYROREDUX_FIXED_DT` set, and
fails the capture if the `bench:` line reports any other `mode=`. Every TSV
records the mode in a `bench_mode` row. Check that row before diffing
anything else. The schema test
`every_baseline_records_the_harness_bench_mode` (`byroredux/src/bench.rs`)
keeps the TSVs, `capture.sh` and the enum label in agreement. Don't set
`BYROREDUX_FIXED_DT` by hand for this audit.

**Readiness is gated on the engine, not on `pong` (#4123).** The debug server
answers `ping` as soon as it binds — 1–6 s into a launch — but cell load runs on
the render thread (#3559), so a cold FNV/FO3 engine then stalls 10–12 s on its
first frame and every query fired into that stall times out. The old fixed
3 s settle turned that into a zero-telemetry FATAL on both games. The harness
now waits for the engine log's `bench:` line (printed only once
`--bench-frames` frames have rendered, and failing at once if the engine
dies), then retries `stats` until it answers. Both gates share
`READY_DEADLINE_S` (default 180 s).

**Why it is a script and not a recipe (#3560).** The teardown this section
used to prescribe — `kill -INT $PID` on the backgrounded `xvfb-run` job —
kills the **wrapper, not the engine**. `xvfb-run` runs its command as a child
(`DISPLAY=… "$@"`, no `exec`; read `/usr/bin/xvfb-run`), so the engine
survives and keeps holding port 9876. The next game's capture then attaches
to the **previous game's still-live engine** and files its numbers under the
new game's filename. Reproduced live on 2026-08-30: an FNV run reported
Oblivion's `Entities: 718` and Oblivion's exact 8-path `tex.missing` list,
with `dbg up at 1s` — impossible for a cell that takes ~40 s to load — as the
only tell. This is the RT-1 / #1619 mis-attribution reached through teardown
failure rather than parallelism, so running **serially does not prevent it**,
and any past `--game all` sweep using the old teardown may carry shifted
telemetry, including baselines regenerated from such a sweep.

The harness closes it with three assertions, and each one **fails the
capture** rather than warning:

1. **Pre-flight** — refuses to launch while any `byroredux` process is alive
   or port 9876 is bound. Uses `pgrep -x`, never `pgrep -f`: the `-f` form
   matches the harness's own command line and would make the check vacuous.
2. **Real PID** — resolves the engine's own PID with `pgrep -x byroredux`
   *after* launch, kills that (not just the wrapper), and sweeps any survivor
   afterwards.
3. **Attribution cross-check** — `Entities:` from the `byro-dbg` stats stream
   against `entities=` on the engine's own `bench:` line. Two different
   transports from the same run: streaming can move them a little (the
   tolerance is the same ±2 % the `entities_total` baseline row uses), but a
   capture that read a *different* engine disagrees by orders of magnitude.
   All five runs in the 2026-08-30 report pass it; the one that failed it was
   discarded and re-run, not reported.

`capture.sh --self-test` exercises the parsers, the tolerance, the PID
resolution and the survivor sweep with no game data — run it if you change
the script.

Run games **serially** — one engine + `byro-dbg` capture at a time. The
debug server binds a single fixed TCP port (`BYRO_DEBUG_PORT`, default
`9876`) with **no rebind/retry** (`crates/debug-server/src/listener.rs`), so
two engines launched in parallel collide: the second logs `failed to bind
port 9876: Address already in use`, its telemetry is unreachable for the
whole run, and the capture silently mis-attributes the first game's numbers
to the second (RT-1 / #1619). Serial is the contract this audit assumes.

To parallelise anyway, give **each** concurrent game a distinct port —
export `BYRO_DEBUG_PORT=$((9876 + i))` for **both** the engine launch and
its `byro-dbg` capture (both honour the env var). Without that per-game
offset, do not run them concurrently.

> **Where each metric lives.** The bench scalars (`wall_fps`, `draws=N/Mb/Kc`,
> `entities=`) are on the single `bench:` line printed at `--bench-frames` exit
> (`byroredux/src/app_events.rs` ~line 1074, the `"bench: mode=…"` block — moved
> out of *main.rs* by the #2731 split) — they land in the
> `.engine.log`, NOT the `byro-dbg` stream. The skin pool is the `bench:`
> line's trailing `skin=L/M+S` token (#4417), read at the measured frame.
> Don't use the once-per-second `engine::stats` `skin=` line
> (`byroredux/src/systems/debug.rs`) for this. It fires only when `TotalTime`
> crosses a second, a frozen `dt` never advances `TotalTime`, and so under
> `renderer-static` it appears only after `--bench-hold` resumes live time,
> sometimes not before the harness quits. There is no `bench-stats` command.

## Phase 3: Extract comparable metrics

Parse these scalars from the captured files. The keys are the live baseline
contract — they must match the committed TSV exactly (cf.
`.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`) or the skill
cannot diff its own baseline:

| Metric | Source | Direction |
|--------|--------|-----------|
| `bench_mode` | `bench:` `mode=` | exact match, **checked first**. On a mismatch, diff nothing else (#4417) |
| `entities_total` | `bench:` `entities=` (or `stats` `Entities:`) | within ±2 % (tolerance — see note) |
| `tex_missing_base_color` | `tex.missing` — count of `[slot=base_color]` lines | ≤ baseline (strict gate) |
| `tex_missing_all_slots` | `tex.missing` summary line (`N unique missing textures:`) | **informational** — report Δ, never gating (see note) |
| `mesh_cache_failed_count` | `mesh.cache failed` summary | ≤ baseline |
| `light_count_point` | `light.dump` — count of `kind=Point` rows in the emitter dump | exact match |
| `light_count_directional` | `light.dump` — count of `kind=Directional` rows in the emitter dump | exact match |
| `skin_pool_live` | `bench:` `skin=L/M+S` (`L`) | **advisory** — report Δ, gate only via `skin_pool_max`/`skin_pool_overflow_attempts` below (see note) |
| `skin_pool_max` | `bench:` `skin=L/M+S` (`M`) | exact match |
| `skin_pool_overflow_attempts` | `bench:` `skin=L/M+S` (`S`) | `== 0` (exact) |
| `bench_fps_p50` | `bench:` `wall_fps` | **advisory** — report Δ, never gating (see note) |
| `bench_fps_avg` | `bench:` `wall_fps` | **advisory** — report Δ, never gating (see note) |
| `bench_frame_p50_ms` | `bench:` `frame_p50_ms` | **advisory** — report Δ, never gating (see note) |
| `bench_frame_p95_ms` | `bench:` `frame_p95_ms` | **advisory** — report Δ, never gating (see note) |
| `bench_frame_max_ms` | `bench:` `frame_max_ms` | **advisory** — report Δ, never gating (see note) |
| `bench_draws_cmds` | `bench:` `draws=N/Mb/Kc` (`N`) | ≤ baseline ×1.1 |
| `bench_draws_raster_cmds` | `bench:` `bench_draws_raster_cmds=R` (sorted raster prefix) | report branch at `R >= 3000` |
| `bench_draws_batches` | `bench:` `draws=N/Mb/Kc` (`M`) | ≤ baseline ×1.1 |
| `bench_draws_gpu_calls` | `bench:` `draws=N/Mb/Kc` (`K`) | ≤ baseline ×1.1 |

Quirks of these scalars (don't fabricate around them):

- `wall_fps` itself is still ONE aggregate value (total frames / total elapsed
  seconds) — `bench_fps_p50` and `bench_fps_avg` both map from that same
  number (re-run and average if you want a true cross-run mean). But a real
  per-frame CPU distribution now exists alongside it: `bench_frame_distribution`
  (the helper stays in `byroredux/src/main.rs` ~line 86; its caller is
  `byroredux/src/app_events.rs` ~line 954) nearest-rank-percentiles the per-frame
  `bench_cpu_frame_ms` samples (one push per rendered frame, `about_to_wait`
  wall-clock) into `frame_p50_ms`/`frame_p95_ms`/`frame_max_ms` on the same
  `bench:` line, unconditionally — not gated behind `--bench-camera` or
  streaming mode. Use those three for tail-latency Δs; don't fall back to
  guessing a distribution from `wall_fps` alone.
- `draws=N/Mb/Kc` is the #1258 three-way split: `N` input DrawCommands / `M`
  post-merge batches / `K` actual GPU calls. The `N` count includes RT-only
  occluders, which remain in the instance/TLAS stream but are excluded from
  raster sorting and batching. Use `bench_draws_raster_cmds` to evaluate the
  `DRAW_SORT_PARALLEL_THRESHOLD` branch; `bench_draws_cmds` alone cannot prove
  which sort path ran. The pre-#1258 single draw count is gone.
- **Draw-split invariant — check it before diffing (#4195).** One capture must
  satisfy `bench_draws_batches <= bench_draws_raster_cmds <= bench_draws_cmds`
  and `bench_draws_gpu_calls <= 2 × bench_draws_batches`. A `DrawBatch` is
  only formed from a command in the raster prefix, and several commands can
  merge into one batch. `gpu_calls` is **not** bounded by `batches`: the
  two-sided blend split (`needs_two_sided_blend_split`) records up to two
  direct draws for a single batch (`geometry_pass.rs`,
  `u32::from(back) + u32::from(front)`). Indirect grouping only lowers it. A
  row set breaking this cannot come from one run, most likely a partial
  `--regen` that held some `bench_draws_*` rows from an older capture. Report
  it as a stale-baseline finding and re-capture all four rows together; don't
  diff against them. The FO4 TSV carried exactly this (`batches 296 >
  raster_cmds 256`) until #4195.
- `light.dump` (`byroredux/src/commands/scene.rs` `LightDumpCommand`) dumps
  `CellLightingRes` / `SkyParamsRes` / `GameTimeRes` **and**, since `5f970bae`
  (2026-08-15), a `LightSource emitters: N` tally followed by a per-emitter dump
  (kind, source, position, radiance, dimmer, range, attenuation, visibility,
  flags). Derive `light_count_point` by counting `kind=Point` rows and
  `light_count_directional` by counting `kind=Directional` rows. Do **not**
  take `light_count_point` from `N`: the tally counts every emitter,
  directional ones included. Oblivion's cell dumps `emitters: 10` = 8 point
  + 2 directional, and its baseline stores 8 (#4419). Do **not**
  infer either from the mere presence of a `CellLightingRes` block, which is
  what made the old `light_count_directional` row a gate that could never fail
  (#3424). The metric has real dynamic range: measured 2026-08-27 at `969d81c8`
  — `fnv` 30, `fo3` 11, `oblivion` 8, `skyrim_se` 28, `fo4` 685. The
  `CellLightingRes` `directional_color` is a separate cell field and is not
  zero on every interior: FNV's cell dumps `[0.224, 0.208, 0.133]`. It is
  not an emitter and feeds neither row.
  **Baselines carry a `light_count_point` row as of the #3556 (RT-10) fix** —
  the values above are what got committed; a future `--regen` still overwrites
  them like any other row.

> **`bench_fps_*` / `bench_frame_*_ms` is advisory, not gating (RT-2, #1701).**
> `wall_fps` and the per-frame `frame_p50_ms`/`frame_p95_ms`/`frame_max_ms`
> distribution are both headless wall-clock measurements under `xvfb-run`,
> where Xvfb scheduling jitter dominates — especially on small, fast cells
> (Oblivion `ICMarketDistrictTheGildedCarafe`: 701 entities, ~4 GPU calls,
> ~400 fps). Two independent sweeps flagged a phantom fps "regression" there
> with every structural metric unchanged: RT-2 (06-14) recommended demoting
> it, and the 06-23 sweep (#1701, 411.8→352.3, −14.4 %) is the second data
> point — that predates the frame-distribution fields, but the same headless
> jitter source applies to them equally. Report the Δ for visibility, but
> **never raise a `bench_fps_*` or `bench_frame_*_ms` move as a REGRESSION
> finding** — only the structural metrics (textures, mesh-cache, skin pool,
> entities, draw split) gate. For a real fps investigation, re-run 3× and
> average `wall_fps`, or cross-check against the single-run `frame_p95_ms` /
> `frame_max_ms` tail for a cheaper (if noisier) same-run signal.

> **`entities_total` is a ±2 % tolerance metric, not exact (RT-3, #1705).**
> It counts *all* ECS entities, including non-rendering bodies — collision-only
> colliders, the ragdoll/character rig, markers — which drift up benignly as
> collision/ragdoll/material work lands without changing what renders. Three
> successive audits logged the same harmless creep (RT-2 06-14 fnv, RT-4 06-14
> fo4, RT-3 06-23 fnv +102 / skyrim +5 / fo4 +10), and an *exact*-match gate
> turns every such addition into a false diff that can mask a real regression in
> the noise. The exact render-load contract is `bench_draws_cmds` (the `N` of the
> draw split, gated `≤ baseline ×1.1`) — that is the "render_entities" half of
> the RT-4 split suggestion; `entities_total` is the total-body half and only
> gates when it moves **beyond ±2 %** in either direction. A drop past −2 % still
> gates (entities failing to spawn is a real regression); within-band drift is a
> clean pass, not a finding. Regenerate the baseline with `--regen` only when a
> deliberate change moves it past the band.

> **`tex_missing_unique_paths` is split into two rows (#3550, RT-4).**
> `ff177576` (#3349, "per-slot tex.missing") widened `tex.missing` from
> walking only `TextureHandle` (base-color) to walking the full 26-role
> `MaterialTextureHandles` set, but the single `tex_missing_unique_paths`
> gate still compared the new 26-slot total against baselines captured on
> the 1-slot surface — a metric-definition change disguised as a
> regression, on every game, every sweep. `tex_missing_base_color` (count
> the `[slot=base_color]` bucket lines) is the strict, baseline-comparable
> gate; `tex_missing_all_slots` (the command's own summary count) is
> informational only — report its Δ, never raise it as a finding, since no
> pre-#3349 baseline is comparable to it and it has no history to diff
> against yet. Once `tex_missing_all_slots` has its own regenerated
> baseline lineage across a few sweeps, promote it to a real gate.

> **`skin_pool_live` is advisory, `skin_pool_max` / `skin_pool_overflow_attempts`
> are the hard gate (#3553, RT-7).** `skin_pool_live` (entities currently
> holding a bone-palette slot) tracks total scene population the same way
> `entities_total` does, and creeps for the same benign reason — it is not
> an independent signal of a `SkinSlotPool` (#1284) problem. The pair that
> actually carries that signal is `skin_pool_overflow_attempts` (must stay
> `0` — any nonzero value means at least one entity is rendering in bind
> pose for lack of a slot) and `skin_pool_max` (the cap itself, exact
> match). Report `skin_pool_live`'s Δ for visibility; only
> `skin_pool_overflow_attempts` moving off `0` or `skin_pool_max` changing
> gates.

Write the extracted scalars to `/tmp/audit/runtime/<game>-<cell>.current.tsv`.

## Phase 4: Diff against baseline

Compare `/tmp/audit/runtime/<game>-<cell>.current.tsv` against
`.claude/audit-baselines/runtime/<game>-<cell>.tsv`:

- **Baseline absent** — first run: copy current to baseline with a
  `# regenerated: YYYY-MM-DD` header. NOT a finding; report as "BASELINE CREATED".
- **`--regen` set** — overwrite baseline with current. NOT a finding; report as
  "BASELINE UPDATED".
- **Metric regressed** (against its Phase 3 direction) — emit one finding per
  metric, severity per magnitude (see `_audit-severity.md`). `bench_fps_*` is
  **advisory** (see the Phase 3 note): list its Δ in the report table but never
  emit it as a finding regardless of magnitude.
  - HIGH: `tex_missing_base_color` or `mesh_cache_failed_count` grew (NOT
    `tex_missing_all_slots` — informational, never a finding, see the Phase 3
    note); `skin_pool_overflow_attempts` moved off `0` (any spill = at least
    one entity rendering in bind pose for lack of a slot — pin to #1284
    `SkinSlotPool` cap + descriptor-pool fix, `a3c2836a`).
  - MEDIUM: any other count moved against direction.
  - LOW: count drift within ±5 % on a tolerance metric.

## Phase 5: Report

1. Combine findings into `docs/audits/AUDIT_RUNTIME_<TODAY>.md`:

   ```markdown
   # Runtime Telemetry Audit — YYYY-MM-DD

   ## Per-game baseline comparison

   | Game | Cell | Status | Δ vs baseline |
   |------|------|--------|---------------|
   | fnv  | FreesideAtomicWrangler | PASS              | tex_missing_base_color 1→1, fps 141→143 |
   | fo4  | InstituteBioScience    | REGRESSION (HIGH) | tex_missing_base_color 1→6 (+5)   |
   | fo3  | MegatonPlayerHouse     | BASELINE CREATED  | first run                     |

   ## Findings

   ### RT-1: tex_missing_base_color grew on fo4 InstituteBioScience
   - **Severity**: HIGH
   - **Game**: fo4
   - **Cell**: InstituteBioScience
   - **Baseline**: 1 unique missing base-color texture path
   - **Current**: 6 (+5)
   - **Suggested Fix**: re-run `tex.missing entities` to find the responsible
     REFRs; bisect against the last commit touching the resolution chain
     (`byroredux/src/asset_provider/texture.rs`) or the single NIFAL material boundary
     (`byroredux/src/material_translate.rs` `translate_material` →
     `Material::resolve_pbr`, `crates/core/src/ecs/components/material.rs`). A
     dropped texture slot at that boundary surfaces here as a `tex.missing` bump.
     Cross-check the import-side sibling
     `crates/nif/tests/translation_completeness.rs`, and run **`/audit-nifal`**
     for the static audit of that tier.
   ```

2. Tell the user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_RUNTIME_<TODAY>.md`.

## Phase 6: Cleanup

1. `rm -rf /tmp/audit/runtime` (baselines under
   `.claude/audit-baselines/runtime/` are NOT touched).
2. Confirm nothing left running: `pgrep -x byroredux; pgrep -x byro-dbg`,
   and `pkill -x byroredux; pkill -x byro-dbg` if either reports anything.
   **`-x`, not `-f`** (#3560): the `-f` form matches your own shell's command
   line whenever it contains those words, so it reports a survivor that isn't
   there and `pkill -f` then targets the harness. `capture.sh` already sweeps
   after each capture; this is the belt-and-braces check for a run that was
   interrupted before its teardown.

## Notes

- **Determinism**: TAA jitter is frame-counter-driven (Halton(2,3)), and
  `capture.sh` pins `--bench-mode renderer-static` (fixed `dt = 0`, authored
  camera), so frame-240 telemetry is reproducible without any env var. Don't
  export `BYROREDUX_FIXED_DT` for this audit. The legacy variable
  (`resolve_bench_selection`, `byroredux/src/bench.rs`) silently selects a
  mode, so the engine rejects it alongside an explicit `--bench-mode`, and
  the harness refuses to start with it set. An earlier revision of this note
  recommended it while the harness inferred `system-live`, and the two
  resulting captures were diffed against each other (#4417).
- **Per-game data**: resolved via the `--game` profile registry
  (`assets/debug_profiles.toml`). The separate `BYROREDUX_*_DATA` env vars in
  `crates/nif/tests/common/mod.rs` drive the *test* harnesses, not this skill.
- **Composability**: a screenshot-diff extension (sibling of
  `byroredux/tests/golden_frames.rs`, currently cube-demo only) is the natural
  next layer — same `(game, cell)` matrix, PNG-pixel diff instead of
  scalar-telemetry diff. This skill's scalar surface is the lower-bar guard that
  lands first.

## References

- Parent epic: [#1277](https://github.com/matiaszanolli/ByroRedux/issues/1277)
- This workstream: [#1283](https://github.com/matiaszanolli/ByroRedux/issues/1283)
- Symptom record: [docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md](../../docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md)
- Smoke-test pattern (`--bench-hold` + `byro-dbg` attach): [docs/smoke-tests/README.md](../../docs/smoke-tests/README.md)
- **Playable-slice gates (2026-08-16)** — the P0/P1/P2 scripts are the runtime
  contract for the gameplay slice, which has no owner audit skill. When this
  audit is run to bless a build, run them and report pass/fail alongside the
  scalar telemetry rather than silently skipping them:
  [p0-door-interaction.sh](../../docs/smoke-tests/p0-door-interaction.sh) (activation +
  cell transition), [p1-character-traversal.sh](../../docs/smoke-tests/p1-character-traversal.sh)
  (movement/collision/camera), [p2-melee-core.sh](../../docs/smoke-tests/p2-melee-core.sh)
  (combat core — Health derives to 50, seven bound attacks emit seven canonical
  `HitEvent`s, zero Health yields one `Dead` transition plus an 18-body ragdoll;
  passing as of 2026-08-16). Specs: [docs/engine/playable-vertical-slice.md](../../docs/engine/playable-vertical-slice.md),
  [docs/engine/p2-combat-fixture.md](../../docs/engine/p2-combat-fixture.md)
- Determinism precedent: [byroredux/tests/golden_frames.rs](../../byroredux/tests/golden_frames.rs)
- Import-side sibling harness: [crates/nif/tests/translation_completeness.rs](../../crates/nif/tests/translation_completeness.rs)
- NIFAL static audit (the `tex.missing` proxy's code-side counterpart):
  **`/audit-nifal`** — boundary fn [byroredux/src/material_translate.rs](../../byroredux/src/material_translate.rs); spec [docs/engine/nifal.md](../../docs/engine/nifal.md)
- SkinSlotPool cap + spill telemetry: [#1284](https://github.com/matiaszanolli/ByroRedux/issues/1284) (`a3c2836a`)
- DrawCommand vs GPU-call split (`draws=N/Mb/Kc`): [#1258](https://github.com/matiaszanolli/ByroRedux/issues/1258) (`30e2360f`)
