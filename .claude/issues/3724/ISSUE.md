# #3724 — ESM-2026-08-30-D2-03: the item DATA arms dispatch on GameKind alone with no length validation

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: Sub-Record Byte Accounting (hardening)
**Record / Sub-record**: `WEAP`/`ARMO`/`AMMO`/`BOOK` `DATA`, `CRDT`, `DNAM`, `ENIT`
**Location**: `crates/plugin/src/esm/records/items.rs` (~:260, :413, :492, :558, :621, :780, :834)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D2-03)

## Description

These are the only fixed-layout multi-field arms in the crate with **no length guard at all**; every other decoder gates on `sub.data.len()`. They rely entirely on `*_or_default` leniency, which is safe against truncation-to-zero (a failed read does not advance the cursor) but **not** against a partial truncation followed by a narrower field.

Worked case: a 13-byte FO3/FNV `WEAP DATA` makes `damage = r.u16_or_default()` fail and then `clip_size = r.u8_or_default()` consume `damage`'s stray byte.

## Evidence

A static scan found **31** such narrowing pairs in the crate; **23** sit in arms guarded to the exact struct width and are unreachable, **8** are in these unguarded arms. No vanilla master triggers it (all lengths are exact — see the Dim-2 census in the report), so this is hardening.

## Impact

Hardening only today. Adding the measured guards also makes a wrong-`GameKind` dispatch fail **visibly** instead of silently, which is the concern of the `GameKind::from_header` doc-rot finding filed alongside this one.

## Suggested Fix

Add a `sub.data.len() >= N` guard to each of the seven arms, using the measured on-disk width per game (the report's Dim-2 census supplies them).

## Completeness Checks
- [ ] **SIBLING**: All seven listed arms guarded in one pass, not just the `WEAP` case
- [ ] **TESTS**: A regression test feeds a partially-truncated `WEAP DATA` and asserts no field-shift
