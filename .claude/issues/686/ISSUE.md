# Issue #686: O3-N-02: Oblivion ARMO DATA shifted by one field vs FO3/FNV — armor reads as value, value reads as health, etc.

**Severity**: HIGH
**File**: `crates/plugin/src/esm/records/items.rs:250-269`
**Dimension**: ESM (TES4)

Oblivion ARMO DATA per UESP TES4 is **16 bytes**: `armor(u32) + value(u32) + health(u32) + weight(f32)`. The current parser groups Oblivion with FO3/FNV at the 12-byte arm reading `(value, health, weight)` from offsets `(0, 4, 8)` — **shifted by one field** for Oblivion.

**Concretely**:
- Oblivion `armor` rating gets stored as ItemRecord `value` (because it's at the right offset for FO3/FNV's value field).
- Real Oblivion `value` lands in ItemRecord `health`.
- Real Oblivion `health` lands in ItemRecord `weight` (truncated).
- Real `weight` is dropped.

ARMO DNAM does not exist on Oblivion; the FO3/FNV `dt`/`dr` extraction at `items.rs:271-289` runs against Oblivion's BMDT-only records and reads zeros (acceptable accidentally), but the DATA field shift means the gameplay/loot values are uniformly displaced.

**Impact**: Oblivion armor rating, value, health, and weight all displaced. Non-rendering, but breaks any downstream loot / vendor / economy lookup.

**Fix**: Add a separate `GameKind::Oblivion` ARMO DATA arm reading `armor → strength_x100, value, health, weight` at offsets `(0, 4, 8, 12)`.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
