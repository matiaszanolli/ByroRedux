# ESM-2026-10-31-02: FO76 `EXPL` `DATA` is decoded at the Oblivion/FO3 offsets, and FO76 `PROJ` `DNAM` is not decoded

Labels: low,esm-plugin,bug,game:fo76

Source: `docs/audits/AUDIT_ESM_2026-10-31.md` (ESM-2026-10-31-02). Severity LOW. Dimension: Sub-Record Byte Accounting. Record: `EXPL` / `DATA`, `PROJ` / `DNAM` (FO76).

**Location**: `crates/plugin/src/esm/records/misc/effects.rs:447-475` (`parse_expl` falls through to the legacy `DATA` arm for `Fallout76`); `:236` (the PROJ `DNAM` arm is gated to `Fallout4`). Gap in the per-game gating added by `c36ab4125`.

## Description
- FO76 `EXPL` `DATA` is 84-96 bytes: Damage Curve Table FormID @24, Force @28, Damage @32, Inner @36, Outer @40.
- Only `Skyrim | Fallout4` take the new arm; FO76 reaches the Oblivion/FO3 arm (`damage` @8, `radius` @12) which reads two FormIDs as floats.
- FO76 `PROJ` carries the same 93-byte `DNAM` as FO4 (530 records, `DATA` empty). The arm is gated to `Fallout4`, so every FO76 projectile has zero flags, speed, range, alt-trigger and explosion.

## Evidence
`crExplosionGrenadePumpkin` (v210, 96 B) has `0xd8f99` @0, `0x5303f7` @4, `0x0` @8. The FO3 arm gives `damage = f32(0)`, `radius = f32(0x1f5b0)` ~ 1.1e-40. Real values: Force 550, Damage 1, Outer 500 at @28/@32/@40.

## Impact
None today: the mine path keys on `is_proximity_mine` and no FO76 consumer reads the other fields. Wrong data waiting for a consumer; `ExplRecord` still advertises `damage`/`radius` for FO76.

## Suggested Fix
Add `Fallout76` to the PROJ `DNAM` gate (layout identical). Give `EXPL` an FO76 arm: skip the extra FormID (version-gated, xEdit says 150), then Force @28, Damage @32, Inner @36, Outer @40. Or decline to decode (leave zeros) for games without a sourced layout, as `parse_bptd` did in #6465.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
