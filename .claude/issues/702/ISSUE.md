# Issue #702: O3-N-10: LIGH DATA test fixture comment at cell.rs:2056 still says 'BGRA' — stale doc post-#389-revert

**Severity**: LOW (one-line doc fix)
**File**: `crates/plugin/src/esm/cell.rs:2054-2056`
**Dimension**: ESM (TES4)

The `build_ligh_record` test helper docstring says "color(BGRA u8×4)" — but the actual test (`parse_ligh_decodes_color_as_rgba` at line 2087) was updated post-#389-revert to confirm RGB. xEdit's `Color { Red; Green; Blue; Unknown }` confirms the on-disk layout is RGB; FNV `OurLadyHopeRed`, `BasementLightKickerWarm` etc. were the disambiguating evidence (every warm/amber/red EDID came out cyan/blue under BGR).

Doc-comment leftovers from the pre-revert era. Cosmetic.

**Fix**: One-line edit — change "BGRA" to "RGBA" / "RGB(unknown)" in the helper docstring.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
