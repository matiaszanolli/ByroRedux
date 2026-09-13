# Batch: 4124, 4125, 4126, 4127

## #4124 — RT-2: Oblivion ICMarketDistrictTheGildedCarafe entities_total +5.67% past ±2% tolerance
- Severity: MEDIUM · runtime telemetry/ECS body-count
- Location: `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv`
- entities_total 705→745 (+5.67%); render-load contract (draws/batches/gpu_calls/lights) all exact/within
  tolerance — points to benign non-rendering entity creep (collision/ragdoll/marker), same pattern as
  RT-3/#1705, RT-8/#3554.
- Live-captured this session (`--game oblivion --cell ICMarketDistrictTheGildedCarafe --bench-frames 240
  --bench-hold`), cross-checked entities=745 on both the byro-dbg stats stream and the engine's own bench
  line (agrees with the issue's own numbers, confirms no #3560 mis-attribution). Additionally ran
  `world.owners` (not part of the standard capture) for a per-ECS-class breakdown: `tlas_instances`
  (rendered-instance count, also printed as `tlas=330` on the bench line) is 330 — the rendering-visible
  set grew by the same +5 the draw split already showed, not by ~40. Confirms benign non-rendering creep
  at the ECS-class level, beyond what the original filing's draw-split-only evidence showed.
- A full per-commit `git bisect` across the ~816 commits since the 2026-08-26 regen (each requiring a
  live engine rebuild+rerun) was NOT performed — out of this pipeline's `cargo test` scope. Regenerated
  the baseline row (705→745) citing this session's evidence; root-cause spawner remains uncited.
- Domain: ecs → byroredux-core (root work was baseline/telemetry archaeology, not a code bug)

## #4125 — RT-3: FO4 InstituteBioScience entities_total −2.22% past −2% tolerance floor
- Severity: LOW · runtime telemetry/ECS body-count
- Location: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`
- entities_total 19399→18969 (−2.22%); render-load evidence (draws flat/improved) argues against lost
  content. Suggested fix explicitly says: "No action needed now beyond noting the direction for the next
  sweep to compare against." Closed with a comment reflecting that; baseline row deliberately left
  un-regenerated (same "leave stale on purpose so evidence survives" policy already used for fnv/fo3/
  skyrim_se in the 2026-08-26 pass) since no action was requested.
- Domain: ecs → byroredux-core (telemetry note, not a code fix)

## #4126 — COORD-01: `--rotation-mode` CLI pre-clamp defeats its own library fallback
- Severity: MEDIUM · legacy-compat/coordinate-system
- Location: `byroredux/src/boot/mod.rs:293-303`
- `mode.min(3)` pre-clamps any out-of-range `--rotation-mode` value to 3 (one of the two explicitly-wrong
  diagnostic conventions) before it reaches `euler_zup_to_quat_yup_mode`'s own documented `_ =>` fallback
  (ship mode 1). Also a stale "Defaults to 0" comment (ship default has been mode 1 since 2026-05-26).
- Fix: pass mode through unclamped; fix stale comment; add regression test.
- Domain: binary → byroredux

## #4127 — COORD-02: `docs/engine/coordinate-system.md` XCLL call-path claim is stale
- Severity: LOW · legacy-compat/documentation
- Location: `docs/engine/coordinate-system.md:192-194`
- Doc claims XCLL lighting calls `euler_zup_to_quat_yup` directly; actually moved to
  `cell_loader/load.rs::xcll_direction_yup` (a dedicated spherical-to-vector helper, #3313-#3316,
  b78749aff) specifically because the quaternion route discarded azimuth.
- Fix: update doc section to describe the real `xcll_direction_yup` derivation.
- SIBLING: check `docs/engine/per-game-translation-survey.md`'s SCOL era-gate description too.
- Domain: documentation
