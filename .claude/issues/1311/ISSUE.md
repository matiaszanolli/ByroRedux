# #1311 -- OBL-D3-02: Oblivion 36-byte XCLL drops fogDirFade+fogClipDist

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: MEDIUM | **Dim 3** — ESM Record Coverage
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D3-2026-05-28-02)

**Location**: `crates/plugin/src/esm/cell/walkers.rs:508-516` (gate `len >= 40`); `walkers.rs:15-16,27` (size doc)

**Issue**: Oblivion XCLL records with a 36-byte body (a valid TES4-specific size in `XCLL_SIZES_OBLIVION = &[28, 32, 36]`) silently drop `fogDirFade` (@ offset 28) and `fogClipDist` (@ offset 32) because the field-decode gate requires `len >= 40`. The size table accepts 36 without a warning, but the separate `>= 40` gate causes silent data loss. Same defect class as the already-fixed Starfield #1291 XCLL-size bug. OpenMW `loadcell.cpp:185` reads exactly 36 bytes for TES4 (`case 36: reader.get(&mLighting, 36)`), confirming both fields are authored in Oblivion data.

**Suggested fix**: split the gate — read `dir_fade`(@28) + `fog_clip`(@32) when `len >= 36`; keep `fog_power` behind `len >= 40` (TES4 doesn't have this). Fix the `walkers.rs:15-16` doc to state the 36-byte TES4 case explicitly.

## Completeness Checks
- [ ] **SIBLING**: verify FO3/FNV XCLL decode path handles the same offset split (they may share the parser)
- [ ] **TESTS**: unit test for a 36-byte Oblivion XCLL asserting dir_fade + fog_clip are populated
- [ ] **CANONICAL-BOUNDARY**: ESM parse-side
- [ ] **UNSAFE**: no unsafe involved
