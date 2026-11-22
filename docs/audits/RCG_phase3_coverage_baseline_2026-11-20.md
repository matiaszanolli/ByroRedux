# RCG Phase 3 — coverage baseline before the cache (2026-11-20)

Evidence for [radiance-cache-gi-coverage](../prd/radiance-cache-gi-coverage.md) Phase 3 (#7074), RCG-VAL-002 and the §8 coverage metric: eligible pixels whose traced share is non-zero ÷ eligible pixels, per scene and per ray quality tier. This is the **control**: the engine with no radiance cache, the footprint gate and the distance fade in place. The cache's own coverage is compared against it, and the §8 threshold is 99 % (Q6).

Raw rows (the gate's manifest format, 72 rows): [RCG_phase3_coverage_baseline_2026-11-20.tsv](RCG_phase3_coverage_baseline_2026-11-20.tsv). Poses and launch commands: [RCG_coverage_manifest.tsv](RCG_coverage_manifest.tsv).

## Coverage, percent of eligible pixels

| Scene | Tier 0 | Tier 1 | Tier 2 | Tier 3 | Eligible pixels (tier 3) |
|---|---|---|---|---|---|
| prospector (FNV Saloon) | 0 | 82.8 | 82.9 | 82.9 | 530,216 |
| whiterun (Skyrim Bannered Mare) | 0 | 30.8 | 30.8 | 30.8 | 920,226 |
| medtek (FO4 MedTekResearch01) | 0 | 35.6 | 35.6 | 35.6 | 308,852 |
| dugout (FO4 Dugout Inn) | 0.14 | 29.4 | 31.0 | 31.0 | 805,697 |
| fallons_downtown (FO4 Commonwealth, Downtown Fallon's) | 0.008 | 0.008 | 0.008 | 0.008 | 674,011 |
| fallons_galleria (FO4 Commonwealth, Galleria Fallon's) | 0 | 57.4 | 57.4 | 57.4 | 724,567 |

Every row fails the 99 % threshold. That is the expected state of the control, not a finding about the gate.

## Reading

- **Tier 0 is zero coverage** on every scene (dugout's 0.14 % is 1,097 pixels), as RCG-COV-004 records: tier 0 has no diffuse path allowance.
- **Tiers 1 to 3 differ by at most 1.6 points on any scene.** The footprint gate, not the tier, decides which pixels are traced, so raising the tier adds no coverage.
- **Downtown Fallon's is the owner's complaint in numbers.** 56 of 674,011 eligible pixels (0.008 %) carry a traced share at every tier, including tier 3. The Phase 0 telemetry on the same pose read `gi_traced=0` and about 1.4 million fragments culled by the gate.
- **The four interiors reach 31 to 83 % at tier 3**, the small Saloon room being the best case. Why they differ was not investigated.

## How it was measured

`scripts/m-light-gates.sh --gates RCG-COV --profile release`: per scene and tier, two captures at the manifest pose, `render.debug traced_share` and `render.debug gi_eligible` (the eligible-pixel mask, white where the primary surface meets the material conditions of GI eligibility), TAA with the jitter off, 64 frames averaged, seed offset 0, RTX 4070 Ti, 1280×720. Eligible is a mask value above 0.5. A pixel counts as traced when any channel of the averaged traced share is above zero, so a pixel traced in any of the 64 frames counts: these figures are an upper bound on per-frame coverage.

- **Two runs.** The four bench scenes ran at revision `78178a558a47-dirty` and the two Fallon's scenes at `c81e6adcebaf-dirty`. The first run's tree held the then-uncommitted `gi_eligible` view and gate, which landed unchanged as `1ee4a4ae5` and `c81e6adce`; the second run followed a fix to the gate. The first run launched the Fallon's scenes from the Data folder, where `--game fo4` finds no profile, and rendered an empty default scene: its Fallon's rows read 0 eligible pixels and are not used. The gate now launches `--game` scenes from the repository and stops when a scene fails to load or has no eligible pixel, instead of reporting 0 % coverage.
- **Poses.** The four bench scenes use their Phase 1b anchor poses. The two Fallon's poses are the Phase 0 measurement poses, not the owner's own wall (#7083).
- **Flicker in the mask.** On the FO4 interiors the averaged mask holds fractional values at animated surfaces (flickering emissives change eligibility between frames); the 0.5 threshold decides them.
