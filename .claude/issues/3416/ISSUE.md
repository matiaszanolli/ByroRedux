# #3416 — FNV-2026-08-27-D4-01: FO3/FNV ARMO `MOD3` (the female biped mesh) is never parsed

**Labels**: high, bug, esm-plugin, character, game:fnv, legacy-compat

**Filed**: 2026-08-27 · from `docs/audits/AUDIT_FNV_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-27.md` — finding `FNV-2026-08-27-D4-01` (HEAD `969d81c8`)

- **Severity**: HIGH
- **Dimension**: 4 — ESM Record Parser (FNV data through it)
- **Location**: `crates/plugin/src/esm/records/common.rs:288` (only `MODL` is read) · `crates/plugin/src/equip.rs:169-178` (the pre-Skyrim arm ignores its own `gender` parameter) · consumed at `byroredux/src/npc_spawn.rs:803-807` and `:918-923`

## Description

On Oblivion / FO3 / FNV an ARMO carries **two** worn meshes: `MODL` (male biped) and `MOD3` (female biped). `MOD3` is parsed in exactly one place in the plugin crate — `parse_arma` (`crates/plugin/src/esm/records/misc/equipment.rs:120`) — which handles **ARMA** records, a Skyrim+ dispatch that FNV NPC inventories never reference. For ARMO, `CommonNamedFields::from_subs_with_remap` captures `MODL` into `model_path` and silently drops `MOD3`, and `resolve_armor_meshes`'s legacy arm returns that single path regardless of gender.

## Evidence

`crates/plugin/src/esm/records/common.rs:285-289`:

```rust
b"EDID" => out.editor_id = read_zstring(&sub.data),
b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
b"MODL" => out.model_path = read_zstring(&sub.data),
b"ICON" => out.icon_path = read_zstring(&sub.data),
```

`crates/plugin/src/equip.rs:169-178` — note that `gender` is bound in the signature and never used on this branch:

```rust
if !is_skyrim_or_later {
    // Oblivion / FO3 / FNV: ARMO MODL is the worn mesh. One record,
    // one mesh — no ARMA dispatch, so never more than one path.
    let path = armor.common.model_path.as_str();
    return if path.is_empty() {
        Vec::new()
    } else {
        vec![path]
    };
}
```

`grep -rn MOD3 crates/plugin/src/` hits only `records/misc/equipment.rs` (the ARMA parser) and two doc comments — never the ARMO path.

Direct census of `FalloutNV.esm` (independent Python GRUP walker, this audit): **389 ARMO records; 245 author a `MOD3`; 213 of those differ from the record's `MODL`.** Samples:

| EDID | `MODL` (used) | `MOD3` (dropped) |
|---|---|---|
| `ArmorWhiteGloveSociety` | `armor\tuxedo\tuxedo_M.NIF` | `armor\tuxedo\tuxedo_F.NIF` |
| `ArmorCombatReinforcedMark2` | `armor\combatarmor\m\mark2combat.NIF` | `armor\combatarmor\f\mark2combatf.NIF` |
| `ChineseStealthArmor` | `dlcanch\armor\chinesestealtharmor\m\outfit.nif` | `dlcanch\armor\chinesestealtharmor\f\outfit.nif` |
| `OutfitGeneralOliver` | `armor\GeneralOliver\GeneralOliverM.NIF` | `armor\GeneralOliver\GeneralOliverF.NIF` |
| `NVPapaKhanArmor` | `armor\papakhan\papakhan.NIF` | `armor\papakhan\papakhan_f.NIF` |

Reach: `FalloutNV.esm`'s NPC_/CREA `CNTO` lists resolve to `ARMO` 1 507 times directly and to `LVLI` 13 974 times (whose leaves expand through `expand_leveled_form_id` into the same ARMO records). 987 of 3 816 `NPC_` records set the `ACBS` female bit. A census of every `NPC_`/`CREA` `CNTO` target in `FalloutNV.esm` (5 394 actors) found **zero ARMA**, so the Skyrim-shaped ARMA equip path is genuinely unreachable on FNV and the fix must land on the ARMO arm.

## Impact

Every female FNV NPC wearing one of those 213 armors renders the male mesh — wrong silhouette, and in the `m\` / `f\` cases a mesh whose dismember partitions and UVs were authored against the male body. This is not a missing asset (`_F.NIF` is present in `Fallout - Meshes.bsa`) and not a renderer issue; it is one unparsed sub-record. It applies to Oblivion equally — the same branch serves both — but FNV is the reference title.

## Related

#3357 (`e0d5ec18`) reworked the *Skyrim+* half of this exact function into a multi-mesh resolver and left the legacy arm untouched; `resolve_armor_meshes`' Skyrim path is already fully gender-aware via `ArmaRecord::{male_biped_model, female_biped_model}`. Sibling gender defect on the head path: FNV-2026-08-27-D4-02. The *body* path (#3037) is already fixed and gender-aware.

## Suggested Fix

Parse `MOD3` on the ARMO path into a `female_model_path` beside `model_path` (the FO3/FNV `ItemKind::Armor` variant is the natural home, so `CommonNamedFields` stays game-neutral), then have `resolve_armor_meshes`'s pre-Skyrim arm select on `gender` and fall back to `MODL` when `MOD3` is empty (144 of 389 FNV ARMOs author none). Pin it with a real-data test over `FalloutNV.esm` asserting a female actor resolves `tuxedo_F.NIF`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (Oblivion ARMO travels the same branch; check CLOT/other worn-item kinds that also author `MOD3`)
- [ ] **TESTS**: A regression test pins this specific fix
