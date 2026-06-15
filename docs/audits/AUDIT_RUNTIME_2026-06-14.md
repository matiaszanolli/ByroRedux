# Runtime Telemetry Audit — 2026-06-14

Runtime arm of the `comprehensive` audit-suite sweep. Drives the headless
engine on each supported game's representative interior cell under Xvfb,
harvests visible-symptom telemetry (`stats` / `tex.missing` /
`mesh.cache failed` / `light.dump` + the `bench:` summary line), and diffs
it against the checked-in baselines under
`.claude/audit-baselines/runtime/`.

## Environment — dynamic portion RAN (device + game data present)

Unlike a code-only environment, this host has everything the skill needs,
so the drive→capture→diff loop was executed in full:

- **GPU**: NVIDIA GeForce RTX 4070 Ti (driver 580.159.03), Vulkan instance
  1.4.341. All three RT extensions present: `VK_KHR_ray_query`,
  `VK_KHR_acceleration_structure`, `VK_KHR_ray_tracing_pipeline`.
- **Headless**: `/usr/bin/xvfb-run` available; render nodes `/dev/dri/renderD128/129`.
- **Game data**: Oblivion, Fallout 3, Fallout NV, Skyrim SE, Fallout 4,
  Starfield all resolve under `/mnt/data/SteamLibrary/steamapps/common`.
- **Build**: `cargo build --release -p byroredux -p byro-dbg` — OK.

`--bench-frames 240 --bench-hold`, telemetry over `byro-dbg` (TCP 9876).
Runs were **serialised** (one engine at a time), deliberately diverging
from the skill's "up to 4 games in parallel" guidance — see RT-1, which is
the same port-collision the 2026-06-01 run hit but is still un-fixed in the
skill text.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| fnv | FreesideAtomicWrangler | **PASS** · minor stale-baseline drift | entities 9250→9350 (+100), skin_pool_max 1365→1364 (−1); all symptom metrics unchanged |
| fo4 | InstituteBioScience | **DRIFT (MEDIUM)** | entities 9167→11331 (+2164) on an exact-match metric; draw counts ~unchanged → non-rendering entities |
| oblivion | ICMarketDistrictTheGildedCarafe | **BASELINE CREATED** | first run — 701 ent, 0 tex.missing, 0 parse fails |
| fo3 | MegatonPlayerHouse | **BASELINE CREATED** | first run — 3311 ent, 0 tex.missing, 3 parse fails |
| skyrim_se | WhiterunDragonsreach | **BASELINE CREATED** | first run — 6044 ent, 0 tex.missing, **11 parse fails incl. 2 corrupted paths** |
| starfield | (default) | **N/A — documented gap** | empty profile archives → 6-entity spinning-cube fallback; no cell rendered |

**Headline:** No rendering regression on either committed guard. The FNV
guard is green (only the already-known RT-1/RT-1-2026-06-01 stale drift).
The FO4 entity count jumped +23.6% since its 2026-06-01 baseline, but draw
counts (`3800/280b/7c` vs `3800/272b/7c`) and the skin pool are unchanged,
so the extra entities don't render — attributable to intervening intentional
work (collision-only entity spawn `1c26bc25`, ragdoll M41.x `2a14b2b7`), not
a single bug. The most interesting *new* symptom this pass is on Skyrim: two
of the 11 `mesh.cache failed` keys are control-character-laced garbage
strings (RT-3) — a string-decode overrun masquerading as an ordinary parse
failure.

## FNV FreesideAtomicWrangler — full diff (the committed guard)

