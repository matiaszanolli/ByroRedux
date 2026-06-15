# D4-01: FNV SCOL records (98) silently dropped by the is_fo4_plus gate → 1084 unresolvable REFRs

**Issue**: #1538 · **Severity**: HIGH · **Labels**: high, import-pipeline, legacy-compat, bug
**Source**: AUDIT_FNV_2026-06-14 (D4-01 == D8-01) · **Status when filed**: NEW, CONFIRMED

## Location
- `crates/plugin/src/esm/records/mod.rs:175-242` (gate + `b"SCOL" if is_fo4_plus` arm @228 + skip arm @229)
- mislabeled doc `crates/plugin/src/esm/records/scol.rs`
- consumer `byroredux/src/cell_loader/refr.rs` (`expand_scol_placements`)

## Description
The `is_fo4_plus` gate is `Fallout4 | Fallout76 | Starfield`. The `b"SCOL"` arm parses only when `is_fo4_plus`; for FNV the fallback logs "SCOL is FO4+ only — skipping" and `skip_group`s. The gate comment claims SCOL is FO4-introduced — false. PKIN/MOVS/MSWP genuinely are FO4+; SCOL is Gamebryo-Fallout (Oblivion 0, FO3 54, FNV 98, Skyrim 0).

## Evidence (real data)
FalloutNV.esm: 1 SCOL group, 98 SCOL records; 1084 REFRs reference those base form IDs; `Fallout - Meshes.bsa` ships 137 cached `meshes\scol\*.nif`. First FNV SCOL (`0017B667`) DATA byte-identical to FO4 layout `parse_scol_group` decodes. Both `index.scols` and cached-`CM*.NIF` `index.statics` SCOL entries empty for FNV.

## Impact
1084 SCOL placements (road segments, guardrails, pylons, debris LOD clusters) render as nothing. Predominantly exterior — interior Prospector bench unaffected. FO3 equally affected (54 SCOL).

## Suggested Fix
Widen the SCOL gate: parse when `is_fo4_plus || matches!(game, GameKind::Fallout3NV)`. Keep PKIN/MOVS/MSWP FO4+. Correct `mod.rs:175-177` + `scol.rs` doc comments. Add FNV SCOL-count assertion (`index.scols.len() == 98`) to `parse_real_esm.rs`.
