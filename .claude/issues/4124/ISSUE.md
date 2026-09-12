### RT-2: Oblivion `ICMarketDistrictTheGildedCarafe` entities_total moved past the ±2% tolerance band for the first time since its 2026-08-26 regen

- **Severity**: MEDIUM
- **Dimension**: runtime telemetry / ECS body-count
- **Location**: `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv`
- **Status**: NEW
- **Description**: `entities_total` 705 → 745 (+5.67%), exceeding the ±2% tolerance band the skill defines for this metric (#1705/RT-3). This is the cleanest, smallest baselined cell (zero missing textures, zero mesh-cache failures both before and after), so it is an unusually legible signal.
- **Evidence**: `bench:` line: `entities=745 … draws=330/20b/2c bench_draws_raster_cmds=22 lights=10`. The render-load contract that exists specifically to distinguish "more bodies" from "more visible geometry" is untouched: `bench_draws_cmds` 325→330 (+1.5%, well inside ×1.1), `bench_draws_batches` 20→20 (exact), `bench_draws_gpu_calls` 2→2 (exact). `light_count_point`/`light_count_directional` (8/2) are also exact matches. Committed baseline row confirmed at `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv` (`entities_total 705`, last regenerated 2026-08-26 per #3288).
- **Impact**: none observed on rendering — every documented instance of this exact pattern on the other four baselines (RT-3/#1705, RT-8/#3554) turned out to be benign non-rendering body creep (collision/ragdoll/marker entities), and the draw-split evidence here points the same way. Flagged per protocol because the tolerance band is crossed, not because there is independent evidence of a real defect.
- **Related**: #1705 (RT-3, the ±2% tolerance band definition), #3554 (RT-8, the same creep pattern on a different cell).
- **Suggested Fix**: `git bisect` the entity count specifically on this cell between the 2026-08-26 regen commit and HEAD, the same way the FNV/FO4/Skyrim baseline headers already did for their own creep events, to confirm which subsystem added ~40 non-rendering entities to a 705-entity interior. If confirmed benign, `--regen` this one baseline row (or the whole file) once bisected — don't fold it into the general "known creep" bucket without a citation, since this is this cell's first breach.

## Completeness Checks
- [ ] **TESTS**: Once bisected, the baseline regen commit cites the root-cause commit the way sibling regen headers in this file do
