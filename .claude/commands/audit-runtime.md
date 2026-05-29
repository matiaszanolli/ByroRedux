---
description: "Runtime telemetry regression audit — drives headless engine on per-game cells, diffs against checked-in baselines"
argument-hint: "--game <name|all> [--regen] [--cell <EDID>]"
---

# Runtime Telemetry Audit

Drive the engine headless against per-game representative cells, harvest the
visible-symptom telemetry (`tex.missing`, `mesh.cache failed`, `light.dump`,
`stats`, `bench-stats`), and compare against the checked-in baseline TSV at
`.claude/audit-baselines/runtime/<game>-<cell>.tsv`. Regressions (counts
moving in the wrong direction) become audit findings.

**Structural fix** for the original #1277 epic complaint that audits inspect
*code* but never *rendered output*. This skill makes runtime behavior
auditable on every relevant commit — the per-game `audit-*` skills cover
the static code; this one covers what actually shows up on screen.

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication rules, and finding format.

## Game Context

Per-game representative cells (interior-only by default — interiors load
fast, contain dense per-cell artifacts, and don't require worldspace
streaming). Override with `--cell <EDID>`.

> **Baseline reality check.** Only ONE baseline is checked in today:
> `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`. The
> rows below are the *candidate* representative cells; a row only becomes
> a diffable regression guard once its baseline TSV is committed (Phase 4
> emits "BASELINE CREATED" on first run). Before relying on a row, confirm
> its baseline exists; otherwise the run only establishes a new baseline.

