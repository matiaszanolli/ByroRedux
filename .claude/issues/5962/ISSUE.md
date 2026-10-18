# SKY-D3-2026-10-17-01: M41 smoke equip floor of 6 is now met by five NPCs plus the player body

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5962 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (report ID SKY-3-01) · **Severity**: LOW (test-gap) · **Dimension**: 3 · **Game**: Skyrim SE

## Description
`docs/smoke-tests/m41-equip.sh` hard-asserts an equip floor of 6 on WhiterunBanneredMare (`:192-199`, `:248-252`), set by #1560 for six named residents: saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda. Amaund Motierre's only placement is `ACHR 0004FA2F` (base `0004E64F`) with header flags `0x00000C00`, so bit `0x800` (Initially Disabled) is set; since `f87490826` (#4813) he is correctly withheld. The sixth `EquipmentSlots` entity is now the player body.

## Evidence
Smoke run (`m41-equip.sh skyrim`, PASS): `EquipmentSlots=6` -> entities 2 "saadia", 246 "brenuin", 482 "mikael", 747 "sinmir", 1022 "hulda", and 5570. Entity 5570 inspected via `byro-dbg`: it carries `ActorValues`, `Perks`, `RigidBodyData`, `EquippedWeapon`; the engine log shows `Player body: assembled from NPC_ 00000007`. The engine log lists `NPC` equip lines for Saadia, Hulda, Brenuin, Mikael, Sinmir and the Player only. Entity 737, the unnamed entity with `Inventory` but no `EquipmentSlots`, is a container holding 100 gold, not an NPC. The Initially Disabled flag was read from `Skyrim.esm` with a byte scan of ACHR records whose NAME is `0004E64F`.

## Impact
A regression that drops one NPC's gear still passes: the guard has lost one NPC of margin.

## Suggested fix
Count only entities without player components (or lower the floor to 5 NPCs and assert the player separately), and correct the comments to five residents.

## Completeness Checks
- [ ] **SIBLING**: FO4 cell floors in the same script checked for the same player-body inflation
- [ ] **TESTS**: The corrected floor fails when one named NPC's `EquipmentSlots` is removed
