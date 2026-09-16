# Runtime Telemetry Audit — 2026-09-16

Sweep of the five baselined games (fnv, fo3, oblivion, skyrim_se, fo4) at
`1bf1c44d3`. Release build (`cargo build --release -p byroredux -p byro-dbg`)
was clean. Before launch, no `byroredux`/`byro-dbg` process was running and
port 9876 was free. Starfield was **not** run. It has no baseline, and the
`citycydoniamainlevel` stall (2026-08-30 RT-1, 2026-09-11 RT-5) is still
unresolved.

All ten captures used `capture.sh`, run one at a time. Every capture exited 0
and passed the harness's `stats`-vs-`bench:` entity cross-check. The #4123
readiness gate worked: FNV's cold first frame (`frame_max_ms=9885.79`) no longer
aborts the capture.

**Method note — two passes.** The first pass exported `BYROREDUX_FIXED_DT=0`,
as this skill's *Notes* section recommends. That flag silently switches the
engine to `--bench-mode renderer-static`. Every committed baseline was captured
in `system-live`, so in that pass the draw-split rows moved on four of five
games (RT-1). The table below comes from the second pass: all five games
re-captured with `FIXED_DT` unset (`mode=system-live`), the mode the baselines
were captured in.

## Per-game baseline comparison (system-live pass)

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| fnv | FreesideAtomicWrangler | PASS | entities 7342→7414 (+0.98 %); tex_missing_base_color 1→1; mesh_cache_failed 0→0; draws 2110→2197 / 109→109b / 26→26c; lights 30/0; skin 217/1364+0; fps 65.4→68.6 (advisory) |
| fo3 | MegatonPlayerHouse | PASS (improved) | entities 3493→3543 (+1.43 %); tex_missing_base_color 0→0; **mesh_cache_failed 3→0**; draws 1581/100b/11c exact; lights 11/0; skin 7/1364+0; fps 62.7→84.8 (advisory) |
| oblivion | ICMarketDistrictTheGildedCarafe | PASS (see RT-3) | entities 745→745; draws 330/20b/2c exact, raster 22→22; lights: 8 `kind=Point` + 2 `kind=Directional` (baseline 8/2) — but `LightSource emitters: 10`; skin 4/1364+0; fps 269.7→285.3 (advisory) |
| skyrim_se | WhiterunDragonsreach | STALE BASELINE (MEDIUM, RT-2) | **entities 8126→9461 (+16.4 %)**; skin_pool_live 83→133 (advisory); draws 2342→2457 (+4.9 %, inside ×1.1) / 9→9b / 2→2c; **mesh_cache_failed 9→0**; lights 28/0; fps 161.9→96.5 (advisory) |
| fo4 | InstituteBioScience | PASS | entities 18969→18969; tex_missing_base_color 1→1; mesh_cache_failed 0→0; draws 3964/248b/16c exact, raster 359→359; lights 685/0; skin_pool_live 299→264 (advisory), 1364+0 |
| starfield | citycydoniamainlevel | NOT RUN | no baseline; known stall |

Gates that held on every game:

- `tex_missing_base_color`: exact match on all five.
- `skin_pool_overflow_attempts`: 0 on all five.
- `skin_pool_max`: 1364 on all five.
- `light_count_directional`: exact match on all five.
- `bench_draws_batches` / `bench_draws_gpu_calls`: exact match on all five.
- The #4195 draw-split invariant (`batches ≤ raster ≤ cmds`, `gpu ≤ 2×batches`) held in every capture and in every committed TSV.

Informational rows (`tex_missing_all_slots`, never gating) all fell:

| Game | Before | After |
|------|--------|-------|
| fnv | 6 | 1 |
| fo3 | 12 | 0 |
| skyrim_se | 10 | 2 |

The two remaining Skyrim paths are the known control-character `textures/\bnor`
and `textures/cubemaps/ore_ebony_e.dds`.

`bench_draws_raster_cmds` is not a directional gate. In system-live it read:

| Game | Baseline | Now |
|------|----------|-----|
| fnv | 283 | 188 |
| fo3 | 123 | 108 |
| skyrim_se | 14 | 9 |

All are far below the 3000 parallel-sort branch.

