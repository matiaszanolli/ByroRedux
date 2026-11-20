# RCG Phase 1 — A7 by region, pre-cache control (2026-11-19)

Evidence for [radiance-cache-gi-coverage](../prd/radiance-cache-gi-coverage.md) Phase 1 (#7072), exit clause
"A7 is published at tiers 1 to 3 on the box rung split by region" (RCG-CACHE-005, RCG-VAL-001).

This is the **control**: A7 on the Lambertian box rung at HEAD `c25b6ec16`, with no radiance cache in the tree.
The cache's own A7 numbers are compared against it. Raw rows (value, noise floors, reference cache key):
[RCG_phase1_A7_control_c25b6ec16.tsv](RCG_phase1_A7_control_c25b6ec16.tsv).

Command: `scripts/m-light-gates.sh --gates A7 --out target/m-light-rcg-p1` (dev build, tier 3 default, 64 engine
frames, 1024 spp references, seeds 0 and 1 on both sides, Mitsuba 3.9.1 `cuda_ad_rgb`, RTX 4070 Ti 580.178.04,
reference cache warm). The manifest's revision reads `c25b6ec16ab1-dirty` because another session's
non-renderer edits (`byroredux/src/save_io`, `crates/plugin`, `crates/scripting`) were uncommitted in the tree; no
renderer, shader or scripts file was modified.

## GI energy deficit (1 − engine ÷ reference; 1.000 = no traced GI)

| Case | box_floor | box_back | box_ceiling |
|---|---|---|---|
| tier 0 | 1.000 | 1.000 | 1.000 |
| tier 1 | 1.000 | 0.956 | 1.000 |
| tier 2 | 1.000 | 0.931 | 1.000 |
| tier 3 | 1.000 | 0.931 | 1.000 |
| tier 3, GI clamp off | 1.000 | 0.914 | 1.000 |
| tier 3, RT-LOD gate off | 0.419 | 0.424 | 0.328 |

Combined noise floors on the deficit rows are at most 4.1e-4 (the RT-LOD-off floor row); the p95 rows reach 1.8e-3.

## Reading

- The footprint gate is the cut on this rung: with it off, the floor, back wall and ceiling carry 58 %, 58 %
  and 67 % of the reference's indirect energy at tier 3, against 0 %, 7 % and 0 % with it on.
- Tier 0 has no traced GI anywhere, as RCG-COV-004 records (#7076).
- Tiers 1 to 3 differ only on the back wall (0.956, 0.931, 0.931); the floor and ceiling are at 1.000 at every tier.
- The control equals the Phase 2 exit run of reference-lighting (`MLIGHT_phase2_exit_A7A8_a89ca367e.tsv`, rev 16)
  to three digits in every cell, so nothing between that commit and `c25b6ec16` moved A7.
- The `rel_error_mean` and `rel_error_p95` rows are in the TSV.

The budget the cache is held to is the owner's, written at Phase 2 entry (RCG-CACHE-005); nothing here is a pass or a fail.
