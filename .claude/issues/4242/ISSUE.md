# FO4-D6-02: TXST override comments cite XPRD as a texture-override REFR sub-record; it is not one

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4242

**Severity**: LOW
**Dimension**: 6 — ESM Architecture Records (SCOL/MOVS/PKIN/TXST)
**Location**: `crates/plugin/src/esm/cell/support.rs:474`, `crates/plugin/src/esm/cell/mod.rs:868,1182`
**Status**: NEW

**Description**: Three comments (inherited from #357) say "REFR XTNM/XPRD overrides". `XPRD` is "Patrol Data" (unrelated to textures) and has no parse arm anywhere in the crate. The real FO4 override mechanism is `XATO`/`XTXR` (both implemented and tested), plus `XTNM` (also real).

**Evidence**: Confirmed in current code — all three cited sites (`support.rs:474`, `mod.rs:868`, `mod.rs:1182`) still read "XTNM/XPRD overrides"; no `XPRD` parse arm exists anywhere under `crates/plugin/src/esm/`.

**Impact**: None on behavior — purely a misleading citation that could send a contributor looking for a nonexistent parsing gap.

**Suggested Fix**: Replace "XTNM/XPRD" with "XTNM/XATO/XTXR" in the three comments.

## Completeness Checks
- [ ] **TESTS**: N/A (documentation-only fix)
