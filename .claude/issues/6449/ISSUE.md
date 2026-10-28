# ESM-2026-10-27-02: WEAP `DNAM`@28 is Sight FOV, not Base VATS To-Hit Chance

Labels: medium, esm-plugin, bug, game:fnv, game:fo3
Source: docs/audits/AUDIT_ESM_2026-10-27.md

**Report**: `docs/audits/AUDIT_ESM_2026-10-27.md` · **Finding**: ESM-2026-10-27-02 · **Severity**: MEDIUM · **Dimension**: Sub-Record Byte Accounting
**Location**: `crates/plugin/src/esm/records/items.rs:442-449` (`parse_weap` `DNAM` arm); consumer `byroredux/src/systems/vats.rs:245-266` (`resolve_weapon`)
**Games**: FO3 / FNV (`WEAP` / `DNAM`)

## Description
xEdit labels the `DNAM` offsets the same way in both games (`wbDefinitionsFNV.pas:8749-8755`, `wbDefinitionsFO3.pas:7581-7584`):

| Offset | Field |
|---|---|
| {16} | Min Spread |
| {20} | Spread |
| {24} | Unused |
| {28} | **Sight FOV** (`f32`) |
| {32} | Unused |
| {36} | Projectile FormID |
| {40} | **Base VATS To-Hit Chance** (`u8`) |

The decoder stores @28 as `vats_base_to_hit` (`ItemKind::Weapon`). The commit cites "wiki tables", but no definition source is given for the offset.

## Evidence
FalloutNV.esm, measured with an independent walker:

| Weapon | @28 `f32` | @40 `u8` |
|---|---|---|
| 10mm pistol | 65 | 15 |
| Sniper rifle | 25 | 16 |
| Laser rifle | 40 | 45 |
| Hunting rifle | 38 | 42 |
| Minigun | 45 | 45 |
| Super sledge | **0** | 45 |
| Ripper | **0** | 27 |
| Lead pipe | **0** | 21 |

- Every melee weapon has @28 = 0. Flamers and missile launchers are 55; Miss Fortune's gun and the Mesmetron are 70.
- The GECK wiki *Weapons* page defines Sight FOV as "the angle of view when using iron sights ... 0 essentially disables iron sights". Its Base VATS To-Hit Chance "is set based upon the size of the weapon", which matches @40 (pistol 15, minigun and sledge 45).
- @36 resolves to a `PROJ` record on all 173 non-null FNV weapons, so xEdit's offsets in this blob are aligned.

## Impact
- `vats_base_to_hit` is the first factor of `VatsModel::to_hit_chance` (`crates/core/src/character/vats.rs:351`).
- Every ranged weapon gets its zoom angle as its VATS accuracy: a scoped sniper rifle (25) is priced worse than a 10mm (65).
- Any melee weapon that reaches the scan has a base chance of 0. The scan has no melee filter; only its doc says "ranged weapons only".

## Related
ESM-2026-10-27-01 (same formula's part term). Real-data pins `parse_real_esm.rs:2046-2071` (`weapon_stat` table) encode the @28 reading. The GECK note that Base VATS To-Hit is the chance "to hit the weapon when it's held by another actor" should be weighed by `/audit-character` before the field is wired into the player's formula. #6350 (tracker), #6403.

## Suggested Fix
- Decode @28 as `sight_fov` and @40 (`u8`) as `vats_base_to_hit`.
- Have CHARAL decide whether that field is a factor in the player-attack formula at all.
- Re-pin the real-data table.

## Completeness Checks
- [ ] **SIBLING**: Remaining `DNAM` offsets (@16/@20/@36/@68) re-checked against xEdit while here
- [ ] **TESTS**: Real-data pin `parse_real_esm.rs:2046-2071` re-pinned; synthetic `DNAM` layout test added

