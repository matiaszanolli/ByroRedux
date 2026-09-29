# #5133: RT-2026-09-29-07: runtime `entities_total` +2.34 % on fo3 MegatonPlayerHouse (3543 → 3626)

**Labels**: low, bug, tech-debt, game:fo3

**Source report**: `docs/audits/AUDIT_RUNTIME_2026-09-29.md` (report ID `RT-7`)
**Severity**: LOW (tolerance metric drifted within ±5 %)
**Dimension**: Telemetry diff

## Location
`.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv` `entities_total 3543`

## Description
Just outside the ±2 % band (budget 71, delta 83). Not bisected. The FO3 run shows the same camera and player-body signature as the other games: COCMarkerHeading spawn, raster 123→1285, `skin_pool_live` 7→3. The most likely net is `a070baaad` (+) and `f87490826` / `5570c221c` (−).

## Evidence
`/tmp/audit/runtime/fo3*` capture at HEAD `9fcfdc3fc`.

## Impact
The FO3 ±2 % entity gate fails on every capture until regenerated.

## Related
RT-3, RT-5.

## Suggested Fix
Include it in RT-3's regeneration. Bisect only if it does not settle there.

Validated at HEAD 9fcfdc3fc: the FO3 TSV `entities_total 3543` row is unchanged since the last re-baseline.

