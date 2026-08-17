# FO4-D6-05: FO4 BOOK DATA is read as a 10-byte record when it is 8

**Issue**: #2996
**Severity**: MEDIUM
**Dimension**: 6 — ESM item records
**Labels**: `medium,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO4_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO4_2026-08-16.md` (Dimension 6 — ESM item records).

**Location**: `crates/plugin/src/esm/records/items.rs`:600-607

## Description

`parse_book`'s single `DATA` arm is **un-gamed** and assumes the FNV layout `flags(u8), skill(u8), value(u32), weight(f32)` = 10 bytes.

FO4 BOOK `DATA` is 8 bytes: `Value(u32) @0, Weight(f32) @4`. So:
- `flags` and `skill_bonus` take the low two bytes of the value
- `value` is read from offset 2, **straddling the Value/Weight boundary**
- `weight` runs off the end and defaults to 0

## Evidence

`DATA` 8 B on **327/327** FO4 BOOK records.

xEdit FO4 BOOK: `wbStruct(DATA, 'Data', [wbInteger('Value', itU32), wbFloat('Weight')])`. The FO4 skill/perk teach data lives in the 13-byte `DNAM`, not in `DATA`.

Re-verified 2026-08-17: the `b"DATA"` arm has no `match game` at all — it reads `u8, u8, u32, f32` unconditionally.

## Impact

All 327 FO4 books carry a garbage value composed of two bytes of the real value and two bytes of the weight's mantissa, plus weight 0 and meaningless `flags` / `skill_bonus`.

## Suggested Fix

Add a `GameKind::Fallout4` arm reading `value(u32), weight(f32)`, and add a `DNAM` arm if the skill/perk teach data is wanted.

The deeper fix is that this arm is the only un-gamed `DATA` in the file — a per-game match here matches the house pattern of every sibling parser.

## Related

- #2992 (FO4-D6-01 — same cluster; the shared real-data assertion catches this one too)

## Completeness Checks
- [ ] **SIBLING**: Any other un-gamed `DATA`/`DNAM` arm in `items.rs` given a per-game match
- [ ] **REAL-DATA-GUARD**: Covered by the uniformly-zero assertion proposed in #2992
- [ ] **TESTS**: A regression test pins a known FO4 book value and weight

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2996 --json state` when live state is needed.*
