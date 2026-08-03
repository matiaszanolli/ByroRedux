# RT-1: bench_draws_batches regressed on skyrim_se (baseline 3 → 8), same symptom class as #2215 but an untracked fourth corpus

**Filed from**: `docs/audits/AUDIT_RUNTIME_2026-08-03.md` (Runtime Telemetry Audit — 2026-08-03), finding RT-1
**Severity**: MEDIUM
**Labels**: medium, performance, renderer, bug
**GitHub issue**: #2351

## Summary

skyrim_se `WhiterunDragonsreach`'s `bench_draws_batches` telemetry has regressed from its baseline of 3 to 8, reproduced identically across two independent back-to-back runs today (`draws=2304/8b/2c` both times). This is the same symptom class as the already-open #2215 (post-batch-merge draw grouping not restoring after `#2165`'s fix) but on a corpus #2215 does not name — #2215 only covers fnv/oblivion/fo4.

## Location

- `byroredux/src/render/mod.rs`, `byroredux/src/render/static_meshes.rs` — draw-batch assembly, touched most recently by `b5d9f181` ("feat(render): add sorting for raster-visible draws and improve draw command handling", 2026-08-01)
- `crates/renderer/src/vulkan/context/draw.rs`, `crates/renderer/src/vulkan/context/geometry_pass.rs` — merge/indirect-grouping consumers

## Description

Baseline `bench_draws_batches` for skyrim_se/WhiterunDragonsreach is 3. The 2026-07-27 sweep measured 9 (not called out as its own finding then, folded into the general delta list). Today's sweep (repo HEAD `1ae86f62`) measured 8, reproduced identically across two independent engine launches — a stable, real behavior change, not small-count sampling noise (contrast with fo3's `gpu_calls` 9↔8↔10 wobble, correctly dismissed as noise at that scale by the 07-27 report).

Confirmed via `gh issue list`: #2215 ("RT-1: #2165's fix does not restore indirect grouping — fnv gpu_calls still 23, oblivion 31, fo4 48 at HEAD") names only fnv/oblivion/fo4. #2216 covers `entities_total`/`skin_pool_live` only. Neither names skyrim_se `bench_draws_batches`.

## Evidence

- Baseline TSV: `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` line 16 (`bench_draws_batches 3`)
- This sweep's two independent captures both show `draws=2304/8b/2c` in the `bench:` line

## Impact

Same class of impact as #2215 — the post-merge batch count growing while the cmds count falls (2614→2304 here) means draws that should combine into one indirect batch are not merging, adding avoidable per-frame CPU (sort/group) and GPU (extra indirect submits) overhead. The small absolute magnitude on this cell (3→8) bounds the blast radius today, but if it shares #2215's root cause in `883f57cd`, it will scale with scene complexity like the other three corpora do.

## Related

- #2215 — same symptom class, different corpora (fnv/oblivion/fo4)
- #2216 — this same cell (skyrim_se/WhiterunDragonsreach) also carries the tracked `entities_total`/`skin_pool_live` drift, now escalated (see comment posted on #2216)

## Suggested Fix

Fold this corpus into #2215's bisection work (`883f57cd` sub-change isolation) rather than treating it as fully disjoint — check whether the same reverted sub-change also restores skyrim_se to ~3 batches. If it does not move together with fnv/oblivion under that bisection, that is itself informative (rules out a single shared cause across all four corpora).

## Completeness Checks

- [ ] **SIBLING**: Check if this affects other games' draw-batching too, or is Skyrim SE-specific (fo3 and fo4 batch counts are currently fine/improved; fnv and oblivion are already tracked under #2215 — confirm whether skyrim_se shares #2215's root cause via the `883f57cd` bisection)
- [ ] **TESTS**: Baseline TSV (`.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv`) should be updated only once the real cause is understood, not just to hide the regression
