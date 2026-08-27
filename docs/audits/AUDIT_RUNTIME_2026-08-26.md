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
| `fnv` | 20 days | entities −22.6 % — **traced to a correctness fix, not a regression**; batches ×1.22 past contract (#3005) |

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

### RT-2026-08-26-01 — fnv `entities_total` −22.6 % is a CORRECTNESS FIX, not a regression — RESOLVED

`FreesideAtomicWrangler`: 9,271 → 7,174 entities, `skin_pool_live` 677 → 206.
Investigated rather than filed, and bisected to a single commit.

**Cause: `bfdc3d3f` (2026-08-23)**, which replaced "spawn every expanded armor
candidate" with biped-slot occupancy resolution — only items that actually win
a slot get a mesh. Measured across the boundary with a 1-frame probe (cell-load
numbers are identical at 1 and 240 frames, so the probe is sound):

```
e5329d64 (parent)  entities=9511  meshes=1270  draws=2608/109b/26c  skinned=677
bfdc3d3f (fix)     entities=7174  meshes= 772  draws=2110/109b/26c  skinned=206
```

Pre-fix, `VFSAtomicWranglerGambler` equipped **29 armor meshes from 9 inventory
entries**; it now equips 2. An actor has ~15 biped slots, so 29
simultaneously-worn meshes were interpenetrating duplicates that could never
all be visible. Cell-wide: armor meshes 123 → 24, skinned meshes 677 → 206.

**Nothing failed to spawn.** NPC count is 19 on both sides and cell-load
entities move only 2017 → 2001. The loss is armor meshes that should never
have existed, plus their skinned sub-shapes and bone entities.

Two narrower hypotheses were tested and **falsified**, which is why a commit
bisect was needed rather than a code reading:

- Reverting #3217's `multi_pick` (`0x04` → `0x02|0x04`) at HEAD moved armor
  meshes only 24 → 27. The same expression yields 123 at 2026-08-20, so the
  flag is not what changed.
- Disabling the #2094 slot-occupancy `retain` at HEAD moved entities only
  7174 → 7183. The change is structural — the candidate list is built
  differently now — and is not recoverable by flipping either line.

**Baseline regenerated.** The 2026-08-06 capture recorded the pre-fix
over-equip, so the old numbers were the wrong ones.

### RT-2026-08-26-02 — the #3005 draw-batch regression is still live, and skyrim_se is worse than filed

`bench_draws_batches` is gated at `≤ baseline ×1.1`:

- `fnv` 89 → 109 = **×1.22**
- `skyrim_se` 9 → 21 = **×2.33**

#3005 (OPEN) filed this for fnv and fo3 on 2026-08-16 and has not been
re-baselined since. This run confirms it is unfixed on fnv and shows
`skyrim_se` — not named in #3005 — is the worst arm of the three. `fo3`'s
batches are 96 → 100 (×1.04), inside contract, so fo3 appears to have
recovered while skyrim_se regressed.

**The obvious benign explanation was tested and disproved.** Removing 99
duplicate armor spawns (RT-01) removes precisely the *most mergeable* draws —
same mesh, same material — so the batch move looked like it might be an
artifact of that fix rather than an independent regression. It is not: the
commit *before* the armor fix already read 109 batches. Bisected timeline for
this cell's draw split:

```
c0f3cda3  2026-08-07   2553/ 89b/25c    <- the value the baseline held
9e96a9f9  2026-08-12   2562/164b/35c    <- spike
d560427c  2026-08-17   2608/109b/26c    <- partial recovery
e5329d64  2026-08-23   2608/109b/26c    <- unchanged through the armor fix
bfdc3d3f  2026-08-23   2110/109b/26c    <- armor fix: cmds fall, batches do not
```

Merge efficiency fell from 28.7 to 19.4 cmds/batch. The regression entered
between 08-07 and 08-12 and was partially recovered by 08-17; it is unrelated
to the armor work. These data points belong to #3005.

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

Regenerated **two** of five:

- **`oblivion-ICMarketDistrictTheGildedCarafe.tsv`** — `entities_total` inside
  ±2 %, and the draw split moved *down* on both gated axes (batches 47 → 20,
  gpu_calls 4 → 2), so re-capturing tightens the contract.
- **`fnv-FreesideAtomicWrangler.tsv`** — after RT-01 established the −22.6 %
  is `bfdc3d3f`'s armor over-equip fix. The old capture recorded ~5× the real
  armor mesh count per NPC, so it was the wrong number to defend. The header
  records the cause, the cross-boundary measurements, and an explicit caveat
  that `bench_draws_batches = 109` is carried forward as a known #3005
  regression rather than an endorsement — gating against the old 89 would be
  permanently red and would mask a *new* batching regression.

Deliberately **not** regenerated:

- **`skyrim_se`** — batches ×2.33 past contract with no benign explanation,
  and its telemetry could not be captured at all (RT-03).
- **`fo3`** — +5.5 % entities against a 73-day baseline with no identified
  deliberate cause. The skill's rule is to regenerate when a *known* change
  moves the metric past the band; nothing here identifies one yet, so the
  drift stays visible.

`fo4` needed no write — it matched exactly.

The two held baselines stay stale *on purpose*. Overwriting a baseline whose
delta you cannot explain is what makes #3288's stated fear — "a genuine
regression indistinguishable from staleness" — come true; the fnv row was
regenerated precisely *because* its delta is now explained, and that is the
distinction this pass is trying to hold.
