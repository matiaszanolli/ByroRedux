# #3716 — ESM-2026-08-30-D2-01: Skyrim BOOK.DATA is decoded with the 10-byte FNV layout against a 16-byte record

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: Sub-Record Byte Accounting
**Record / Sub-record**: `BOOK` / `DATA`
**Location**: `crates/plugin/src/esm/records/items.rs` (`parse_book`, the `Oblivion | Fallout3NV | Skyrim | Fallout76 | Starfield` DATA arm, ~:834-857)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D2-01)

## Description

The arm groups `Oblivion | Fallout3NV | Skyrim | Fallout76 | Starfield` and decodes `flags(u8), skill(u8), value(u32), weight(f32)` = **10 bytes**, with **no length guard**. Its own comment concedes the shape: *"Preserve the existing decode for the other families until their BOOK schemas receive dedicated coverage."* Skyrim's record is **16** bytes.

## Evidence

Census over `Skyrim.esm` — all **821** BOOK `DATA` sub-records are exactly 16 bytes. Sample payloads:

```
04000000 ecdd1000 da020000 0000803f
04000000 eef71000 ee020000 0000803f
```

Decoding all 821 both ways:

| reading | `value` range | `weight` range |
|---|---|---|
| current (`value`@2, `weight`@6) | 0 … **4 294 901 760** (`0xFFFF0000`) | 0 … 4.74e-33 (denormal noise) |
| offsets 8 / 12 | 0 … **2 500** | 0 … **20.0** |

The 8/12 reading is self-evidently the authored one — book prices and weights. The bytes at 4..8 are FormID-shaped (`0x0010DDEC`, `0x0010F7EE`): the skill-book "Teaches" reference, which the current arm swallows as part of `value`. No spec citation is needed; the two candidate offsets differ by six orders of magnitude in plausibility. **No test pins the current behaviour**, so this is an open gap, not a settled decode.

## Impact

Every Skyrim/SSE book in `EsmIndex.items` carries a 4-billion-scale `value` and a ~0 `weight`. Any consumer doing economy or carry-weight math on Skyrim data gets nonsense, and a `u32` near `u32::MAX` will wrap or saturate downstream.

## Suggested Fix

Give `GameKind::Skyrim` its own arm gated on `len() >= 16` (`flags u8, type u8, unknown u16, teaches u32, value u32, weight f32`), routing `teaches` into the existing `teaches_skill` field — currently fed only by `SKIL`, which Skyrim does not emit. Add a length guard to the 10-byte arm so a 16-byte record can never take it. **FO76 / Starfield need their own census before being moved.**

## Completeness Checks
- [ ] **SIBLING**: The other unguarded item `DATA` arms (`WEAP`/`ARMO`/`AMMO`) checked for the same over-broad game grouping
- [ ] **TESTS**: A regression test pins a real 16-byte Skyrim BOOK `DATA` payload to plausible value/weight
