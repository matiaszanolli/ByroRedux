# Issue #685: O3-N-01: Oblivion WEAP DATA layout collapsed onto FO3/FNV schema — every ItemRecord field after value is wrong

**Severity**: HIGH
**File**: `crates/plugin/src/esm/records/items.rs:147-158`
**Dimension**: ESM (TES4)

`parse_weap` matches `GameKind::Fallout3NV | GameKind::Oblivion | GameKind::Fallout4` together and reads "WEAP DATA (16 bytes): value(i32), health(i32), weight(f32), damage(i16), clip(u8) + pad" — but **Oblivion's WEAP DATA is 15 bytes with completely different fields**:

```
Type(u8) + Speed(f32) + Reach(f32) + Flags(u32) + Value(u32) + Health(u32) + Weight(f32) + Damage(u16)
```

(Per UESP and Gamebryo 2.3 source.) The current code reads `value = bytes 0..4` which on Oblivion would yield `(Type<<24 | Speed_top3)` — a junk u32. `weight` ends up reading Oblivion's `Flags`, etc.

**Bundled finding O3-N-08**: the comment at `items.rs:148-149` says "FO3/FNV WEAP DATA (16 bytes)" without flagging Oblivion's collapse. The comment ambiguity directly enabled this regression.

**Impact**: Every Oblivion WEAP record's `common.value`, `common.weight`, and `Weapon.damage` fields are wrong. No rendering impact today, but any future damage/economy/inventory consumer will see garbage. Same root cause as the AUDIT_OBLIVION_2026-04-17 medium finding "`records/items.rs` 100% FNV-layout — Oblivion WEAP/ARMO DATA offsets differ".

**Fix**:
- Split `GameKind::Oblivion` into its own `match` arm.
- Oblivion DNAM does not exist for WEAP — Oblivion stores all stats inline in DATA.
- Update the per-arm comment to name each game explicitly.

Pairs with O3-N-02 (ARMO) and O3-N-03 (AMMO).

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
