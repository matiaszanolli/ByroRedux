# NIF-D3-02: Per-game clean-rate matrix is stale-low; Starfield parse_real_nifs floors erode with it

**Issue**: #1900 · **Severity**: LOW · **Labels**: low, nif-parser, nif, bug
**Dimension**: Block Dispatch Coverage (measurement calibration) · **Filed from**: docs/audits/AUDIT_NIF_2026-07-06.md (nif-deep suite)
**Location**: docs/engine/nif-parser.md § "Per-game NIF coverage"; docs/engine/game-compatibility.md;
crates/nif/tests/parse_real_nifs.rs (Starfield min_clean floors)

## Description
Both matrices understate the live parser 2–4 points (Oblivion 96.24% doc vs 99.93% live; FO4 96.46 vs
100; FO76 97.34 vs 100; SF Meshes01 97.21 vs 100). parse_real_nifs.rs pins per-archive min_clean floors
to these rates. FO4 floors refreshed to 0.995 (#1457); Starfield NOT — Meshes01.ba2 min_clean:0.970
(line 142) vs live 100%, ~3 points unguarded headroom.

## Impact
(a) doc matrix mis-calibrates audit severity; (b) a Starfield regression re-introducing ~900 truncated
Meshes01 files would still pass the floor test.

## Suggested Fix
Refresh both matrices from a live 7-game sweep; tighten the 5 Starfield min_clean floors to #1457
treatment (live minus ~0.5%). Update Oblivion "~149 NetImmerse files" → "6 v3.3.0.13 marker files".

**Related**: #1457, #746/#747.
