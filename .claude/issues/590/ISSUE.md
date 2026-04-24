# #590: FO4-DIM6-05: COBJ / OMOD / MSWP / CMPO / FLST records not parsed — FO4 weapons, armor variants, settlement crafting silently incomplete

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/590
**Labels**: bug, medium, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 6)
**Severity**: MEDIUM
**Location**:
- `RecordType::COBJ/OMOD/MSWP/CMPO/FLST` constants exist at `crates/plugin/src/record.rs:158-164`.
- `esm/cell.rs:518-521` MODL-group match arm has no dispatch for any of them.
- `esm/records/mod.rs` has no modules for them.
- `EsmIndex` has no field for any.

## Description

Five FO4 record types are defined in `RecordType` but not parsed anywhere:

| Record | Purpose | Vanilla count (approx) |
|---|---|---|
| **COBJ** (Constructible Object) | Settlement crafting recipes (CNAM → output, CTDA conditions, BNAM workbench keyword) | ~1500 |
| **OMOD** (Object Modification) | Weapon/armor mods. `Weapon.BMOD` default-attached OMODs + `MOD2` AMMO-alt keywords | ~2000 |
| **MSWP** (Material Swap) | Per-REFR texture-set substitution (Raider Armor 12 color schemes from one mesh) | ~1200 |
| **CMPO** (Component) | Crafting ingredients (refined materials) | ~80 |
| **FLST** (Form List) | Flat FormID list — used by every "accepts any of N" slot (quest objectives, ammo types, workbench output) | ~2500 |

## Impact

- **MSWP** is the highest-value gap after BGSM scalars — vanilla FO4 interiors use MSWP on NPCs, Raider corpses, settlement clutter, and power armor frames. Without MSWP, every Raider armor looks identical.
- **OMOD**: a 10mm pistol with Short Barrel + Reflex Sight OMODs renders as bare.
- **FLST**: breaks every FormList reference (quest-target collections, workbench recipe outputs, ammo-group references).
- **COBJ / CMPO**: needed for any "open workbench UI" feature; deferrable.

## Suggested Fix

Split this issue into per-record sub-issues once triaged; recommended order:

1. **MSWP** first (single-purpose: FormID array of TextureSet pairs) — unblocks visual variety.
2. **FLST** (flat FormID list — minimal parser; wide reach).
3. **OMOD** (weapon/armor mod data — follows existing `items.rs` pattern).
4. **COBJ / CMPO** (crafting recipes / components — deferrable until workbench UI).

All follow the existing pattern (`crates/plugin/src/esm/records/items.rs` is the template for WEAP/ARMO/AMMO). Add per-record module under `esm/records/`, populate on `EsmIndex`, resolve at cell-load time for REFR-XMSP overrides (coordinates with FO4-DIM6-02 stage 2).

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Existing WEAP/ARMO/AMMO/ALCH/INGR parsers in `items.rs` are the template
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Corpus regression — assert vanilla FO4 yields ≥1200 MSWP, ≥2000 OMOD, ≥2500 FLST records.

## Related

- FO4-DIM6-02 (TXST override consumption) depends on MSWP for `XMSP` REFR sub-record resolution.