| metric | base | cur | direction | verdict |
|---|---|---|---|---|
| entities_total | 9250 | 9350 | exact | +100 — see RT-2 (stale baseline) |
| tex_missing_unique_paths | 1 | 1 (`grey.bmp`) | ≤ | PASS |
| mesh_cache_failed_count | 11 | 11 | ≤ | PASS (same 11 DLC marker/furniture NIFs) |
| light_count_directional | 1 | 1 | exact | PASS |
| skin_pool_live | 686 | 686 | ≤ | PASS (exact) |
| skin_pool_max | 1365 | 1364 | exact | −1 — see RT-2 |
| skin_pool_overflow_attempts | 0 | 0 | ==0 | PASS (healthy) |
| bench_fps_p50 / avg | 141.4 / 147.3 | 151.7 | ≥0.9× | PASS (advisory — Xvfb fps unreliable) |
| bench_draws_cmds | 2722 | 2921 | ≤1.1× | PASS (+7.3%, within tolerance) |
| bench_draws_batches | 104 | 104 | ≤1.1× | PASS |
| bench_draws_gpu_calls | 10 | 10 | ≤1.1× | PASS |

The two structural deltas (entities +100, skin_pool_max −1) are stable
across the parallel and solo re-runs, so they are real, not noise — but
they trace to commits that landed *after* the 2026-05-28 baseline, not to
any defect. The 2026-06-01 audit already flagged the −1 skin_pool_max drift
(then entities 9250→9249) as RT-1 and recommended `--regen`; that
regeneration was never committed, so the drift has only widened.

## FO4 InstituteBioScience — full diff (the committed guard)

| metric | base | cur | direction | verdict |
|---|---|---|---|---|
| entities_total | 9167 | 11331 | exact | **+2164 (+23.6%)** — see RT-4 |
| tex_missing_unique_paths | 1 | 1 (`textures\temp_v1_d.dds`) | ≤ | PASS |
| mesh_cache_failed_count | 0 | 0 | ≤ | PASS |
| light_count_directional | 1 | 1 | exact | PASS |
| skin_pool_live | 100 | 100 | ≤ | PASS |
| skin_pool_max | 1364 | 1364 | exact | PASS |
| skin_pool_overflow_attempts | 0 | 0 | ==0 | PASS (healthy) |
| bench_fps_p50 / avg | 50.0 | 75.5 | ≥0.9× | PASS (improved; Xvfb advisory) |
| bench_draws_cmds | 3800 | 3800 | ≤1.1× | PASS (exact) |
| bench_draws_batches | 272 | 280 | ≤1.1× | PASS (+2.9%) |
| bench_draws_gpu_calls | 7 | 7 | ≤1.1× | PASS |

Cell-load log: `'InstituteBioScience' loaded: 2692 entities` +
`PreCombined: 13 hashes — 1722 entities spawned` → `Scene ready: 11331
entities`. CSG opened cleanly (`32370 objects, 6841 chunks`), 79 REFRs
absorbed, 0 decode/CSG failures, 0 panics, 0 Vulkan errors.

## Findings

### RT-1: `audit-runtime` skill still endorses parallel runs that collide on debug port 9876
- **Severity**: MEDIUM
- **Dimension**: runtime-harness / audit-infra
- **Location**: `.claude/commands/audit-runtime/SKILL.md` (Phase 2: "Up to 4 games run in parallel (Xvfb auto-display lets them coexist)"); `crates/debug-server/src/listener.rs` (single bind, no rebind)
- **Status**: Regression of the 2026-06-01 run-note (documented there but never promoted to a skill fix)
- **Description**: The debug server binds TCP 9876 exactly once at startup and
  logs `Debug server failed to bind port 9876: Address already in use (os
  error 98)` if the port is taken — it never retries. When two engines launch
  in parallel (which the skill explicitly tells the operator to do), the
  second engine renders fine but its telemetry is permanently unreachable for
  that run, even after the first engine is killed (the second never rebinds).
  Reproduced live this pass: the parallel FNV+FO4 launch left FO4 with a dead
  debug server; `byro-dbg` against it got `Connection refused`.
- **Evidence**: `/tmp/audit/runtime/fo4-InstituteBioScience.engine.log`
  (first parallel run) — `ERROR byroredux_debug_server::listener: Debug
  server failed to bind port 9876: Address already in use (os error 98)`.
- **Impact**: Any `--game all` operator who follows the skill's parallel
  guidance silently captures telemetry for only the first game and either
  mis-attributes the first game's numbers to the second (the 2026-06-01
  failure mode) or gets a connection-refused. The whole point of this audit —
  per-game diffs — is defeated for N−1 games.
