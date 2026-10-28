# RT-1: entities_total fell 16.7 % on fo4 InstituteBioScience and draw split improved past the parallel-sort threshold; no TSV refresh (also covers oblivion +4.4 % and fnv/fo3 draw-row drift, folded in from RT-2 and RT-7)

Labels: medium, tech-debt, bug, game:fo4, game:oblivion, game:fnv, game:fo3
Source: docs/audits/AUDIT_RUNTIME_2026-10-27.md

**Report**: `docs/audits/AUDIT_RUNTIME_2026-10-27.md` · **Findings**: RT-1 (primary), RT-2 and RT-7 folded in (same `--regen` fix) · **Severity**: MEDIUM · **Dimension**: Telemetry diff / baseline integrity
**Location**: `.claude/audit-baselines/runtime/{fo4-InstituteBioScience,oblivion-ICMarketDistrictTheGildedCarafe,fnv-FreesideAtomicWrangler,fo3-MegatonPlayerHouse}.tsv`
**Audited tree**: HEAD `fa6be4a6b` plus the orchestrator's one-line EXT-D6-2026-10-27-01 fix.

## Description
No baseline TSV has been refreshed since 2026-10-07 while ~1,700 engine commits landed. The baselines README requires the TSV refresh in the same commit as the engine change. Current committed TSVs (verified unchanged at HEAD) still hold the old values, so gating rows read red on intended fixes and loose `<=` draw rows can silently regress.

### RT-1 fo4 InstituteBioScience: entities 16885 -> 14061 (-16.7 %)
- Baseline: entities 16885; draws 3971/700b/108c, raster 3618; skin_live 132. Current: entities 14061; draws 2471/710b/110c, raster 2247; skin_live 258.
- Dominant step `777351ec1` (Fix #5484, precombine skip set from XCRI pairs, not XPRI): one-commit A/B 17266 -> 14237 entities, cmds 4002 -> 2376, raster 3649 -> 2152, batches 719 -> 690. Intended correctness fix. Many small commits moved entities both ways (16885->17266 before idx 287; 14237->14776->15165 through 10-14 (M47.2); 15049->15057 at #6077; 15057->14061 after 2026-10-20, not bisected).
- `skin_pool_live` doubled (132 -> 258), advisory but unrecorded.
- The raster prefix moved from above to below `DRAW_SORT_PARALLEL_THRESHOLD` (3000), so this cell no longer exercises the parallel draw-sort branch, which was the reason the 2026-09-30 regen recorded it.
- HEAD log: `502 REFRs skipped via FO4 PreCombined absorption`, `PreCombined: 13 hashes - 1722 entities spawned`, `21 NPCs spawned via pre-baked-FaceGen path`, `8 REFRs withheld by their XESP enable parent's state (#6077)`.

### RT-2 oblivion ICMarketDistrictTheGildedCarafe: entities 928 -> 969 (+4.4 %)
`1b45763fa` (Fix #5487, Oblivion's own NPC body table and race body skins) moves 928 -> 966 entities, 335 -> 336 cmds, 9 -> 10 skin_live (one-commit A/B, idx 290 -> 291); a further +3 landed between idx 307 and 615. Intended fix, no TSV refresh. HEAD log: `Player body: assembled from NPC_ 00000007 (Oblivion) ... meshes=20 equipment_parts=4`.

### RT-7 fnv / fo3 draw-row drift (LOW, within tolerance, unattributed)
fnv 2069/500b/120c -> 2069/503b/118c; fo3 1514/511b/65c raster 1285 -> 1515/516b/62c raster 1289. Already present at `011ad0c07` (2026-10-20), byte-identical to HEAD. gpu_calls improved on both, so the loose `<=` gate lets the old values return.

## Evidence
Bisect table in the report (`git rev-list --first-parent --reverse 2c36c29d8..HEAD`, per-commit builds in a separate `rtbisect` cargo profile). Captures went through `capture.sh`; the entities cross-check (`stats` vs `bench:`) delta was 0 on every kept capture.

## Impact
The gate is red on improvements, so the next real regression hides inside it; with the cmds/raster rows loose the old 3971/3618 values could return and still read green. No 3000+-raster interior baseline now exercises the parallel draw-sort branch.

## Related
RT-3 (skyrim_se, filed separately); prior instance AUDIT_RUNTIME_2026-10-03 RT-1 (#5235, closed); #5131 (closed).

## Suggested Fix
Run `/audit-runtime --game all --regen` in one renderer-static run. Write one `# regenerated:` block per TSV naming `777351ec1` (#5484) as the fo4 draw step and the multi-commit entity drift, `1b45763fa` (#5487) for oblivion. Decide whether another cell should carry the >3000-raster parallel-sort coverage. Do the skyrim_se regen after the RT-3 attribution.

## Completeness Checks
- [ ] **SIBLING**: All five TSVs refreshed in the same run (Starfield has no baseline: separate decision)
- [ ] **TESTS**: `cargo test -p byroredux bench::` baseline-schema tests still pass after the regen