| Game        | Cell EDID                             | Baseline | Rationale                                                                 |
|-------------|---------------------------------------|----------|---------------------------------------------------------------------------|
| FNV         | `FreesideAtomicWrangler`              | ✓        | The one committed baseline today (9 250 entities, post-#1284 schema). Primary regression guard. |
| FNV (alt)   | `GSDocMitchellHouse`                  | —        | `FALLOUT_SYMPTOMS_2026-05-26.md` F2 investigation; well-characterized fallback-texture distribution. |
| Oblivion    | `ICMarketDistrictTheGildedCarafe`     | —        | The "gorgeous baseline" per `FALLOUT_SYMPTOMS_2026-05-26.md` — zero fallback textures, zero parse fails. Catches regressions on the cleanest path. |
| FO3         | `MegatonPlayerHouse`                  | —        | F2 sibling; 929 REFRs, exterior-style architecture in interior shell.    |
| Skyrim SE   | `WhiterunDragonsreach`                | —        | Stress-tests the per-entity hot path.                                    |
| FO4         | `InstituteBioScience`                 | —        | FO4 BGSM-heavy + bhkNPCollisionObject test bed (#1277 Task 1).           |
| Starfield   | `CityCydoniaMainLevel`                | —        | SF ESM Phase 1 (`eac91535`) now loads cells; first walkable SF render in Session 42 (`83e4326d`, see `docs/audits/SF_FIRST_RENDER_2026-05-28.md`). First SF runtime-telemetry candidate (`--materials-ba2` required for BGSM resolution). |

`--game all` runs the suite across every game whose data is available
(`BYROREDUX_*_DATA` env-var lookup per
`crates/nif/tests/common/mod.rs::game_data_dir`).

## Parameters (from $ARGUMENTS)

- `--game <name|all>`: Required. One of `oblivion`/`fnv`/`fo3`/`skyrim-se`/`fo4`/`fo76`/`starfield`/`all`.
- `--cell <EDID>`: Override the per-game default cell (e.g., for re-running a
  user-reported symptom against the specific scene that triggered it).
- `--regen`: After running, OVERWRITE the baseline TSV with the current
  values. Use only after an intentional change you've eyeballed — same
  semantics as `BYROREDUX_REGEN_GOLDEN=1` for `golden_frames.rs`.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/runtime`.
3. `mkdir -p .claude/audit-baselines/runtime` (if first run on the repo).
4. Fetch dedup baseline:
   `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
5. Confirm `cargo build --release -p byroredux -p byro-dbg` succeeds.

## Phase 2: Per-game Headless Launch

For each `(game, cell)` pair selected by `--game` / `--cell`:

1. Skip the game if its `BYROREDUX_*_DATA` env var doesn't resolve to a
   directory (per `crates/nif/tests/common/mod.rs::game_data_dir`).
2. Launch the engine under `xvfb-run -a` (no real Vulkan-on-display
   needed — the swapchain still presents to the headless X server,
   `byro-dbg` reads telemetry through the TCP debug protocol):

   ```bash
   xvfb-run -a --server-args="-screen 0 1280x720x24" \
     ./target/release/byroredux \
       --esm "<ESM>" --cell "<CELL_EDID>" \
       --bsa "<MESHES_BSA>" --textures-bsa "<TEXTURES_BSA>" \
       --bench-frames 240 --bench-hold \
       > "/tmp/audit/runtime/<game>-<cell>.engine.log" 2>&1 &
   ```

   Capture PID for cleanup.

3. Poll `byro-dbg` for ping success (up to 90 s):

   ```bash
   for i in $(seq 1 90); do
     if echo "ping" | timeout 2 ./target/release/byro-dbg | grep -q -i pong; then
       break
     fi
     sleep 1
   done
   ```

4. Sleep 3 s to let the cell settle past initial load.
5. Drive the telemetry capture sequence:

   ```bash
   printf "stats\ntex.missing\nmesh.cache failed\nlight.dump\nquit\n" \
     | ./target/release/byro-dbg \
     > "/tmp/audit/runtime/<game>-<cell>.telem.txt" 2>&1
   ```

   > **No `bench-stats` console command exists.** The four live commands
   > are `stats` / `tex.missing` / `mesh.cache failed` / `light.dump`
   > (`byroredux/src/commands.rs`); there is no `bench-stats`. Bench
   > scalars (`wall_fps`, `draws=N/Mb/Kc`) are parsed from the single
   > `bench:` summary line printed to stdout at `--bench-frames` exit
   > (`byroredux/src/main.rs:2096`), so they land in the
   > `<game>-<cell>.engine.log` file, **not** the byro-dbg telemetry
   > stream. Likewise `skin_pool_*` is NOT in the `stats` command output
   > (`commands.rs:48`) nor the `bench:` line — it is emitted to the
   > `engine::stats` log target once per wall-second
   > (`byroredux/src/systems/debug.rs:46`, `skin=L/M+S`), so grep the
   > engine log for the last `skin=` line.

6. Tear down: `kill -INT $PID; sleep 2; kill -9 $PID; wait $PID`.

Runs for up to 4 games in parallel (Xvfb auto-display lets them coexist).

## Phase 3: Extract Comparable Metrics

For each captured `.telem.txt` / `.engine.log` file, parse out the
comparable scalars. These keys are the LIVE baseline contract — they must
match the checked-in TSV exactly (see
`.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`), or the
skill cannot diff its own baseline:

| Metric                        | Source line                          | Direction       |
|-------------------------------|--------------------------------------|-----------------|
| `entities_total`              | `stats` output (`Entities:`)         | exact match     |
| `tex_missing_unique_paths`    | `tex.missing` summary line           | ≤ baseline      |
| `mesh_cache_failed_count`     | `mesh.cache failed` summary          | ≤ baseline      |
| `light_count_directional`     | `light.dump` (`CellLightingRes`)     | exact match     |
| `skin_pool_live`              | engine-log `skin=L/M+S` (`L`)        | ≤ baseline      |
| `skin_pool_max`               | engine-log `skin=L/M+S` (`M`)        | exact match     |
| `skin_pool_overflow_attempts` | engine-log `skin=L/M+S` (`S`)        | `== 0` (exact)  |
| `bench_fps_p50`               | `bench:` line `wall_fps` (main.rs:2096) | ≥ baseline ×0.9 |
| `bench_fps_avg`               | `bench:` line `wall_fps` (main.rs:2096) | ≥ baseline ×0.9 |
| `bench_draws_cmds`            | `bench:` line `draws=N/Mb/Kc` (`N`)  | ≤ baseline ×1.1 |
| `bench_draws_batches`         | `bench:` line `draws=N/Mb/Kc` (`M`)  | ≤ baseline ×1.1 |
| `bench_draws_gpu_calls`       | `bench:` line `draws=N/Mb/Kc` (`K`)  | ≤ baseline ×1.1 |

Notes on the bench scalars:
- The engine emits a single `wall_fps` (`main.rs:2059`), NOT a p50/avg
  percentile distribution. The baseline's `bench_fps_p50` / `bench_fps_avg`
  are therefore both mapped from that one `wall_fps` value (capture it
  twice, or re-run and average across runs if you want a true mean). Do
  not invent a percentile the engine never computes.
- `bench_draws_*` is the #1258 three-way split (`30e2360f`): `N` input
  DrawCommands / `M` post-merge batches / `K` actual GPU calls
  (`main.rs:2122`). The pre-#1258 single `bench_draw_calls_total` is gone.
- The `light.dump` command (`commands.rs:1669`) dumps
  `CellLightingRes` / `SkyParamsRes` / `GameTimeRes` only — it surfaces the
  one directional sun, NOT a per-point-light count, so there is no
  `light_count_point` metric.

Write the extracted scalars to a per-run TSV:
`/tmp/audit/runtime/<game>-<cell>.current.tsv`.

## Phase 4: Diff Against Baseline

For each `(game, cell)` pair, compare
`/tmp/audit/runtime/<game>-<cell>.current.tsv` against
`.claude/audit-baselines/runtime/<game>-<cell>.tsv`:

- **Baseline absent**: first run — copy current to baseline with a `# regenerated:
  YYYY-MM-DD` header. NOT a finding; surfaces in the report as "BASELINE CREATED".
- **`--regen` set**: overwrite baseline with current. NOT a finding;
  surfaces as "BASELINE UPDATED".
- **Metric regressed** (against the direction in the Phase 3 table):
  emit a finding per metric with severity per the regression magnitude:
  - HIGH: `tex_missing_*` or `mesh_cache_failed_count` grew,
    `skin_pool_overflow_attempts` moved off `0` (any spill means at least
    one entity is rendering in bind pose for lack of a slot — pin to #1284
    `SkinSlotPool` cap + descriptor-pool fix, `a3c2836a`), OR
    `bench_fps_p50` dropped > 20 %.
  - MEDIUM: any other count moved against direction.
  - LOW: count drift within ±5 % on tolerance metrics.

## Phase 5: Report Finalization

1. Combine per-game findings into `docs/audits/AUDIT_RUNTIME_<TODAY>.md`:

   ```markdown
   # Runtime Telemetry Audit — YYYY-MM-DD

   ## Per-game baseline comparison

   | Game        | Cell                              | Status                | Δ vs baseline                          |
   |-------------|-----------------------------------|-----------------------|----------------------------------------|
   | Oblivion    | ICMarketDistrictTheGildedCarafe   | PASS                  | tex_missing 0→0, fps 47.2→47.5         |
   | FNV         | GSDocMitchellHouse                | REGRESSION (HIGH)     | tex_missing_unique 54→58 (+4) ←–       |
   | FO3         | MegatonPlayerHouse                | BASELINE CREATED      | first run                              |

   ## Findings

   ### RT-1: tex_missing_unique_paths grew on FNV GSDocMitchellHouse
   - **Severity**: HIGH
   - **Game**: FNV
   - **Cell**: GSDocMitchellHouse
   - **Baseline**: 54 unique missing texture paths
   - **Current**: 58 unique missing texture paths (+4)
   - **Top new misses**: …
   - **Suggested Fix**: re-run `tex.missing entities` to identify the
     responsible REFRs; compare against the last commit touching
     `byroredux/src/asset_provider.rs` (resolution chain) OR the NIFAL
     canonical-translation boundary
     `byroredux/src/material_translate.rs::translate_material` /
     `Material::resolve_pbr`
     (`crates/core/src/ecs/components/material.rs:588`) — a regression
     that drops a texture slot at the single `ImportedMesh`→`Material`
     boundary surfaces here as a runtime `tex.missing` bump. Cross-check
     against the import-side sibling
     `crates/nif/tests/translation_completeness.rs` (`MaterialStats`
     per-game slot coverage), and run **`/audit-nifal`** for a static
     audit of that translation tier.
   ```

2. Inform the user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_RUNTIME_<TODAY>.md`.

## Phase 6: Cleanup

1. `rm -rf /tmp/audit/runtime` (per-run artifacts; baseline TSVs in
   `.claude/audit-baselines/runtime/` are NOT touched).
2. Confirm no `byroredux` or `byro-dbg` processes left running:
   `pgrep -f 'byroredux|byro-dbg' && pkill -f 'byroredux|byro-dbg'`.

## Notes

- **Determinism**: the engine's TAA jitter is frame-counter driven (Halton(2,3)),
  so frame-240 telemetry is reproducible. `BYROREDUX_FIXED_DT=0` (set per the
  golden_frames.rs precedent) freezes the wall-clock dt so animation /
  camera spin / cube rotation don't advance — recommended in Phase 2's
  engine launch when surfacing tolerance metrics.
- **Game data**: `BYROREDUX_OBLIVION_DATA` / `BYROREDUX_FO3_DATA` /
  `BYROREDUX_FNV_DATA` / `BYROREDUX_SKYRIMSE_DATA` / `BYROREDUX_FO4_DATA` /
  `BYROREDUX_FO76_DATA` / `BYROREDUX_STARFIELD_DATA` per
  `crates/nif/tests/common/mod.rs`. Falls back to canonical Steam install
  paths when unset.
- **Composability**: a future screenshot-diff extension (sibling of
  `byroredux/tests/golden_frames.rs` — currently cube-demo only) is the
  natural next layer: same `(game, cell)` matrix, PNG-pixel diff instead
  of telemetry-scalar diff. Tracked as a separate follow-up; this skill's
  scalar-telemetry surface is the lower-bar regression guard that lands
  first.
- **Sibling of Task 8's `translation_completeness` harness**: Task 8
  exercises the IMPORT path (parse + import → MaterialStats); this
  skill exercises the RUNTIME path (cell-load + render → console
  telemetry). Both surface regressions the per-game `audit-*` static
  audits structurally can't catch.

## References

- Parent epic: [#1277](https://github.com/matiaszanolli/ByroRedux/issues/1277)
- This workstream: [#1283](https://github.com/matiaszanolli/ByroRedux/issues/1283)
- Symptom-record this skill operationalises: [docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md](../../docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md)
- Per-game data lookup: [crates/nif/tests/common/mod.rs](../../crates/nif/tests/common/mod.rs)
- Determinism precedent: [byroredux/tests/golden_frames.rs](../../byroredux/tests/golden_frames.rs)
- Sibling import-side harness: [crates/nif/tests/translation_completeness.rs](../../crates/nif/tests/translation_completeness.rs)
- NIFAL canonical-translation static audit (the runtime `tex.missing`
  proxy's code-side counterpart): **`/audit-nifal`** —
  [.claude/commands/audit-nifal.md](audit-nifal.md);
  spec [docs/engine/nifal.md](../../docs/engine/nifal.md);
  boundary fn [byroredux/src/material_translate.rs](../../byroredux/src/material_translate.rs)
- SkinSlotPool cap + spill telemetry: [#1284](https://github.com/matiaszanolli/ByroRedux/issues/1284) (`a3c2836a`)
- DrawCommand vs GPU-call split (`draws=N/Mb/Kc`): [#1258](https://github.com/matiaszanolli/ByroRedux/issues/1258) (`30e2360f`)
