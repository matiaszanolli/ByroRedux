# #3714 — ESM-2026-08-30-D3-01: parse_armo, parse_arma and parse_race all bypass the load-order remap — every DLC-/ESL-added FO4 armor resolves to zero meshes

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: HIGH · **Dimension**: FormID & Load Order
**Record / Sub-record**: `ARMO`/`MODL`, `ARMA`/`RNAM`+`MODL`, `RACE`/`WNAM`
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D3-01)

**Location**
- `crates/plugin/src/esm/records/items.rs` — `parse_armo`'s `b"MODL" if is_skyrim_or_later` arm pushes armature FormIDs raw (~:538-541)
- `crates/plugin/src/esm/records/misc/equipment.rs` — `parse_arma` takes **no `remap` parameter at all** (~:67); `RNAM` (~:99), `MODL` additional races (~:105)
- `crates/plugin/src/esm/records/actor/mod.rs` — `parse_race` takes **no `remap`** (~:1428); `WNAM` default skin (~:1739), `XNAM` (~:1722)
- Consumers: `crates/plugin/src/equip.rs` (~:245-285); `byroredux/src/npc_spawn.rs:806, :819, :935`

**Status note**: same defect class as **#3400** (CLOSED, filed HIGH). The `05bdb969` sweep's touched-file list contains neither `items.rs` nor `misc/equipment.rs`, and `parse_race` was untouched — so the premise that "the remap sweep is finished for the `records/` tier" is incomplete.

## Description

`EsmReader::read_record_header` remaps every record's own FormID, so `EsmIndex.armor_addons` / `.items` / `.races` are keyed in **global** space. The three parsers above read *embedded* FormIDs **raw**. Every lookup in the armor-equip chain — `RACE.WNAM` -> `ARMO.MODL` (armature list) -> `ARMA.RNAM` (race match) — therefore misses the moment the remap is non-identity, which it is for the second and later plugin of any multi-master load order and for every ESL by construction.

## Evidence

- `crates/plugin/src/esm/records/items.rs` — `parse_armo` **does** receive `remap: &Option<FormIdRemap>`, yet the armature push bypasses it:
  ```rust
  b"MODL" if is_skyrim_or_later => {
      if let Ok(id) = SubReader::new(&sub.data).u32() {
          armatures.push(id);          // raw, un-remapped
      }
  }
  ```
- `actor/mod.rs:1012` — NPC side **is** remapped: `record.race_form_id = remap_fid(raw, remap);`
- `misc/equipment.rs:99` — ARMA side is **not**: `out.race_form_id = SubReader::new(&sub.data).u32_or_default();`
- `equip.rs:246`/`:277` — `index.armor_addons.get(&arma_fid)` with the **raw** id from `ARMO.MODL`; on a miss the loop `continue`s, and `resolve_armor_meshes` ends `Vec::new()`.
- `equip.rs:250` — `arma.race_form_id == race_form_id`, where `race_form_id` is the **remapped** `npc.race_form_id`.

**Census over the shipped FO4 DLC masters.** Each declares exactly one master, so its own forms are authored `0x01…` while `allocate_global_slot` keys them `0x02…` — the exact non-identity case #3400 names:

| plugin | ARMO `MODL` armature refs, self-ref / total | ARMA `RNAM` self-ref / total | RACE `WNAM` self-ref / total | `NPC_` `RNAM` self-ref / total |
|---|---|---|---|---|
| DLCRobot.esm     |  26 / 35  |  4 / 31  |  2 / 6  |  58 / 235 |
| DLCCoast.esm     |  97 / 137 | 31 / 94  | 11 / 15 |  83 / 378 |
| DLCNukaWorld.esm | 203 / 236 | 32 / 196 |  7 / 15 | 115 / 691 |

**326 of 408** armature references across the three DLCs point at an ARMA the same plugin adds. Plus 67 ARMA race refs and 20 RACE skin refs self-referencing.

## Impact

On `--master Fallout4.esm --esm DLCNukaWorld.esm` (and on every ESL, where the remap is non-identity by construction) DLC- and mod-added armor renders as **empty space** — indistinguishable from an unshipped mesh, the same user-visible symptom #3400 was filed for. Where the ARMA *does* resolve, the un-remapped `ARMA.RNAM` still fails the race match for all 67 DLC-added-race addons, dropping the actor into the deliberately single-valued pass-2 fallback — the exact multi-addon collapse #3357 fixed. References to *base-game* races/ARMAs are unaffected (`mod_index` 0 -> slot 0, identity), which is why single-plugin vanilla loads look correct.

## Related

#3400, #3401, #3314, #3357; and the successor allowlist finding filed alongside this one.

## Suggested Fix

Thread `remap: &Option<FormIdRemap>` into `parse_arma` and `parse_race`, wrap `RNAM` / `WNAM` / `XNAM` / the additional-race `MODL` push, and wrap `items.rs`'s `armatures.push(id)` in the `remap_fid` `parse_armo` already receives. Then add all three parsers to the `record_parsers_with_embedded_form_ids_take_a_remap` allowlist (`crates/plugin/src/esm/records/tests.rs`).

Follow-up worth scoping alongside: `docs/smoke-tests/m41-equip.sh` should grow a DLC-master case — it would have caught this.

## Completeness Checks
- [ ] **SIBLING**: Every other embedded-FormID read in `items.rs` / `misc/equipment.rs` / `actor/mod.rs` checked in the same pass
- [ ] **TESTS**: A regression test pins the multi-master case (a plugin whose remap is non-identity resolving an armature it added)
