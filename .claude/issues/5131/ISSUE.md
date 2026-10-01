# null: RT-2026-09-29-05: runtime `entities_total` outside ±2 % on fnv (−5.8 %), oblivion (+25.4 %), fo4 (−11.0 %) — three intentional content changes landed without a TSV refresh

labels: bug, medium, tech-debt
state: OPEN

**Source report**: `docs/audits/AUDIT_RUNTIME_2026-09-29.md` (report ID `RT-5`)
**Severity**: MEDIUM
**Dimension**: Telemetry diff (tolerance metric) / baseline integrity

## Location
`entities_total` rows in `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` (7414), `oblivion-ICMarketDistrictTheGildedCarafe.tsv` (745), `fo4-InstituteBioScience.tsv` (18969)

## Description
Baseline → current: fnv 7414 → 6985 · oblivion 745 → 934 · fo4 18969 → 16885.

The bisect assigns each step:
- FNV: `5570c221c` −92 (dismemberment caps, with −92 draws and −92 `skin_pool_live`), then about +195 in a window containing `a070baaad` (player body), then `f87490826` −532 (Initially Disabled refs withheld).
- Oblivion: `a070baaad` +177 (the baseline 745 vs `cb44d99f6`'s 757 is the known in-band creep).
- FO4: +187 in the `a070baaad` window, then `f87490826` −2301 (with `skin_pool_live` 229 → 132).

Each is an intended behaviour change. None refreshed the TSV, against the baselines README's same-commit rule.

## Evidence
Bisect table in the report.

## Impact
The ±2 % entity gate is inert until regenerated; every future capture will "fail" for these known reasons and bury a real move. The `f87490826` drop is also a behaviour change worth a visual spot check (are quest-gated actors now absent where the game also hides them?) — not verified.

## Related
RT-3, RT-6, RT-7, #4813, #4814, #4820, FNV-2026-09-29-D6-01 (#5067).

## Suggested Fix
Fold into RT-3's single regeneration and name all three commits in the `# regenerated:` header.

Validated at HEAD 9fcfdc3fc: the three TSV `entities_total` rows are unchanged since `49ea8ab96` / `fa4453ed0`.