- **Related**: 2026-06-01 audit "Skill methodology fixes" §2; debug-server `listener.rs`.
- **Suggested Fix**: Either (a) make the skill run games strictly serially
  (what this pass did — clean), or (b) give the debug server a
  per-process port (e.g. `BYRO_DEBUG_PORT` offset by an index, already
  supported via env) and document the offset in the skill, and/or (c) make
  the listener retry-bind. The cheapest correct fix is to delete the
  "Up to 4 games run in parallel" line and serialise.

### RT-2: `bench_fps_*` and the exact-match structural metrics drift on an un-regenerated FNV baseline
- **Severity**: LOW
- **Dimension**: runtime-baseline staleness
- **Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` (`# regenerated: 2026-05-28`)
- **Status**: Existing: extension of RT-1 in `docs/audits/AUDIT_RUNTIME_2026-06-01.md` (never regenerated)
- **Description**: `entities_total` 9250→9350 (+100) and `skin_pool_max`
  1365→1364 (−1) are stable across re-runs, so real, but originate from
  commits that landed after the 2026-05-28 baseline (the 2026-06-01 audit
  already saw the −1 and the then-smaller entity drift and recommended
  `--regen`). The baseline was never regenerated, so the gap only widened.
  `bench_fps_*` is separately unreliable under headless Xvfb (151.7 this
  run vs 141.4 baseline vs 132.0 on the parallel run — same binary).
- **Evidence**: `/tmp/audit/runtime/fnv-FreesideAtomicWrangler.{telem.txt,engine.log}`;
  `bench: ... entities=9350 ... draws=2921/104b/10c`, `skin=686/1364+0`.
- **Impact**: The exact-match gate on `entities_total`/`skin_pool_max`
  false-positives every run until the baseline is refreshed; an operator
  could waste a bisect on a non-bug. `bench_fps_*` adds noise to the gate.
- **Related**: RT-1/RT-2 in 2026-06-01 audit.
- **Suggested Fix**: `/audit-runtime --game fnv --regen` to refresh the
  FNV baseline against current HEAD (commit the TSV diff alongside the
  engine change that moved it). Consider dropping `bench_fps_*` from the
  headless contract or marking it advisory in the skill (already noted but
  not actioned in the prior audit).

### RT-3: Skyrim `mesh.cache failed` contains corrupted control-character mesh paths (string-decode overrun)
- **Severity**: MEDIUM
- **Dimension**: nif-parser / import-pipeline (Skyrim path resolution)
- **Location**: surfaced via `mesh.cache failed` keys (`byroredux/src/commands.rs:540-564` enumerates `NifImportRegistry.core.cache`); origin is the Skyrim REFR/NIF mesh-path string decode (candidate: `crates/nif/src/` string read or `byroredux/src/cell_loader/references.rs` path assembly)
- **Status**: NEW (count was recorded as `11/296` in `docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md`, but the *corruption* of two of those paths was never called out)
- **Description**: Of the 11 failed-parse keys reported for
  WhiterunDragonsreach, nine are legitimate marker/effect/clutter NIFs that
  genuinely fail to import (e.g. `dummypotion01.nif`, `fxcobwebcorner01.nif`,
  `leantablemarker.nif`). Two are garbage: `meshes\-e\x03` and
  `meshes\j.\x01` — i.e. an embedded `0x03`/`0x01` control byte after a
  one/two-character stem. A NIF/REFR path string is being read past its real
  length (or before a missing terminator), so the load key itself is
  corrupt. The engine logs **zero** parse warnings for these, so the
  corruption is silent — only the cache dump exposes it.
- **Evidence**: `/tmp/audit/runtime/skyrim_se-WhiterunDragonsreach.telem.txt`:
  `"11 failed-parse paths:\n  meshes\-e\n ... \n  meshes\j."`.
  Stable: the byte pattern is short stem + control byte, the classic
  read-past-length signature.
- **Impact**: Whatever real Skyrim meshes those two REFRs reference never
  load (they resolve to a nonexistent garbage path → fallback). Because the
  byte-level overrun reads attacker-uncontrolled but file-driven data into a
  String key, it is also a latent correctness hazard for any code that keys
  off the path. Confined to Skyrim content; ~2 REFRs in this cell.
