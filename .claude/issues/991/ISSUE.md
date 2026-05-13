# #991 — FNV-D1-NEW-01: per-block FNV baseline stale since #707

**Source**: `docs/audits/AUDIT_FNV_2026-05-12.md` § Dim 1 / Dim 5
**Severity**: LOW (hygiene; net parse fidelity unchanged)
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/991

## Location

`crates/nif/tests/data/per_block_baselines/fallout_nv.tsv:47` (committed 2026-04-24, `a9c7bc9`)

## Summary

`per_block_baseline_fallout_nv` fails on current `main` with `NiPSysBlock 17933 → 17887`. The 46 missing blocks are now dispatched into a new `NiPSysColorModifier` bucket per #707 / `ff23881` (2026-05-01). Total parse output unchanged (17933 = 17887 + 46). The baseline TSV was never regenerated when #707 landed.

## Fix

```sh
BYROREDUX_REGEN_BASELINES=1 cargo test --release -p byroredux-nif \
  --test per_block_baselines per_block_baseline_fallout_nv -- --ignored
```

Commit the one-line TSV diff.

## Cross-game spillover (separate work)

FO4 / FO76 / Skyrim LE / Skyrim SE baselines have the same staleness pattern (likely same #707-class redirect, benign — same regen fixes them). Oblivion and Starfield show categorically different drifts (UNKNOWN-grew rows) — file separate findings before any regen.
