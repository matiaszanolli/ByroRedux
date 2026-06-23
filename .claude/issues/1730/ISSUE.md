# D3-FO3-01: FO3 36-byte XCLL trips a spurious "lighting may be mis-computed" warning

**Issue**: #1730
**Severity**: LOW
**Labels**: low, import-pipeline, legacy-compat, bug
**Dimension**: 3 — ESM Record Coverage (FO3-divergent)
**Location**: `crates/plugin/src/esm/cell/walkers.rs` — `XCLL_SIZES_FALLOUT_ERA` (line 47) / `xcll_size_sanity_warn` (line 60); decode at the `b"XCLL"` arm (~534)
**Source audit**: AUDIT_FO3_2026-06-23 (D3-FO3-01)

## Description
The canonical XCLL size set for `GameKind::Fallout3NV` is `XCLL_SIZES_FALLOUT_ERA = &[28, 40]`. Real `Fallout3.esm` ships 36-byte XCLL records on 17 interior cells (Oblivion-style tail: dir_fade@28 + fog_clip@32, no fog_power@36). Because 36 is absent from the canonical set, `xcll_size_sanity_warn` fires 17× during a vanilla parse with a message claiming the lighting may be mis-computed / cross-game injection. The data is vanilla and correct; the decode is per-field length-gated (#1312) and reads it correctly — only the canonical-size set and the warning gate are wrong. `FalloutNV.esm` emits zero non-canonical XCLL, confirming a clean FO3-vs-FNV divergence.

## Impact
17 misleading WARN lines per FO3 parse; misleads future FO3 cell-lighting debugging. No render impact.

## Related
#1312 (per-field XCLL gating); prior `AUDIT_FO3_2026-06-14.md` (mis-asserted XCLL identity with FNV).

## Suggested Fix
Add `36` to `XCLL_SIZES_FALLOUT_ERA` (→ `&[28, 36, 40]`); update `fo3_fnv_xcll_sizes_pinned` + doc comment. Decode unchanged.
