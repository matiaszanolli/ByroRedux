# FNV-2026-08-26-D5-03

**Issue**: #3342
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 5 — NIF Parser Regression Guard
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `ROADMAP.md:571`

**Premise verified**: the cell reads *"100% (14 881 NIF meshes — this column is the NIF-mesh parse
rate over `Fallout - Meshes.bsa` …)"*. `cd9a5ef2` (in the delta window) moved
`parse_rate_fallout_nv` off `game.mesh_archive()` onto `open_all_mesh_archives`, and the gate at
HEAD certifies **20,746 NIFs across 11 archives** (base + 1.4 `Update.bsa` + 4 story DLC + 4
pre-order packs) — measured this run.

**Impact**: documentation drift only; the ROADMAP understates the certified corpus by 5,865 files
(28%) and names an archive scope the gate no longer uses. The number is quoted by every downstream
audit brief as the FNV baseline, so it propagates.

**Fix sketch**: update the cell to "100% (20 746 NIF meshes across 11 mesh-bearing archives —
base + Update.bsa + 4 story DLC + 4 pre-order packs)". FO3 (17,172 / 6 archives) and Skyrim SE
(32,709 / 2 archives) rows moved in the same commit and should be checked together.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