- **Related**: `docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md` (count only);
  `mesh.cache failed` subcommand.
- **Suggested Fix**: Trace the Skyrim mesh-path read with `tex.missing
  entities` / a `mesh.info` on the affected REFRs to recover their FormIDs,
  then byte-decode the offending REFR's mesh-path subrecord (the FNV stride-
  drift method in MEMORY `nif_v10x_stride_drift_resolved`). Likely a string-
  length field read with the wrong width or a missing NUL guard in the
  Skyrim REFR/NIF string reader. Pair with `/audit-skyrim`.

### RT-4: FO4 `entities_total` drifted +2164 against the committed exact-match baseline
- **Severity**: MEDIUM
- **Dimension**: runtime-baseline staleness (FO4 guard)
- **Location**: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` (`# regenerated: 2026-06-01`)
- **Status**: NEW (no prior report; baseline is exact-match and moved 23.6%)
- **Description**: `entities_total` 9167→11331 (+2164, +23.6%). The metric
  is graded exact-match, so any movement is a finding. However, draw counts
  (`3800/280b/7c` vs baseline `3800/272b/7c`), skin pool (`100/1364+0`),
  tex.missing (1), and mesh_cache_failed (0) are all unchanged — so the
  extra ~2164 entities **do not render**. Several intentional commits landed
  since 2026-06-01 that add non-rendering entities: `1c26bc25` ("exclude
  synthesized collision-only entities from RT BLAS" — i.e. collision-only
  entities are now spawned), ragdoll M41.x `2a14b2b7`, and the material/PBR
  overhaul `83d6a155`. This is stale-baseline drift from intentional work,
  the FO4 analogue of RT-2, not a regression bug.
- **Evidence**: `/tmp/audit/runtime/fo4-InstituteBioScience.{telem.txt,engine.log}`
  — `Scene ready: 11331 entities`; CSG opened `32370 objects, 6841 chunks`;
  1722 precombine entities; 0 CSG/decode failures. Reproduced identically
  in both the parallel and solo runs (11331 both times).
- **Impact**: The exact-match gate false-positives on FO4 every run until
  refreshed; risks masking a *future* real entity-count regression in the
  noise. No current rendering impact.
- **Related**: RT-2 (same staleness class on FNV); commits `1c26bc25`,
  `2a14b2b7`, `83d6a155`.
