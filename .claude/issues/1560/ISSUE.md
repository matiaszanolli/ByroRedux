**Severity**: LOW · **Dimension**: NPC Equip + FaceGen (M41)
**Location**: `docs/smoke-tests/m41-equip.sh:186-195`
**Source**: AUDIT_SKYRIM_2026-06-14 (SK-D3-02)

## Description
The only HARD assertions in the M41 equip smoke test are cell-wide `entities`/`draws` floors. The actual equip signals — `Inventory` / `EquipmentSlots` entity counts — are emitted as WARN only and never affect the exit code. The named-NPC count (6: saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda) is never asserted anywhere. A regression dropping all NPC gear would still pass as long as the static-mesh count stays above its floor.

## Evidence
`m41-equip.sh:186-195` — zero-Inventory / zero-EquipmentSlots emit `echo … WARN`, not a non-zero exit (`hard_fail` is untouched in those branches); no reference to the 6 NPC names or the count 6 in the test.

## Impact
The one smoke test that exercises the full outfit chain wouldn't catch a silent equip regression. (The equip code itself is correct — verified clean in the audit.)

## Suggested Fix
Promote the zero-Inventory / zero-EquipmentSlots WARN to a HARD fail with a small floor (e.g. `>= 6` entities carrying both components).

## Completeness Checks
- [ ] **TESTS**: The smoke test exits non-zero when Inventory/EquipmentSlots fall below the floor
