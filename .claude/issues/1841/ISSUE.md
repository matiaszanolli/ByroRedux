# NIF-D3-01: 5 of 7 per-block baseline TSVs are stale — opt-in per-type regression gate sits RED (false-positive)

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1841
**Labels**: bug, nif-parser, low

**Severity**: LOW (test infrastructure; no parser defect)
**Dimension**: Block Dispatch Coverage
**Location**: `crates/nif/tests/data/per_block_baselines/{fallout_3,fallout_nv,fallout_4,fallout_76,skyrim_se}.tsv`
**Status**: NEW (no matching open issue; AUDIT_NIF_2026-06-14 recorded it only as a "tooling note"; the 06-23 audit quoted the checked-in TSVs without checking live-run parity)

## Description

**Game Affected**: FO3, FNV, FO4, FO76, Skyrim SE (Oblivion + Starfield TSVs are current)

The five TSVs were last regenerated 2026-05-15/16 (verified: `fallout_nv.tsv` git date 2026-05-16). The `BSPSysSimpleColorModifier` typed promotion (#1345, `7dacf9e6`, 2026-05-30) moved that type's histogram key out of the opaque `NiPSysBlock` aggregate (the struct now registers its own `block_type_name`), so a live run reports `NiPSysBlock` PARSED-shrank vs. these baselines and the `per_block_baselines` test fails on all five games for a non-regression reason. Because the test is opt-in (`--ignored`, needs game data), it sits silently RED — the memory-note "FO76 silently RED on NiPSysBlock" generalized to 5 games. The checked-in TSVs themselves read unknown=0.

## Evidence

TSV git dates vs. #1345 date — confirmed live:
```
fallout_3.tsv:   2026-05-16
fallout_nv.tsv:  2026-05-16
fallout_4.tsv:   2026-05-15
fallout_76.tsv:  2026-05-16
skyrim_se.tsv:   2026-05-16
oblivion.tsv:    2026-06-15   (post-dates #1345, unaffected)
starfield.tsv:   2026-06-13   (post-dates #1345, unaffected)
```
Commit `7dacf9e6` ("Fix #1345: capture BSPSysSimpleColorModifier inline particle colors") is dated 2026-05-30 — after the five affected TSVs, before the two unaffected ones. `common/mod.rs::record_scene_blocks` keying (parsed → `block_type_name()`, unknown → header name); AUDIT_NIF_2026-06-14 verified the exact conservation (−1484 `NiPSysBlock` = +1484 typed on FO3).

## Impact

While RED, the per-type regression gate is effectively disabled for 5 of 7 games — a real unknown-growth regression on those games would be buried in expected stale noise, and the trained "just regen" response could launder it into a new baseline.

## Suggested Fix

`BYROREDUX_REGEN_BASELINES=1 cargo test -p byroredux-nif --test per_block_baselines -- --ignored` on the data machine, verify the diff is exactly the #1345-shaped key move (`NiPSysBlock` −N / typed +N per game, unknown column all-zero), commit the 5 TSVs. Consider filing a tracking issue so it survives until a data-machine session.

## Completeness Checks
- [ ] **TESTS**: Regenerating the baselines re-arms the opt-in per-type regression gate for 5 of 7 games (this issue's entire purpose)
