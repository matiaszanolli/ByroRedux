# S6-02: Skyrim ARMO/WEAP/AMMO DATA layouts diverge from FNV — items parser produces garbage stats

**Issue**: #347 — https://github.com/matiaszanolli/ByroRedux/issues/347
**Labels**: bug, high, legacy-compat

---

## Severity
**HIGH** (gameplay readiness; renderer is unaffected — uses `statics`, not `items`)

## Location
`crates/plugin/src/esm/records/items.rs:124-318`

## Description
Every items parser hard-codes the FNV/FO3 sub-record schema. Skyrim divergences:

| Field | FNV/FO3 | Skyrim |
|---|---|---|
| ARMO biped flags | `BMDT` (8 bytes) | `BOD2` (8 bytes: biped slot bitfield + armor type) |
| ARMO `DATA` | `value(u32) + weight(f32) + health(u32)` (12 bytes) | `value(u32) + weight(f32)` (8 bytes, **no health**) |
| ARMO `DNAM` | `DT(f32) + DR(u32)` (8 bytes) | `armor_rating × 100` (4 bytes) |
| WEAP `DATA` | `value + weight + health + clip + damage` | 10 bytes (`value + weight + damage`, **no health/clip**) |
| WEAP `DNAM` | one layout | ~100 bytes with completely different field positions |
| AMMO sub-records | various | entirely different (`DATA = projectile_form + flags + damage + value`) |

No game-aware dispatch:
```rust
// items.rs:212-215 (Skyrim has no BMDT)
b"BMDT" if sub.data.len() >= 8 => { ... }

// items.rs:223 — Skyrim DNAM is 4 bytes, not 8
b"DNAM" if sub.data.len() >= 8 => {
    dt = read_f32_at(&sub.data, 0).unwrap_or(0.0);  // garbage on Skyrim
    dr = read_u32_at(&sub.data, 4).unwrap_or(0);    // garbage on Skyrim
}
```

`grep BOD2` → 0 hits in workspace.

## Impact
Cell loading still works (uses `statics: HashMap<u32, StaticObject>` from `parse_modl_group`, not items.rs). But the M24 `EsmIndex.items` map will contain Skyrim entries with zero/garbage stats. Anything downstream that reads damage/AC/clip-size from a Skyrim ARMO/WEAP gets wrong data.

## Suggested Fix
Plumb `EsmVariant` (or a derived `GameKind`) into `parse_esm` and dispatch ARMO/WEAP/AMMO based on it. Add `BOD2` parser for Skyrim biped flags. The `MODL/EDID/FULL` fields of `CommonItemFields` work cross-game; only `DNAM/DATA/biped` need game-aware branching.

## Completeness Checks
- [ ] **SIBLING**: Same `EsmVariant`-aware dispatch needed for ALCH, INGR, BOOK, KEYM, MISC if their DATA layouts also diverge — verify against UESP for each.
- [ ] **TESTS**: Add Skyrim-specific items unit tests with real Skyrim.esm record bytes.
- [ ] **VERIFY**: Run `parse_esm` on `Skyrim.esm` and spot-check 5 known weapons (Iron Sword, Daedric Greatsword, etc.) — values should match in-game.

## Source
Audit `docs/audits/AUDIT_SKYRIM_2026-04-16.md` finding **S6-02**.