- **Suggested Fix**: `/audit-runtime --game fo4 --regen` to refresh the FO4
  baseline (and verify the +2164 is entirely collision-only / ragdoll /
  marker entities via `entities` filtering before committing, so the
  refreshed baseline isn't laundering a real bug). If the count should be
  bisected, the candidate commits above are the window.

### RT-5: Runtime-baseline README schema lists metric keys that no committed TSV uses
- **Severity**: LOW
- **Dimension**: doc rot (audit-infra)
- **Location**: `.claude/audit-baselines/runtime/README.md:24-34` (schema example)
- **Status**: NEW
- **Description**: The README's "Schema" block shows keys
  `tex_missing_entity_count`, `light_count_point`, and
  `bench_draw_calls_total` that appear in **no** committed baseline TSV and
  are **not** in the skill's Phase 3 metric contract. The skill itself notes
  `light.dump` only surfaces the one directional sun (no per-point tally) and
  the draw count is the three-way `N/Mb/Kc` split, not a single
  `bench_draw_calls_total`. So the README schema documents a contract the
  skill explicitly contradicts.
- **Evidence**: `.claude/audit-baselines/runtime/README.md` schema vs
  `fnv-FreesideAtomicWrangler.tsv` / `fo4-InstituteBioScience.tsv` (neither
  carries those three keys); SKILL.md Phase 3 quirks list.
- **Impact**: A future operator hand-writing a baseline from the README
  would emit keys the skill never diffs, silently dropping them. Pure doc
  rot, no runtime effect.
- **Related**: SKILL.md Phase 3; RT-2/RT-4 (baseline hygiene).
- **Suggested Fix**: Update the README schema block to the live key set
  (the 12 keys in the committed TSVs / Phase 3 table). One-line edit pass.

## BASELINE CREATED (candidate cells — first-ever runtime guards)

Three previously-unbaselined candidate cells were run clean and their
scalars committed as new baselines (Phase 4 "first run → copy to baseline"):

- `oblivion-ICMarketDistrictTheGildedCarafe.tsv` — 701 ent, **0** tex.missing,
  **0** parse fails, draws 324/30b/3c. The cleanest cell in the matrix
  (matches its `sample_cells` "known-good" rationale).
- `fo3-MegatonPlayerHouse.tsv` — 3311 ent, **0** tex.missing, 3 parse fails
  (all legit: a shack exit-door, supermutant bedding, an fx fill), draws 1839/96b/9c.
- `skyrim_se-WhiterunDragonsreach.tsv` — 6044 ent, **0** tex.missing, 11
  parse fails (**2 corrupted — see RT-3**), draws 2614/3b/5c. Baseline
  header carries a NOTE pointing at RT-3 so the corruption isn't silently
  laundered into a "known-good" guard.

These are diffable guards going forward; the Skyrim one ships with the RT-3
caveat baked into its header comment.

## Starfield — documented gap, not a finding

`--game starfield` with the shipped (empty) profile archives and no
`sample_cells` falls through to the 6-entity spinning-cube default scene
(`entities=6, meshes=6, textures=2`, 0 lights, draws 4/4b/4c). This exactly
matches the skill's stated "Starfield profile ships empty archives + no
sample_cells; runtime cell render not yet a stable guard. Use `--sf-smoke`."
No cell rendered → no baseline created → not a regression.

## Harness verification (static, independent of the dynamic run)

Confirmed the skill's wiring is internally consistent with current code, so
even where the dynamic run had quirks the contract is sound:

- `expand_game_profile_args` / `--game` / `--bench-frames` / `--bench-hold`
  all present in `byroredux/src/main.rs`; profile keys
  `fnv/fo3/oblivion/skyrim_se/fo4/starfield` match `assets/debug_profiles.toml`.
- `bench:` line format (`byroredux/src/main.rs:2275`) carries
  `entities=`, `draws={}/{}b/{}c`, `wall_fps` as the skill claims (note the
  suffix is `b`/`c` not the skill's prose `Mb/Kc`, but the N/M/K positions are
  identical — cosmetic only).
- `skin=L/M+S` emits to `engine::stats` once per wall-second
  (`byroredux/src/systems/debug.rs:40-47`); default `RUST_LOG=info`
  (`main.rs:187`) routes it to stderr.
- `stats` / `tex.missing` / `mesh.cache failed` / `light.dump` / `ping` /
  `quit` all live (`byroredux/src/commands.rs`, `tools/byro-dbg/src/main.rs`).
- **Re-confirmed the prior audit's `BYROREDUX_FIXED_DT=0` trap**: with
  `FIXED_DT=0` the once-per-second skin/stats line never fires
  (`total.floor() != prev.floor()` never crosses), so the three
  `skin_pool_*` baseline metrics are uncapturable. This run dropped
  `FIXED_DT` for all captures, which is why FNV/FO4/Oblivion show live skin
  counts. The skill's Notes still recommend `FIXED_DT=0` for tolerance
  metrics without warning it kills the skin contract — a doc gap (folded
  into RT-1's "skill text vs reality" theme; not separately filed).

## Next

- `/audit-publish docs/audits/AUDIT_RUNTIME_2026-06-14.md` to file RT-1, RT-3, RT-4, RT-5.
  (RT-2 is a `--regen` chore, not a code bug — fold into the same `--regen`
  commit that refreshes the FNV+FO4 baselines.)
- After acknowledging RT-2/RT-4, regenerate both committed baselines:
  `/audit-runtime --game fnv --regen` and `--game fo4 --regen`.
- RT-3 is the one to chase with code: `/audit-skyrim` + byte-decode the two
  corrupted REFR mesh-path subrecords.
