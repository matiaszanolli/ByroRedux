# FO4-D6-02: FO4 ARMO reads weight and health swapped — every armor piece weighs 0.0

**Issue**: #2993
**Severity**: HIGH
**Dimension**: 6 — ESM item records
**Labels**: `high,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO4_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO4_2026-08-16.md` (Dimension 6 — ESM item records).

**Location**: `crates/plugin/src/esm/records/items.rs`:333-339

## Description

The shared `GameKind::Fallout3NV | GameKind::Fallout4` arm reads FO4 ARMO `DATA` as `value(u32), health(u32), weight(f32)`.

xEdit's FO4 ARMO `DATA` is `Value(i32) @0, **Weight(f32) @4**, **Health(u32) @8**`.

**The length matches (12 bytes) but the field order does not**, so the last two fields are read through each other's types.

## Evidence

Measured `Armor_Leather_TorsoE3` — authored `(value 25, weight 5.0, health 0)`; the parser yields:
- `weight = 0.0` (the `health` u32 `0` reinterpreted as `f32`)
- `health = 1084227584` (the raw IEEE-754 bits of `5.0f32`)

`DATA` is 12 B on **688/688** FO4 ARMO records, so this is total, not partial. The same shape reproduces on every sampled record (`Clothes_InstituteLabCoat*`: authored weight 3.0 → parsed weight 0.0, health 1077936128).

Re-verified 2026-08-17: the arm still reads `value, health, weight` for the shared FO3NV|FO4 branch.

## Impact

Every FO4 armor piece has weight 0.0 and a ~1e9 nonsense health. Carry-weight and encumbrance computed off `common.weight` see zero for all worn gear; any consumer of `ItemKind::Armor::health` sees a value nine orders of magnitude out of range.

Unlike the WEAP case (#2992) this fails **silently and plausibly** — 0.0 is a legal weight.

## Suggested Fix

Give FO4 its own arm reading `value, weight, health`; keep FO3/FNV on `value, health, weight`.

## Related

- **`AUDIT_LEGACY_COMPAT_2026-08-16.md` (line 331) explicitly cleared `parse_armo`'s FO4 bucketing as "found correct"** on the strength of the 12-byte length agreeing with the arm's layout. That check compared **sizes, not order**, and is a false negative — worth recording, because "the byte count matches" is exactly the reasoning that will be applied to the four other records in this cluster.
- #2992 (FO4-D6-01, same "FO4 shares the FO3/FNV arm" root)

## Completeness Checks
- [ ] **SIBLING**: Every shared `Fallout3NV | Fallout4` arm in `items.rs` re-checked for **field order**, not just byte length
- [ ] **ORDER-NOT-SIZE**: The verification method compares authored values to parsed values, not struct sizes
- [ ] **FALSE-NEGATIVE**: The legacy-compat report's "found correct" verdict corrected
- [ ] **TESTS**: A regression test asserts a known authored weight round-trips

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2993 --json state` when live state is needed.*
