# Runtime Telemetry Audit — 2026-08-26

## Scope and execution mode

**Live headless-engine comparison pass: EXECUTED, all five baselined cells.**

This closes #3288, which observed that four of the five runtime baselines had
gone 18–71 days stale against 682+ intervening commits and recommended a live
`--game all` re-run "the next time no engine/`byro-dbg` instance is running,
prioritizing `fo3` first". `pgrep -x byroredux` was empty at dispatch, so
Phase 2–4 ran for real — unlike the 2026-08-16 and 2026-08-24 sweeps, both of
which had to skip them under the no-parallel-engine rule.

Method per `audit-runtime/SKILL.md` Phase 2–3: `xvfb-run` headless launch,
`--bench-frames 240 --bench-hold`, wait for the `bench-hold:` notice, then
`byro-dbg` capture of `stats` / `tex.missing` / `mesh.cache failed` /
`light.dump`. Serial, one engine at a time on the single fixed debug port.

> **Two orchestration hazards worth recording**, since both produced
> convincing false failures before being identified:
>
> 1. **Attach on `bench-hold:`, never on a `byro-dbg` ping.** The debug server
>    binds during cell load, so a ping succeeds mid-bench; capturing there and
>    tearing down yields no `bench:` line at all and looks like an engine that
>    never benched.
> 2. **Reap the previous engine before launching the next.** A leftover holds
>    port 9876, the next launch logs `Address already in use`, and its
>    telemetry is silently unreachable for the whole run (the RT-1 / #1619
>    mis-attribution hazard, hit here from the other direction). Note also that
>    `pkill -f byroredux` kills the orchestrating script itself — its own argv
>    matches the pattern — so teardown must resolve PIDs and kill those.

## Result summary

| Game | Baseline age | Verdict |
|---|---|---|
| `fo4` | 4 days | **PASS — exact match on every metric** |
| `oblivion` | 20 days | PASS, within band; draw split improved |
| `fo3` | 73 days | entities +5.5 % (outside ±2 %); mesh-cache failures cleared |
| `skyrim_se` | 17 days | entities +3.5 % (outside ±2 %); **batches ×2.3 past the ×1.1 contract**; device-lost during hold |
| `fnv` | 20 days | **entities −22.6 %** (a gating drop); batches ×1.22 past contract |

`fo4`'s exact reproduction is the load-bearing control here: its baseline is
the freshest (2026-08-22) and every one of `entities_total`,
`bench_draws_cmds`, `bench_draws_batches`, `bench_draws_gpu_calls`,
`skin_pool_live`, `tex_missing_unique_paths` and `mesh_cache_failed_count`
came back identical. The capture path is therefore sound, and the deltas on
the other four are properties of those cells, not of the harness.

## Measurements

| Metric | fo3 | fnv | oblivion | skyrim_se | fo4 |
|---|---|---|---|---|---|
| `entities_total` | 3311 → **3492** | 9271 → **7174** | 701 → 705 | 8126 → **8411** | 18256 → 18256 |
| | +5.5 % | **−22.6 %** | +0.6 % | +3.5 % | 0.0 % |
| `bench_draws_cmds` | 1839 → 1581 | 2553 → 2110 | 324 → 325 | 2342 → 2413 | 3949 → 3949 |
| `bench_draws_batches` | 96 → 100 | 89 → **109** | 47 → 20 | 9 → **21** | 296 → 296 |
| `bench_draws_gpu_calls` | 9 → 11 | 25 → 26 | 4 → 2 | 2 → 4 | 16 → 16 |
| `tex_missing_unique_paths` | 0 → 0 | 1 → 1 | 0 → 0 | (uncaptured) | 1 → 1 |
| `mesh_cache_failed_count` | 3 → **0** | 11 → **0** | 0 → 0 | (uncaptured) | 0 → 0 |
| `skin_pool_live` | 0 → 7 | 677 → **206** | 3 → 4 | 83 → 86 | 248 → 248 |
| `skin_pool_overflow_attempts` | 0 | 0 | 0 | 0 | 0 |

