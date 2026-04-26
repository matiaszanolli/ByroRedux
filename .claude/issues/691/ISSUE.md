# Issue #691: O3-N-03: Oblivion AMMO has no AMMO DATA shape match — clip_rounds reads garbage from low byte of weight

**Severity**: MEDIUM
**File**: `crates/plugin/src/esm/records/items.rs:319-339`
**Dimension**: ESM (TES4)

`parse_ammo` matches `Oblivion` alongside FO3/FNV/FO4 in the `b"DATA"` arm, but Oblivion AMMO uses `ENAM` (Enchantment FormID) + `ANAM` + `DATA`. UESP-listed Oblivion AMMO DATA is **13 bytes** (`speed(f32), flags(u8), pad(3), value(u32), weight(f32)`) — close to FO3/FNV's `(speed, flags, pad, value, clipRounds(u8))` but the trailing field differs (weight f32 vs clip_rounds u8).

The FO3/FNV arm reads `clip_rounds = sub.data[12]` which would land in Oblivion's `weight` low byte.

**Impact**:
- Oblivion AMMO `clip_rounds` reads garbage from low byte of `weight`.
- ENAM enchantment ref dropped.
- Non-rendering.

**Fix**: Per-game arm; AMMO is the kind of record where Oblivion shape differs slightly — reuse FO3/FNV up through `value`, then read `weight` not `clip_rounds`.

Pairs with #685 (WEAP) and #686 (ARMO) — same root cause.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
