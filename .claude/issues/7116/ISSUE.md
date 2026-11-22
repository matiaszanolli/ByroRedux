# #7116 — ESM-D7-04: `ItemKind::Armor::slot_mask` is a dead `u16` that truncates the u64 biped mask; doc says FO3/FNV-only

Labels: low,esm-plugin,tech-debt,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D7-04)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D7-04**

- **Severity**: LOW (dead field; latent trap)
- **Dimension**: 7 (equip.rs biped-slot constants)
- **Record / Sub-record**: ARMO `BMDT` / `BOD2` / `BODT` / `BO64`
- **Location**: `crates/plugin/src/esm/records/items.rs:190-191` (field and doc "Equip slot mask (FO3/FNV-only biped-flag low 16 bits)"), `:750`, `:781`, `:796`, `:811`, `:824` (writes `(biped_flags & 0xFFFF) as u16` in every arm, including `BO64`)
- **Status**: NEW (`9405138e9` widened `biped_flags` to `u64` and kept this field)
- **Description**: `slot_mask` is assigned in all four decode arms for every game, so its "FO3/FNV-only" doc is wrong, and on Starfield it keeps only the clothing layer bits 0-15 of a 64-bit mask. Nothing reads it. `rg 'slot_mask'` over `crates`, `byroredux` and `tools`: the definition, the four writes, and constructors with `slot_mask: 0` in tests (`equip.rs`, `armor.rs`, `combat.rs`, `skill_use_emit.rs`, `loot_appearance.rs`); every consumer destructures `biped_flags` (`inventory/catalog.rs:647,780`, `npc_spawn/equip.rs:259,386`, `scene_runtime.rs:123`, `armor.rs:77`). The unrelated `slot_mask` in `crates/scripting/src/equipment.rs` and `crates/core/.../inventory.rs` are `u64` parameters, not this field.
- **Impact**: none today; a future reader of `slot_mask` would get a 16-bit view that silently drops Starfield's high bits and FO76/FO4 bits above 15.
- **Suggested fix**: delete the field and its five writes (and the `slot_mask: 0` constructor lines), or, if kept for a reason, document it as "legacy low 16 bits" and gate the write.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