## Findings

### RT-1: `BYROREDUX_FIXED_DT=0`, which the skill recommends, silently switches the bench to `renderer-static`, and every baseline was captured in `system-live`, so the draw-split rows can't be compared
- **Severity**: MEDIUM
- **Status**: NEW
- **Dimension**: audit infrastructure (`.claude/commands/audit-runtime/SKILL.md` Notes, `capture.sh`, baseline TSVs)
- **Description**: The skill's *Notes → Determinism* paragraph recommends
  `BYROREDUX_FIXED_DT=0` "when capturing tolerance metrics".
  `resolve_bench_selection` (`byroredux/src/bench.rs` ~L194) maps that env var
  to `BenchMode::RendererStatic`. `capture.sh` never passes `--bench-mode`, so
  without the env var the engine runs `system-live`, and every committed
  baseline was captured that way. The TSVs don't record the mode, so a capture
  taken per the skill's own advice is diffed against a baseline from a
  different mode, and nothing flags it.
- **Evidence**: Same build and same cells, captured minutes apart. The
  `bench:` draw split in each mode:

  | Game | `renderer-static` (`FIXED_DT=0`) | `system-live` (unset) | Baseline |
  |------|------------------------------|-----------------------|----------|
  | oblivion | 330/**78b/5c**, raster **132** | 330/20b/2c, raster 22 | 330/20b/2c, raster 22 |
  | fnv | 2204/**167b/36c**, raster 283 | 2197/109b/26c, raster 188 | 2110/109b/26c |
  | fo3 | 1579/**114b**/12c, raster 123 | 1581/100b/11c, raster 108 | 1581/100b/11c |
  | skyrim_se | 2458/**13b/3c**, raster 14 | 2457/9b/2c, raster 9 | 2342/9b/2c |
  | fo4 | 3969/**196b/13c**, raster **256** | 3964/248b/16c, raster 359 | 3964/248b/16c |

  Read against the baselines, the static pass would have filed four phantom
  draw regressions: batches +53 % on fnv, +290 % on oblivion, +44 % on skyrim
  and +14 % on fo3. In the static pass's own logs, the once-per-second
  `engine::stats` lines printed *after* the `bench:` line read 109b / 20b /
  100b / 9b again. `--bench-hold` puts the engine back into live
  wall-clock mode, so the split is tied to the frozen-dt window, not to the
  scene. Separately, the static-mode FO4 run exited before any
  `engine::stats` boundary, so its three `skin_pool_*` rows were not captured.
- **Contract conflict**: `BenchMode` in `bench.rs` documents `RendererStatic`
  as the mode for "deterministic regression gates only". It documents
  `SystemLive` as "combined system observation only; **never a regression
  gate**". The skill gates on exactly that mode today.
- **Open question (not a finding yet)**: why does a frozen `dt` push Oblivion
  from 22 to 132 raster commands and from 20 to 78 batches, with the same 330
  total? One candidate: a dt-driven state (fade, alpha or visibility ramp)
  never advancing, which leaves more commands in the blended raster prefix.
  That is unverified. It matters because `renderer-static` is the mode the code
  says gates should use.
- **Suggested Fix**: Pick one mode and pin it. Pass `--bench-mode` explicitly
  from `capture.sh`, record it in each TSV (e.g. a `bench_mode` row the diff
  checks exact-match first), and delete or rewrite the `FIXED_DT` advice in
  *Notes*. If the pinned mode is `renderer-static`, as `bench.rs` intends,
  re-capture all five baselines in one run each, and answer the open question
  first so the baselines don't bake in an artifact of the frozen dt.

### RT-2: Skyrim SE `WhiterunDragonsreach` `entities_total` is +16.4 % past its band, and its "held pending bisect" status has no open tracker
- **Severity**: MEDIUM
- **Status**: Existing condition, **untracked** (#3553 / #3554 closed 2026-09-02 by `a2a2168f`, which kept this cell's row unchanged "pending its own bisect")
- **Dimension**: runtime telemetry / ECS body-count
- **Description**: `entities_total` 8126 → 9461 (+16.4 %) and
  `skin_pool_live` 83 → 133.

  | Sweep | entities_total | skin_pool_live |
  |-------|----------------|----------------|
  | 2026-08-30 | 9363 | 133 |
  | 2026-09-11 | 9428 | 133 |
  | today | 9461 | 133 |

  The number keeps creeping, and it has now stayed outside the gate for
  17 days. `a2a2168f` closed both issues and kept this row at its old value,
  so no open issue tracks the bisect it deferred. Until someone does it, every
  sweep re-reports the same breach.
- **Evidence**: The render-load rows are still inside their gates:
  - `bench_draws_cmds` 2342→2457 (+4.9 %, inside ×1.1)
  - `bench_draws_batches` 9→9 and `bench_draws_gpu_calls` 2→2
  - `light_count_point` 28→28
  - `skin_pool_overflow_attempts` 0
  - `mesh_cache_failed_count` 9→0 (improved)

  `bench_fps` 161.9→96.5 (−40 %) is advisory only, and FPS has been noisy
  under xvfb (#1701).
- **Suggested Fix**: File a dedicated tracker. Bisect `entities_total` on this
  cell from the 2026-08-09 regen to HEAD, using `world.owners` for a per-class
  breakdown as #4124 did for Oblivion. Once explained, `--regen` the file.
  The same regen should also pick up the `mesh_cache_failed_count` 9→0
  improvement (see RT-4).

### RT-3: The skill's `light_count_point` definition disagrees with the committed Oblivion baseline
- **Severity**: LOW
- **Status**: NEW
- **Dimension**: audit infrastructure (SKILL.md Phase 3 / doc rot)
- **Description**: Phase 3 says to parse `light_count_point` from the
  `LightSource emitters: N` tally. That tally counts **every** emitter,
  directional ones included. Oblivion's dump today is `emitters: 10`: 8
  `kind=Point` rows plus 2 `kind=Directional` rows. The baseline stores
  `light_count_point 8`, which is the count of `kind=Point` rows. Following
  the text literally produces a false 8→10 exact-match failure. The other four
  cells have 0 directional emitters, so the two definitions only disagree on
  Oblivion.
- **Related doc rot in the same section**:
  - *"every one dumps `directional_color = [0.000, 0.000, 0.000]`"* no longer
    holds. FNV's `CellLightingRes` dumps `directional_color = [0.224, 0.208, 0.133]`.
  - The *Checked-in baselines* table is stale on three cells:

    | Cell | Table says | TSV says |
    |------|------------|----------|
    | Oblivion | 705 | 745 |
    | FO4 | 19 399 | 18 969 |
    | FNV | "7 342" | 7342 ✓ |

- **Suggested Fix**: Define `light_count_point` as the count of `kind=Point`
  rows, the same rule `light_count_directional` already uses. Drop the
  zero-directional-colour claim, and point the table at the TSVs instead of
  repeating their numbers.

### RT-4: Three improvements are sitting below loose `≤ baseline` gates, and #4124 is resolved but still open
- **Severity**: LOW
- **Status**: NEW (bookkeeping)
- **Dimension**: baseline hygiene
- **Description**:
  - **Loose mesh-cache gates.** `mesh_cache_failed_count` is now 0 on fo3
    (baseline 3) and skyrim_se (baseline 9). Both gates are `≤ baseline`, so a
    regression back to 3 or 9 failed parses would pass silently. The FO3 file
    has no hold reason, so it can be regenerated now. Skyrim's regen waits on
    RT-2.
  - **#4124 left open.** The Oblivion TSV was regenerated on 2026-09-13 with a
    full #4124 rationale (`world.owners` breakdown, cross-checked capture), but
    #4124 is still OPEN. Today's capture matches that regen exactly (745,
    330/20b/2c).
- **Suggested Fix**: `--regen` fo3 (all rows from one capture, per #4195),
  close #4124 with a pointer to the 2026-09-13 regen, and fold Skyrim's
  mesh-cache row into the RT-2 regen.

## Not run

- **Starfield**: no baseline, and the 2026-08-30 stall is still unresolved.
- **Playable-slice gates** (`p0-door-interaction.sh`,
  `p1-character-traversal.sh`, `p2-melee-core.sh`): not run. This sweep was not
  blessing a build.
