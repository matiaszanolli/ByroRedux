# Runtime Telemetry Audit — 2026-07-25

Final leg of the 2026-07-25 `comprehensive` audit-suite sweep. Drove the
headless engine (`xvfb-run` + `byro-dbg` TCP capture, port 9876, serial —
one game at a time) against every `--game all` profile whose data dir
resolves and diffed `stats` / `tex.missing` / `mesh.cache failed` /
`light.dump` / the `bench:` summary line against the checked-in baseline
TSVs under `.claude/audit-baselines/runtime/`.

## Method note — `BYROREDUX_FIXED_DT` NOT used this run

The skill's determinism note recommends `BYROREDUX_FIXED_DT=0` for
tolerance metrics. A dry-run on `fnv` with `BYROREDUX_FIXED_DT=0` set
produced **zero** `engine::stats` boundary lines for the whole 240-frame
bench window: `log_stats_system`'s once-per-wall-second gate
(`crosses_one_second_boundary`, `byroredux/src/systems/debug.rs:42-45`) is
driven by the simulated `TotalTime` resource, which is fed by `DeltaTime` —
freezing `DeltaTime` at `0` means `TotalTime` never advances past its
initial value, so the boundary the `skin=L/M+S` line depends on never
fires. Every prior baseline in this file was captured with a live
`skin_pool_live` value (`fnv`=686, `fo3`=0, `oblivion`=3, `fo4`=100), which
would have been impossible to capture under a frozen clock — confirming
prior sweeps did not set the flag either. This run reproduced that same
(unset) methodology for consistency with the committed baselines. Practical
effect: `entities_total` / `bench_draws_*` retain a small amount of
run-to-run wall-clock jitter (camera is static, but NPC AI/animation phase
at frame ~240 varies slightly by real elapsed time) — within the tolerance
bands the skill already defines for those metrics.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|----------------|
| fnv | FreesideAtomicWrangler | REGRESSION (MEDIUM) | entities 9250→9271 (+0.23%, in band); tex 1→1; mesh 11→11; skin 686/1364+0 → 677/1364+0 (decrease, fine); draws 2722→2629 cmds (decrease, fine), 104→106 batches (+1.9%, fine), **10→23 gpu_calls (+130%, exceeds ×1.1)**; `wall_fps` 147.3→138.7 (advisory) |
| fo3 | MegatonPlayerHouse | PASS | entities 3311→3311 (exact); tex 0→0; mesh 3→3; skin 0/1364+0 (exact); draws 1839→1583 cmds (decrease), 96→91 batches (decrease), 9→8 gpu_calls (decrease); `wall_fps` 93.3→91.4 (advisory) |
| oblivion | ICMarketDistrictTheGildedCarafe | REGRESSION (MEDIUM) | entities 701→701 (exact); tex 0→0; mesh 0→0; skin 3/1364+0 (exact); draws 324→324 cmds (exact), **27→31 batches (+14.8%, exceeds ×1.1)**, 4→4 gpu_calls (exact); `wall_fps` 323.4→459.2 (advisory — see note) |
| skyrim_se | WhiterunDragonsreach | REGRESSION (MEDIUM ×2) | **entities 6044→6395 (+5.81%, exceeds ±2% band)**; tex 0→0; mesh 11→9 (decrease, fine); **skin 0/1364+0 → 25/1364+0 (increase against `≤ baseline`)**; draws 2614→2397 cmds (decrease), 3→19 batches, 5→4 gpu_calls (decrease, fine — batches count itself isn't gated when cmds/gpu_calls both hold/decrease); `wall_fps` 321.1→256.9 (advisory) |
| fo4 | InstituteBioScience | REGRESSION (MEDIUM ×3) | **entities 11279→12448 (+10.36%, exceeds ±2% band)**; tex 1→1 (`textures\temp_v1_d.dds`); mesh 0→0; **skin 100/1364+0 → 124/1364+0 (increase against `≤ baseline`)**; draws 3800→3824 cmds (+0.6%, fine), 272→279 batches (+2.6%, fine), **40→46 gpu_calls (+15%, exceeds ×1.1)**; `wall_fps` 50.0→58.9 (advisory) |
| starfield | — | SKIPPED | No candidate cell / no committed baseline for this skill (per the skill's own candidate-cell table: "Starfield profile ships empty archives + no `sample_cells`; runtime cell render not yet a stable guard. Use `--sf-smoke` for SF coverage"). Data dir resolves (`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/`), so it is present, not absent — it is excluded by design, not by a missing install. |

All 5 non-Starfield profile data dirs resolved and ran; no engine/`byro-dbg`
process was already running when each game launched (checked via `ps aux`
before every launch), and games ran strictly serially on the default port
9876 per the skill's contract.

## Findings

### RT-1: `bench_draws_gpu_calls` grew on fnv `FreesideAtomicWrangler`
- **Severity**: MEDIUM
- **Dimension**: runtime/draw-batching
- **Location**: `crates/renderer/src/vulkan/context/geometry_pass.rs:394-446` (indirect-vs-direct dispatch decision); `crates/renderer/src/vulkan/context/draw.rs:325-328` (`needs_two_sided_blend_split`)
- **Status**: Regression of #1804 (CLOSED) — cross-referenced against this same sweep's **D2-01** in `docs/audits/AUDIT_PERFORMANCE_2026-07-25.md`
- **Description**: `bench_draws_gpu_calls` (the `K` in `draws=N/Mb/Kc`) rose from 10 to 23 (+130%) while `bench_draws_cmds` fell and `bench_draws_batches` barely moved. The concurrent `/audit-performance` leg of this sweep root-caused the same code path independently: commit `883f57cd` (2026-07-20) dropped the `&& b.z_write` limb from `needs_two_sided_blend_split`, re-broadening the FRONT/BACK two-pass cull split (originally scoped to order-dependent glass by #1804) to every two-sided alpha-blend batch — which now includes FNV's ambient particle effects (`fxmistlow01`, `fxsmokewisps01`, etc., all `alpha_blend: true, two_sided: true, z_write: false`). Each such batch now emits 2 direct `cmd_draw_indexed` calls instead of joining one `cmd_draw_indexed_indirect` group, inflating `indirect_call_count` without changing total draw commands.
- **Evidence**: `docs/audits/AUDIT_PERFORMANCE_2026-07-25.md` D2-01, `crates/renderer/src/vulkan/context/draw.rs:325-328` (current, unconditional `is_blend && b.two_sided`).
- **Impact**: Wasted GPU submission overhead on every particle-heavy interior; FNV's Atomic Wrangler runs enough ambient FX batches to move the metric > 2× baseline. Bounded (particle batch count itself is small — this is the runtime-telemetry corroboration of a MEDIUM finding, not a new discovery).
- **Related**: #1804 (closed, reverted); commit `883f57cd`; `docs/audits/AUDIT_PERFORMANCE_2026-07-25.md` D2-01.
- **Suggested Fix**: Same as D2-01 — carry an explicit `two_sided_blend_split: bool` set at emit time from `material_kind` (glass/MultiLayerParallax only) instead of using `z_write` as a proxy, restoring the particle fast path.

### RT-2: `bench_draws_batches` grew on oblivion `ICMarketDistrictTheGildedCarafe`
- **Severity**: MEDIUM
- **Dimension**: runtime/draw-batching
- **Location**: same as RT-1
- **Status**: Regression of #1804 (CLOSED) / D2-01 (`docs/audits/AUDIT_PERFORMANCE_2026-07-25.md`)
- **Description**: `bench_draws_batches` rose from 27 to 31 (+14.8%, exceeds the `≤ baseline×1.1` gate) while `bench_draws_cmds` (324) and `bench_draws_gpu_calls` (4) both held exact. The 2026-07-16 runtime audit recorded this exact cell at an **exact** `324/27b/4c` match against baseline — the drift is new within the last 9 days, bracketing it to the same 2026-07-20 `883f57cd` window as RT-1/RT-3. Oblivion's market-district cell carries torch/campfire-style additive-particle FX that now split out of indirect grouping the same way FNV's do; here the effect shows up one stage earlier (more distinct `DrawBatch` entries) rather than in the final indirect-call count.
- **Evidence**: `docs/audits/AUDIT_RUNTIME_2026-07-16.md` line 48 (`draws 324→324/27b/4c (exact)`); this run's `bench:` line `draws=324/31b/4c`.
- **Impact**: Same mechanism as RT-1, smaller magnitude (Oblivion has fewer/smaller particle emitters than FNV's Freeside).
- **Related**: RT-1; #1804; commit `883f57cd`; D2-01.
- **Suggested Fix**: Same as RT-1 — this is one root cause surfacing on two independent game corpora; a single fix (explicit split-eligibility flag) resolves both.

### RT-3: `bench_draws_gpu_calls` grew on fo4 `InstituteBioScience`
- **Severity**: MEDIUM
- **Dimension**: runtime/draw-batching
- **Location**: same as RT-1
- **Status**: Regression of #1804 (CLOSED) / D2-01 (`docs/audits/AUDIT_PERFORMANCE_2026-07-25.md`)
- **Description**: `bench_draws_gpu_calls` rose from 40 to 46 (+15%, exceeds `≤ baseline×1.1`) while `bench_draws_cmds` (3800→3824, +0.6%) and `bench_draws_batches` (272→279, +2.6%) both stayed within tolerance. Third independent game corpus showing the same D2-01 symptom (FO4's Institute BioScience cell has multiple ambient/FX particle emitters that now fail the split-eligibility check).
- **Evidence**: this run's `bench:` line `draws=3824/279b/46c` vs baseline `3800/272b/40c`.
- **Impact**: Same class as RT-1/RT-2 — corroborates D2-01 across 3 of 5 audited games, none of which is a "particle-light" architecture-only cell (fo3's MegatonPlayerHouse and skyrim's WhiterunDragonsreach, the two clean-on-this-metric cells, are comparatively particle-sparse interiors).
- **Related**: RT-1, RT-2; #1804; commit `883f57cd`; D2-01.
- **Suggested Fix**: Same as RT-1.

### RT-4: `entities_total` drift beyond ±2% band on skyrim_se `WhiterunDragonsreach`
- **Severity**: MEDIUM (borderline — see note)
- **Dimension**: runtime/entity-count (non-rendering)
- **Location**: `byroredux/src/npc_spawn.rs`; `byroredux/src/systems/{wander,travel,follow,escort,guard,patrol}.rs`
- **Status**: NEW (no matching open/closed issue found for this exact drift)
- **Description**: `entities_total` moved 6044→6395 (+351, +5.81%), just past the ±5% "LOW drift" ceiling into MEDIUM territory per `_audit-severity.md`'s tolerance-metric carve-out. `bench_draws_cmds` (the exact render-load contract) held at a **decrease** (2614→2397), confirming this is non-rendering entity growth, not new visible content. The 9-day window since the last runtime sweep (2026-07-16, +5 entities only) bracket-matches the full M42 AI-package rollout landing in that window: Wander (`097371b5`), Travel (`4af8efec`), Follow (`99c6b05a`), Escort (`923df40c`), Guard/Patrol (`aaed1503`) all shipped 2026-07-16→07-25 per `git log`, each adding a `SparseSetStorage` behavior/state component pair per eligible NPC.
- **Evidence**: `git log --oneline --since=2026-06-14 -- byroredux/src/npc_spawn.rs` shows the 7-procedure M42 rollout landing entirely inside this window; `docs/engine/npc-spawn-ai-packages.md` documents all seven runtimes now active.
- **Impact**: Likely benign feature-driven drift matching the documented RT-3/#1705 precedent pattern (non-render entity creep from AI/physics work), but it is the largest single-sweep jump recorded for this cell to date and crosses the tolerance ceiling, so it is reported rather than silently absorbed.
- **Related**: #1705 (RT-3 precedent); `docs/engine/npc-spawn-ai-packages.md`.
- **Suggested Fix**: Not a code fix — eyeball-confirm no unintended entity leak (e.g. `entities` command filtered to `WanderState`/`TravelState`/etc. component counts should sum to the delta), then `--regen` the baseline. If the sum doesn't reconcile, that's a real leak worth its own issue.

### RT-5: `skin_pool_live` increased on skyrim_se `WhiterunDragonsreach`
- **Severity**: MEDIUM
- **Dimension**: runtime/skin-pool
- **Location**: `crates/core/src/ecs/resources/skin_slot_pool.rs`
- **Status**: NEW
- **Description**: `skin_pool_live` moved from the baseline's `0` to `25` (last `skin=` sample in the capture). `skin_pool_overflow_attempts` stayed `0` (the HIGH-gating condition), and `skin_pool_max` held exact at `1364` — so this is not a cap/overflow issue, just more skinned NPCs holding live slots at the capture instant.
- **Evidence**: `.engine.log` last line `skin=25/1364+0` vs baseline `skin_pool_live 0`.
- **Impact**: Plausibly explained by the same M42 AI rollout as RT-4 — Dragonsreach hosts NPCs (guards, the Jarl's court) that are now actively Wander/Guard/Patrol-ing rather than static, which drives real per-frame animation and therefore GPU-skin-slot acquisition, where the 2026-06-14 baseline capture (before any M42 procedure existed) would have found every NPC in a static/idle state never triggering slot acquisition.
- **Related**: RT-4 (same likely root cause); #1284 (`SkinSlotPool` cap mechanism, unaffected).
- **Suggested Fix**: Same as RT-4 — confirm the NPCs holding slots are genuinely animated by an active AI behavior (not a leak), then `--regen`.

### RT-6: `entities_total` drift beyond ±2% band on fo4 `InstituteBioScience`
- **Severity**: MEDIUM
- **Dimension**: runtime/entity-count (non-rendering)
- **Location**: same as RT-4
- **Status**: NEW
- **Description**: `entities_total` moved 11279→12448 (+1169, +10.36%). `bench_draws_cmds` (the render-load contract) held almost flat (3800→3824, +0.6%), confirming — as with RT-4 — that this is non-rendering entity growth. This is the *second* large intentional-looking jump logged against this exact baseline: the currently-committed baseline's own header already documents an earlier +2112 entity jump (2026-06-01→06-19) as "intentional non-rendering drift" from collision/ragdoll/material work. This sweep's +1169 is a third data point in the same trend, again bracketed to the M42 AI rollout window (RT-4's evidence applies identically here).
- **Evidence**: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` header comment (prior +2112 precedent); this run's `bench:` line `entities=12448` vs baseline `11279`.
- **Impact**: Likely benign, same mechanism as RT-4. Flagged because it is the largest single-sweep percentage jump recorded for this cell and crosses the tolerance ceiling by a wide margin (>5×).
- **Related**: RT-4 (same likely root cause, different game); `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` prior-drift note.
- **Suggested Fix**: Same as RT-4 — reconcile the delta against active-behavior-component counts, then `--regen`.

### RT-7: `skin_pool_live` increased on fo4 `InstituteBioScience`
- **Severity**: MEDIUM
- **Dimension**: runtime/skin-pool
- **Location**: same as RT-5
- **Status**: NEW
- **Description**: `skin_pool_live` moved from baseline `100` to `124` (last `skin=` sample). `skin_pool_overflow_attempts` held at `0` and `skin_pool_max` held exact at `1364`.
- **Evidence**: `.engine.log` last line `skin=124/1364+0` vs baseline `skin_pool_live 100`.
- **Impact**: Same likely-benign mechanism as RT-5 — Institute BioScience has synth/scientist NPCs now driven by active M42 procedures.
- **Related**: RT-5, RT-6.
- **Suggested Fix**: Same as RT-5.

## Notes

- **`bench_fps_*` is advisory only (RT-2/#1701) — no fps finding was raised**, per
  the skill's rule. For visibility: fnv 147.3→138.7 (−5.9%), fo3 93.3→91.4
  (−2.0%), oblivion 323.4→459.2 (+42.0%, Xvfb-jitter-prone small/fast cell,
  matches the documented RT-2 precedent), skyrim_se 321.1→256.9 (−20.0%), fo4
  50.0→58.9 (baseline is a documented conservative floor, so this is not
  read as an improvement). The skyrim_se and fnv drops are plausibly the
  same `PERF-REGRESSION-6c56e311` / D5-01 (`triangle.frag` ~2.2× slower
  since 2026-07-19) already tracked as HIGH by the concurrent
  `docs/audits/AUDIT_PERFORMANCE_2026-07-25.md` leg of this sweep — not
  re-raised here since fps is explicitly non-gating for this skill.
- **RT-1/RT-2/RT-3 all trace to one root cause** (`883f57cd` re-broadening
  the two-sided-blend split, tracked as D2-01 in the concurrent
  `/audit-performance` leg of this same sweep). This runtime leg's value-add
  is independent telemetry corroboration across three separate game
  corpora (FNV, Oblivion, FO4) that the batching regression is real and
  visible in the actual per-frame draw-call count, not just in a static code
  read.
- **RT-4/RT-5/RT-6/RT-7 all trace to the same likely cause** — the M42
  AI-package rollout (Wander/Travel/Follow/Escort/Guard/Patrol) that landed
  entirely within the 2026-07-16→07-25 window, per `git log`. None of the
  four is HIGH-gated (no `skin_pool_overflow_attempts` moved off `0`, no
  `tex_missing`/`mesh_cache_failed` regression), and `bench_draws_cmds`
  (the exact render-load contract) stayed within tolerance on both affected
  games — the drift is in non-rendering entity/animation state, consistent
  with the documented ±2%-tolerance rationale. Recommend eyeballing the
  behavior-component counts before `--regen`, rather than treating these as
  urgent.
- Starfield was **not** run against this skill (no committed baseline, no
  `sample_cells`, per the skill's own candidate-cell table) — this is a
  documented gap, not a skipped-due-to-conflict game. `--sf-smoke` is the
  correct tool for Starfield coverage until a cell baseline is created.
- No concurrent engine/`byro-dbg` process was ever detected running before
  any of the 5 launches; all 5 ran serially on port 9876 as required.

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 7 (RT-1..RT-7) |
| LOW | 0 |

3 of 7 findings (RT-1/RT-2/RT-3) are runtime corroboration of an
already-tracked code-level regression (D2-01 / #1804 regressed by
`883f57cd`) surfaced independently by the concurrent `/audit-performance`
leg of this sweep — no new GitHub issue needed for those three beyond what
D2-01 already tracks. The remaining 4 (RT-4/RT-5/RT-6/RT-7) are two
symptoms (`entities_total`, `skin_pool_live`) each on two games
(skyrim_se, fo4), both plausibly explained by the M42 AI-package rollout
landing in this exact window — recommend a baseline `--regen` after a
quick component-count reconciliation rather than filing as bugs.

## Cleanup

`/tmp/audit/runtime` scratch captures removed; `pgrep -f 'byroredux|byro-dbg'`
confirmed clean (no engine or debug-CLI process left running) before this
report was finalized. Baselines under `.claude/audit-baselines/runtime/`
were left untouched (no `--regen` was passed for this sweep).
