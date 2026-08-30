# #3723 — ESM-2026-08-30-D2-02: Skyrim AMMO.DATA is 20 bytes on disk, not the 16 the decoder's comment claims

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: Sub-Record Byte Accounting
**Record / Sub-record**: `AMMO` / `DATA`
**Location**: `crates/plugin/src/esm/records/items.rs` (the `Skyrim | Fallout76 | Starfield` AMMO DATA arm, ~:648-657)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D2-02)

## Description

The comment reads *"Skyrim AMMO DATA (16 bytes): projectile_form(u32), flags(u32), damage(f32), value(u32)"*. Census over `Skyrim.esm`: **35/35** records are **20** bytes.

The decoded first 16 are correct; bytes 16..20 are an undecoded `f32` whose value is `0.1` in every one of the 35 records — consistent with an authored per-arrow weight, which `common.weight` currently leaves at `0` (`CommonItemFields` sets weight from no sub-record on this path).

## Impact

Small and uniform, but the comment is factually wrong about the on-disk size, which is the kind of stale schema note that misleads the next field addition — and a real (if tiny) weight field is dropped.

## Suggested Fix

Correct the comment and read the trailing `f32` behind a `remaining() >= 4` check so a genuine 16-byte Skyrim LE record still decodes. FO76 / Starfield share this arm and need their own census before being assumed 20-byte.

## Completeness Checks
- [ ] **SIBLING**: The other Skyrim-grouped item `DATA` arms checked against a real census of on-disk lengths
- [ ] **TESTS**: A regression test pins a real 20-byte Skyrim AMMO `DATA` payload including the trailing weight
