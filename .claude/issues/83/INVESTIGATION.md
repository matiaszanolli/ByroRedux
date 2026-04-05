# Investigation: #83 — Oblivion NIF variant detection fragile

## Problem
`NifVariant::detect()` has two issues:
1. v20.0.0.5 only recognized as Oblivion when `user_version_2 == 0`. If a tool writes
   v20.0.0.5 with non-zero uv/uv2 values, it falls through to wrong variants.
2. BSVER gap ranges (101-129, 156-169) map to Unknown instead of nearest known game.

## Fix
Move `version == V20_0_0_5` check before the match — this version is exclusively Oblivion.
Fill gap ranges: 101-129 → SkyrimSE, 156-169 → Fallout76 (closest known).
Add comprehensive edge case tests.

## Scope
1 file: `crates/nif/src/version.rs`
