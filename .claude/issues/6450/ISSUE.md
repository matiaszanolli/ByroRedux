# ESM-2026-10-27-03: FNV `WEAP` `VATS` sub-record is the melee VATS special attack; `DNAM`@68 is the Override AP

Labels: low, esm-plugin, bug, game:fnv, game:fo3
Source: docs/audits/AUDIT_ESM_2026-10-27.md

**Report**: `docs/audits/AUDIT_ESM_2026-10-27.md` · **Finding**: ESM-2026-10-27-03 · **Severity**: LOW · **Dimension**: Sub-Record Byte Accounting
**Location**: `crates/plugin/src/esm/records/items.rs:450-497` (`DNAM`@68 -> `ap_cost`; `VATS`@12 -> `WeaponVats::ap_override`); consumer `byroredux/src/systems/vats.rs:256-266` (`resolve_weapon`, `v.ap_override > 0.0`)
**Games**: FNV (`VATS` sub-record), FO3/FNV (`DNAM`)

## Description
- xEdit FNV `VATS` (`wbDefinitionsFNV.pas:8881-8889`) is *Effect (SPEL), Skill, Dam. Mult, AP, Silent, Mod Required*.
- All 45 FalloutNV.esm weapons with a non-zero `VATS` AP are melee weapons (Super Sledge, Ripper, Lead Pipe, knives, machetes). All have skill requirement 50, damage multipliers 0.5-2.0, and `MaulerKnockdownSpell` / `VictoryRifleKnockdownSpell` effects: the FNV melee VATS **special-attack** block.
- xEdit names `DNAM`@68 "Override - Action Points", gated by Flags 2 `0x8` "Override - Action Points" (@56). On FalloutNV.esm @68 is non-zero on exactly the 203 records with the bit set; the 58 without it read 0. Fallout3.esm shows the same pattern (117 with the bit, 43 without, @68 = 0 on all 43).
- The code calls @68 the "standard cost" and the `VATS` AP the "Override-tab value".
- `resolve_weapon` prefers `ap_override` when above 0, so a melee VATS attack would be charged its special-move cost (super sledge 48 instead of 38).

## Evidence
`items.rs` DNAM arm: comment "Offset 68 - Action Points (f32), the standard VATS cost"; `VATS` arm: "Offset 12 - the Override-tab 'Action Points'". `systems/vats.rs:260`: `vats.filter(|v| v.ap_override > 0.0).map(|v| v.ap_override)`.

## Impact
None on ranged weapons today (no ranged weapon authors a `VATS` AP, and the values themselves are right). The labels and docs are inverted and the override flag is ignored. Becomes live when melee VATS lands.

## Related
ESM-2026-10-27-01, -02; CHAR-2026-10-26-M-01 (#6422, same `resolve_weapon` path); #6350 (tracker).

## Suggested Fix
- Rename the `VATS` field to the special-attack AP and keep it out of the standard cost.
- Document @68 as the Override AP, honoured when Flags 2 `0x8` is set, with the engine default otherwise (the code already behaves this way because 0 falls back to the default).

## Completeness Checks
- [ ] **SIBLING**: `WeaponVats` consumers and docs (`esm-records.md`) updated for the renamed field
- [ ] **TESTS**: A regression test pins DNAM@68 / Flags 2 `0x8` and the melee `VATS` special-attack reading

