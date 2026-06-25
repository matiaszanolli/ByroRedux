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

## Invocation surface (verified against `byroredux/src/main.rs`)

The engine resolves a whole game install from one `--game <key>` flag via the
profile registry in `assets/debug_profiles.toml` (`expand_game_profile_args`).
That replaces the old hand-written `--esm`/`--bsa`/`--textures-bsa` table — you
no longer spell out archives per game.

- `--game <key>` expands to the profile's `--esm`, `--bsa`,
  `--textures-bsa`, and (FO4+) `--materials-ba2` args, joined under
  `<--games-root | $BYROREDUX_GAMES_ROOT | /mnt/data/SteamLibrary/steamapps/common>/<subdir>`.
- `--cell <EDID>` loads an interior cell (omit to fall through to the profile's
  `[defaults].cell`, if any).
- `--bench-frames N` runs N frames then prints the single `bench:` summary line.
- `--bench-hold` keeps the engine alive after the bench window so `byro-dbg`
  can attach on port 9876 (prints a `bench-hold:` notice to stderr).

**Profile keys** (the literal `[profiles.<key>]` blocks in
`assets/debug_profiles.toml`): `fnv`, `fo3`, `oblivion`, `skyrim_se`, `fo4`,
`starfield`. There is no `fo76` profile. Use these exact keys for `--game`.

## Checked-in baselines (verified — `ls .claude/audit-baselines/runtime/`)

Five runtime baselines are committed today (the original fnv/fo4 pair plus the
fo3/oblivion/skyrim_se trio created in the 2026-06-14 `--game all` sweep):

| Baseline TSV | Cell | Notes |
|--------------|------|-------|
| `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` | FNV `FreesideAtomicWrangler` | Primary FNV guard. ~9 250 entities, post-#1284 `SkinSlotPool` schema, `MAX_TOTAL_BONES=196608`. |
| `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` | FO4 `InstituteBioScience` | Post-M49 precombine-CSG render + LOD fix. Regenerated 2026-06-19 (RT-4 / #1621) post the efd3c41b precombine alpha-blend wall fix; entities_total drifted 9167→11279 intentionally. Profile `sample_cells` lists this EDID. |
| `.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv` | FO3 `MegatonPlayerHouse` | Created 2026-06-14. ~3 311 entities, zero fallback textures. |
| `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv` | Oblivion `ICMarketDistrictTheGildedCarafe` | Created 2026-06-14. ~701 entities, cleanest path (zero fallback textures / parse fails). |
| `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` | Skyrim SE `WhiterunDragonsreach` | Created 2026-06-14. ~6 044 entities; `mesh_cache_failed_count=11` includes 2 corrupted control-char paths (see AUDIT_RUNTIME_2026-06-14 RT-3). |

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
| `starfield` | — | — | Starfield profile ships empty archives + no `sample_cells`; runtime cell render not yet a stable guard. Use `--sf-smoke` for SF coverage until a cell baseline lands. |

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

For each selected `(game, cell)`:

1. Skip the game if its profile data dir doesn't resolve (the engine logs
   `--game <key>: resolved data dir does not exist`).
2. Launch under `xvfb-run -a` (the swapchain presents to the headless X server;
   `byro-dbg` reads telemetry over the TCP debug protocol):

   ```bash
   xvfb-run -a --server-args="-screen 0 1280x720x24" \
     ./target/release/byroredux \
       --game <KEY> --cell "<CELL_EDID>" \
       --bench-frames 240 --bench-hold \
       > "/tmp/audit/runtime/<game>-<cell>.engine.log" 2>&1 &
   ```

   Capture the PID for cleanup. (Starfield needs its `--materials-ba2`
   archives, which the empty SF profile does not supply — pass them explicitly
   if you ever baseline an SF cell.)

3. Poll `byro-dbg` for ping success (up to 90 s):

   ```bash
   for i in $(seq 1 90); do
     if echo "ping" | timeout 2 ./target/release/byro-dbg | grep -q -i pong; then break; fi
     sleep 1
   done
   ```

4. Sleep 3 s to let the cell settle past initial load.
5. Drive the capture sequence (the four live console commands —
   `byroredux/src/commands/assets.rs` + `byroredux/src/commands/world_info.rs`):

   ```bash
   printf "stats\ntex.missing\nmesh.cache failed\nlight.dump\nquit\n" \
     | ./target/release/byro-dbg \
     > "/tmp/audit/runtime/<game>-<cell>.telem.txt" 2>&1
   ```

6. Tear down: `kill -INT $PID; sleep 2; kill -9 $PID; wait $PID`.

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
> (`byroredux/src/main.rs`, the `println!("bench: …")` block) — they land in the
> `.engine.log`, NOT the `byro-dbg` stream. The `skin=L/M+S` line is emitted to
> the `engine::stats` log target once per wall-second
> (`byroredux/src/systems/debug.rs`, format `skin={}/{}+{}`), so grep the
> `.engine.log` for the LAST `skin=`. There is no `bench-stats` command.

## Phase 3: Extract comparable metrics

Parse these scalars from the captured files. The keys are the live baseline
contract — they must match the committed TSV exactly (cf.
`.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`) or the skill
cannot diff its own baseline:

| Metric | Source | Direction |
|--------|--------|-----------|
| `entities_total` | `bench:` `entities=` (or `stats` `Entities:`) | exact match |
| `tex_missing_unique_paths` | `tex.missing` summary line | ≤ baseline |
| `mesh_cache_failed_count` | `mesh.cache failed` summary | ≤ baseline |
| `light_count_directional` | `light.dump` `CellLightingRes` (always 1 sun) | exact match |
| `skin_pool_live` | `.engine.log` last `skin=L/M+S` (`L`) | ≤ baseline |
| `skin_pool_max` | `.engine.log` last `skin=L/M+S` (`M`) | exact match |
| `skin_pool_overflow_attempts` | `.engine.log` last `skin=L/M+S` (`S`) | `== 0` (exact) |
| `bench_fps_p50` | `bench:` `wall_fps` | **advisory** — report Δ, never gating (see note) |
| `bench_fps_avg` | `bench:` `wall_fps` | **advisory** — report Δ, never gating (see note) |
| `bench_draws_cmds` | `bench:` `draws=N/Mb/Kc` (`N`) | ≤ baseline ×1.1 |
| `bench_draws_batches` | `bench:` `draws=N/Mb/Kc` (`M`) | ≤ baseline ×1.1 |
| `bench_draws_gpu_calls` | `bench:` `draws=N/Mb/Kc` (`K`) | ≤ baseline ×1.1 |

Quirks of these scalars (don't fabricate around them):

- The engine emits ONE `wall_fps`, not a percentile distribution. The baseline's
  `bench_fps_p50` and `bench_fps_avg` both map from that one value (re-run and
  average if you want a true mean). Do not invent a percentile the engine never
  computes.
- `draws=N/Mb/Kc` is the #1258 three-way split: `N` input DrawCommands / `M`
  post-merge batches / `K` actual GPU calls. The pre-#1258 single draw count is
  gone.
- `light.dump` (`byroredux/src/commands/scene.rs` `LightDumpCommand`) dumps `CellLightingRes` /
  `SkyParamsRes` / `GameTimeRes` only — it surfaces the one directional sun, not
  a per-point-light tally, so `light_count_directional` is effectively a
  constant 1 and there is no `light_count_point`.

> **`bench_fps_*` is advisory, not gating (RT-2, #1701).** The engine's single
> `wall_fps` is a headless wall-clock measurement under `xvfb-run`, where
> Xvfb scheduling jitter dominates — especially on small, fast cells (Oblivion
> `ICMarketDistrictTheGildedCarafe`: 701 entities, ~4 GPU calls, ~400 fps). Two
> independent sweeps flagged a phantom fps "regression" there with every
> structural metric unchanged: RT-2 (06-14) recommended demoting it, and the
> 06-23 sweep (#1701, 411.8→352.3, −14.4 %) is the second data point. Report
> the Δ for visibility, but **never raise a `bench_fps_*` move as a REGRESSION
> finding** — only the structural metrics (textures, mesh-cache, skin pool,
> entities, draw split) gate. For a real fps investigation, re-run 3× and
> average (the engine emits one value, not a distribution).

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
  - HIGH: `tex_missing_*` or `mesh_cache_failed_count` grew;
    `skin_pool_overflow_attempts` moved off `0` (any spill = at least one
    entity rendering in bind pose for lack of a slot — pin to #1284
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
   | fnv  | FreesideAtomicWrangler | PASS              | tex_missing 1→1, fps 141→143 |
   | fo4  | InstituteBioScience    | REGRESSION (HIGH) | tex_missing_unique 1→6 (+5)   |
   | fo3  | MegatonPlayerHouse     | BASELINE CREATED  | first run                     |

   ## Findings

   ### RT-1: tex_missing_unique_paths grew on fo4 InstituteBioScience
   - **Severity**: HIGH
   - **Game**: fo4
   - **Cell**: InstituteBioScience
   - **Baseline**: 1 unique missing texture path
   - **Current**: 6 (+5)
   - **Suggested Fix**: re-run `tex.missing entities` to find the responsible
     REFRs; bisect against the last commit touching the resolution chain
     (`byroredux/src/asset_provider.rs`) or the single NIFAL material boundary
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
2. Confirm nothing left running:
   `pgrep -f 'byroredux|byro-dbg' && pkill -f 'byroredux|byro-dbg'`.

## Notes

- **Determinism**: TAA jitter is frame-counter-driven (Halton(2,3)), so
  frame-240 telemetry is reproducible. `BYROREDUX_FIXED_DT=0`
  (`byroredux/src/main.rs`, the `BYROREDUX_FIXED_DT` env read) freezes the
  wall-clock dt so animation / camera / spin don't advance — recommended when
  capturing tolerance metrics.
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
- Determinism precedent: [byroredux/tests/golden_frames.rs](../../byroredux/tests/golden_frames.rs)
- Import-side sibling harness: [crates/nif/tests/translation_completeness.rs](../../crates/nif/tests/translation_completeness.rs)
- NIFAL static audit (the `tex.missing` proxy's code-side counterpart):
  **`/audit-nifal`** — boundary fn [byroredux/src/material_translate.rs](../../byroredux/src/material_translate.rs); spec [docs/engine/nifal.md](../../docs/engine/nifal.md)
- SkinSlotPool cap + spill telemetry: [#1284](https://github.com/matiaszanolli/ByroRedux/issues/1284) (`a3c2836a`)
- DrawCommand vs GPU-call split (`draws=N/Mb/Kc`): [#1258](https://github.com/matiaszanolli/ByroRedux/issues/1258) (`30e2360f`)
