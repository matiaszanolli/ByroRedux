# NIF-D3-01: Oblivion per-block TSV baseline is stale-high on NiUnknown

**Issue**: #1899 · **Severity**: LOW · **Labels**: low, nif-parser, nif, bug
**Dimension**: Block Dispatch Coverage · **Filed from**: docs/audits/AUDIT_NIF_2026-07-06.md (nif-deep suite)
**Game**: Oblivion (NIF v20.0.0.5, bsver < 11; sizeless — no block_sizes)
**Location**: crates/nif/tests/data/per_block_baselines/oblivion.tsv

## Description
oblivion.tsv (regenerated 2026-06-15) records NiMaterialProperty (row 30) and NiTexturingProperty
(row 53) with a residual NiUnknown 1; live parser (2026-07-06) emits 0 across Oblivion - Meshes.bsa.
#1840/#1841 regenerated the other 5 TSVs but left oblivion.tsv + starfield.tsv untouched.
compare_histograms only fails on unknown-GROWTH or parsed-SHRINKAGE, so a 1→0 unknown drop passes
silently → stale-high ceiling never self-corrects.

## Impact
Oblivion is the sizeless cascade-risk game (no block_sizes anchor) — worst place for a masked
NiUnknown regression. Ceiling stuck at 1 vs live 0 = the exact regression-net hole #1841 closed
elsewhere, still open on the highest-risk game.

## Suggested Fix
BYROREDUX_REGEN_BASELINES=1 cargo test -p byroredux-nif --test per_block_baselines -- --ignored;
verify the diff is exactly the two unknown 1→0 drops (no parsed shrinkage); commit. Same for starfield.tsv.