`light_count_directional` is a constant 1 wherever `CellLightingRes` dumps at
all (the skill's own Phase 3 note), and was 1 on every captured game.

## Findings

### RT-2026-08-26-01 — fnv `entities_total` dropped 22.6 %, with `skin_pool_live` down 70 %

`FreesideAtomicWrangler`: 9,271 → 7,174 entities, and the skinned-actor pool
went 677 → 206 live slots. The skill's own tolerance note is explicit that a
drop past −2 % gates, because "entities failing to spawn is a real
regression" — this is eleven times that band, and the correlated skin-pool
collapse points at actors specifically rather than at benign non-render body
drift.

Not re-baselined. Doing so would erase the only evidence that this happened.

Two things argue against a pure-regression reading and need checking before
this is called a bug: `mesh_cache_failed_count` went 11 → 0 over the same
window (fewer NIFs failing to load, not more), and `bench_draws_cmds` fell
2553 → 2110, which is *within* contract rather than past it. A cell whose
actor population genuinely shrank and whose meshes all now load is a strange
shape for a spawn regression. The FNV baseline predates the #2371/#2372
exterior-streaming tranche that #3288 flagged.

### RT-2026-08-26-02 — the #3005 draw-batch regression is still live, and skyrim_se is worse than filed

`bench_draws_batches` is gated at `≤ baseline ×1.1`:

- `fnv` 89 → 109 = **×1.22**
- `skyrim_se` 9 → 21 = **×2.33**

#3005 (OPEN) filed this for fnv and fo3 on 2026-08-16 and has not been
re-baselined since. This run confirms it is unfixed on fnv and shows
`skyrim_se` — not named in #3005 — is the worst arm of the three. `fo3`'s
batches are 96 → 100 (×1.04), inside contract, so fo3 appears to have
recovered while skyrim_se regressed.

### RT-2026-08-26-03 — skyrim_se `WhiterunDragonsreach` loses the Vulkan device during bench-hold

```
ERROR byroredux::app_frame] Draw failed: wait_for_fences:
  The logical device has been lost.
```

Reproduced 3/3 under `xvfb-run`, including a clean solo run with no port
contention and a warm pipeline cache. The engine completes its 240-frame
bench window and prints a valid `bench:` line, then loses the device during
the post-bench hold — so the structural metrics above are trustworthy, but
`byro-dbg` telemetry (`tex_missing_unique_paths`, `mesh_cache_failed_count`)
could not be captured for this cell in any of the three attempts.

No fix attempted. Per `feedback_speculative_vulkan_fixes`, a device-lost whose
failure mode is invisible to `cargo test` wants RenderDoc, not a reasoned
patch. Filed as an observation with a reproduction, nothing more.

### RT-2026-08-26-04 — fo3 `entities_total` drift has grown, superseding #2521

#2521 (OPEN) records fo3 `entities_total` as "marginally exceeding the ±2 %
tolerance band". It is now +5.5 % (3311 → 3492) against a 73-day-old
baseline — no longer marginal. `mesh_cache_failed_count` 3 → 0 over the same
window is a genuine improvement.

## Baseline actions

Regenerated: **`oblivion-ICMarketDistrictTheGildedCarafe.tsv`** only. Its
`entities_total` is inside ±2 %, and its draw split moved *down* on both
gated axes (batches 47 → 20, gpu_calls 4 → 2), so re-capturing tightens the
contract rather than laundering a regression.

Deliberately **not** regenerated: `fnv`, `fo3`, `skyrim_se`. Each carries at
least one metric that is past a gating threshold in the wrong direction.
Overwriting them is what would make #3288's stated fear — "a genuine
regression indistinguishable from staleness" — actually come true, one commit
after the audit that was supposed to prevent it. `fo4` needed no regeneration:
it matched exactly.

The three stale baselines stay stale *on purpose* until the findings above are
resolved, and the measurements in this report are the record of what current
HEAD actually produces.
